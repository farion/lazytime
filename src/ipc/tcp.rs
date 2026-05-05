use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader as StdBufReader, Write};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcMessage {
    pub r#type: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcAck {
    pub status: String,
    pub error: Option<String>,
}

pub async fn run_ipc_server(endpoint: &str, tx_reload: mpsc::Sender<String>) -> Result<()> {
    let listener = TcpListener::bind(endpoint)
        .await
        .with_context(|| format!("failed to bind ipc tcp endpoint {}", endpoint))?;

    loop {
        let (stream, _) = listener.accept().await?;
        let tx_reload = tx_reload.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_client(stream, tx_reload).await {
                tracing::warn!("ipc tcp client error: {err}");
            }
        });
    }
}

async fn handle_client(stream: TcpStream, tx_reload: mpsc::Sender<String>) -> Result<()> {
    let (reader_half, mut writer_half) = stream.into_split();
    let mut reader = BufReader::new(reader_half);
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    let msg = serde_json::from_str::<IpcMessage>(line.trim());

    let ack = match msg {
        Ok(message) if message.r#type == "projects_updated" => {
            if tx_reload.send(message.timestamp).await.is_ok() {
                IpcAck {
                    status: "ok".to_string(),
                    error: None,
                }
            } else {
                IpcAck {
                    status: "error".to_string(),
                    error: Some("reload channel closed".to_string()),
                }
            }
        }
        Ok(_) => IpcAck {
            status: "error".to_string(),
            error: Some("unsupported message type".to_string()),
        },
        Err(err) => IpcAck {
            status: "error".to_string(),
            error: Some(format!("invalid message: {}", err)),
        },
    };

    let mut serialized = serde_json::to_string(&ack)?;
    serialized.push('\n');
    writer_half.write_all(serialized.as_bytes()).await?;
    Ok(())
}

pub async fn notify_projects_updated(endpoint: &str, timestamp: &str) -> Result<()> {
    let mut stream = TcpStream::connect(endpoint)
        .await
        .with_context(|| format!("failed to connect IPC tcp endpoint {}", endpoint))?;
    let message = IpcMessage {
        r#type: "projects_updated".to_string(),
        timestamp: timestamp.to_string(),
    };
    let mut line = serde_json::to_string(&message)?;
    line.push('\n');
    stream.write_all(line.as_bytes()).await?;

    let mut reader = BufReader::new(stream);
    let mut ack_line = String::new();
    reader.read_line(&mut ack_line).await?;
    let ack: IpcAck = serde_json::from_str(ack_line.trim())?;
    if ack.status != "ok" {
        bail!(
            "daemon rejected projects_updated notification: {}",
            ack.error.unwrap_or_else(|| "unknown error".to_string())
        );
    }
    Ok(())
}

pub fn notify_projects_updated_blocking(endpoint: &str, timestamp: &str) -> Result<()> {
    let mut stream = std::net::TcpStream::connect(endpoint)
        .with_context(|| format!("failed to connect IPC tcp endpoint {}", endpoint))?;
    let message = IpcMessage {
        r#type: "projects_updated".to_string(),
        timestamp: timestamp.to_string(),
    };
    let mut line = serde_json::to_string(&message)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    stream.flush()?;

    let mut reader = StdBufReader::new(stream);
    let mut ack_line = String::new();
    reader.read_line(&mut ack_line)?;
    let ack: IpcAck = serde_json::from_str(ack_line.trim())?;
    if ack.status != "ok" {
        bail!(
            "daemon rejected projects_updated notification: {}",
            ack.error.unwrap_or_else(|| "unknown error".to_string())
        );
    }
    Ok(())
}

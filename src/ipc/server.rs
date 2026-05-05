use anyhow::{Result, bail};
use std::path::Path;
use tokio::sync::mpsc;

pub async fn run_ipc_server(endpoint: &Path, _tx_reload: mpsc::Sender<String>) -> Result<()> {
    #[cfg(all(feature = "ipc-unix", target_family = "unix"))]
    if endpoint.is_absolute() {
        return super::unix::run_ipc_server(&endpoint.to_string_lossy(), _tx_reload).await;
    }

    #[cfg(feature = "ipc-tcp")]
    if let Some(endpoint) = endpoint.to_str() {
        return super::tcp::run_ipc_server(endpoint, _tx_reload).await;
    }

    bail!(
        "no IPC transport available for endpoint {}; enable ipc-unix/ipc-tcp features",
        endpoint.display()
    )
}

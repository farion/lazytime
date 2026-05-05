pub mod reload;
pub mod state;

use anyhow::Result;
use chrono::Utc;
use tokio::sync::mpsc;

use crate::config::Config;
use crate::db;
use crate::ipc::server::run_ipc_server;
use crate::platform;
use crate::platform::types::WindowInfo;
use crate::rules::{RuleCache, load_rules};

pub const DAEMON_RUNTIME_LOCK_KEY: &str = "daemon_runtime_lock";

struct DaemonLockGuard {
    db_file: String,
    owner: String,
}

impl Drop for DaemonLockGuard {
    fn drop(&mut self) {
        if let Ok(conn) = db::open(std::path::Path::new(&self.db_file)) {
            let _ = db::release_lock_if_value(&conn, DAEMON_RUNTIME_LOCK_KEY, &self.owner);
        }
    }
}

pub async fn run_daemon(config: &Config) -> Result<()> {
    let conn = db::open(config.db_path())?;
    db::migrate(&conn)?;

    let owner_token = std::env::var("LAZYTIME_DAEMON_OWNER")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_default();
    let owner = if owner_token.is_empty() {
        format!("pid:{}", std::process::id())
    } else {
        format!("{}|pid:{}", owner_token, std::process::id())
    };
    if !db::try_acquire_lock_with_value(&conn, DAEMON_RUNTIME_LOCK_KEY, &owner)? {
        anyhow::bail!("daemon already running")
    }
    let _lock_guard = DaemonLockGuard {
        db_file: config.db_file.clone(),
        owner,
    };

    let rules = load_rules(&conn)?;
    let cache = RuleCache::default();
    cache.replace(rules).await;

    let (tx_reload, mut rx_reload) = mpsc::channel::<String>(128);
    let socket_path = config.ipc_socket_path();

    let cache_for_reload = cache.clone();
    let db_path = config.db_file.clone();
    tokio::spawn(async move {
        while let Some(timestamp) = rx_reload.recv().await {
            tracing::info!("received projects_updated IPC at {}", timestamp);
            match db::open(std::path::Path::new(&db_path)) {
                Ok(conn) => match load_rules(&conn) {
                    Ok(new_rules) => {
                        cache_for_reload.replace(new_rules).await;
                        tracing::info!("reloaded project rules from DB");
                    }
                    Err(err) => {
                        tracing::error!("rule reload failed: {err}");
                    }
                },
                Err(err) => tracing::error!("failed to open DB for reload: {err}"),
            }
        }
    });

    let ipc_task = tokio::spawn(async move { run_ipc_server(&socket_path, tx_reload).await });

    let mut daemon_state = state::DaemonState::new(config.clone());
    let start_info = WindowInfo {
        app_id: None,
        instance: None,
        class: None,
        title: "startup".to_string(),
        workspace: None,
        output: None,
    };
    let _ = daemon_state
        .process_event(&db::open(config.db_path())?, &cache, start_info, Utc::now())
        .await;

    tokio::select! {
        res = platform::run_event_loop(config, cache, daemon_state) => {
            res?;
        }
        _ = shutdown_signal() => {
            tracing::info!("shutdown signal received; stopping daemon");
        }
    }

    ipc_task.abort();
    Ok(())
}

#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};

    let mut term = signal(SignalKind::terminate()).expect("failed to subscribe SIGTERM");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

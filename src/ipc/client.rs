use anyhow::{Result, bail};
use std::path::Path;

pub async fn notify_projects_updated(endpoint: &Path, _timestamp: &str) -> Result<()> {
    #[cfg(all(feature = "ipc-unix", target_family = "unix"))]
    if endpoint.is_absolute() {
        return super::unix::notify_projects_updated(&endpoint.to_string_lossy(), _timestamp).await;
    }

    #[cfg(feature = "ipc-tcp")]
    if let Some(endpoint) = endpoint.to_str() {
        return super::tcp::notify_projects_updated(endpoint, _timestamp).await;
    }

    bail!(
        "no IPC transport available for endpoint {}; enable ipc-unix/ipc-tcp features",
        endpoint.display()
    )
}

pub fn notify_projects_updated_blocking(endpoint: &Path, _timestamp: &str) -> Result<()> {
    #[cfg(all(feature = "ipc-unix", target_family = "unix"))]
    if endpoint.is_absolute() {
        return super::unix::notify_projects_updated_blocking(
            &endpoint.to_string_lossy(),
            _timestamp,
        );
    }

    #[cfg(feature = "ipc-tcp")]
    if let Some(endpoint) = endpoint.to_str() {
        return super::tcp::notify_projects_updated_blocking(endpoint, _timestamp);
    }

    bail!(
        "no IPC transport available for endpoint {}; enable ipc-unix/ipc-tcp features",
        endpoint.display()
    )
}

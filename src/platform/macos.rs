use std::sync::mpsc;

use super::types::{LockEvent, OutputRect, WindowInfo};

pub fn request_permissions_if_needed() {
    super::ax::request_accessibility_permission_prompt();
    super::ax::request_automation_permission_prompt();
}

pub fn spawn_macos_monitors(tx_lock: mpsc::Sender<LockEvent>, tx_window: mpsc::Sender<WindowInfo>) {
    super::macos_lock::spawn_lock_monitor(tx_lock);

    let tx_fallback = tx_window.clone();
    std::thread::spawn(move || {
        if let Err(err) = super::ax::monitor_window_events(tx_window) {
            tracing::warn!("macos ax window monitor unavailable: {err}");
            super::cgwindow::monitor_window_events(tx_fallback);
        }
    });
}

pub fn output_rect(output_name: &str) -> Option<OutputRect> {
    super::cgwindow::output_rect(output_name)
}

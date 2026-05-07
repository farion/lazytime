use std::sync::mpsc;

use super::types::{LockEvent, WindowInfo};

pub fn spawn_desktop_monitors(
    tx_lock: mpsc::Sender<LockEvent>,
    tx_window: mpsc::Sender<WindowInfo>,
) {
    spawn_lock_monitors(tx_lock);
    spawn_window_monitors(tx_window);
}

fn spawn_lock_monitors(tx_lock: mpsc::Sender<LockEvent>) {
    let tx_session = tx_lock.clone();
    std::thread::spawn(move || {
        if let Err(err) = super::linux_lock::monitor_screensaver(tx_session) {
            tracing::warn!("screensaver lock monitor stopped: {err}");
        }
    });

    std::thread::spawn(move || {
        if let Err(err) = super::linux_lock::monitor_login1(tx_lock) {
            tracing::warn!("login1 lock monitor stopped: {err}");
        }
    });
}

fn spawn_window_monitors(tx_window: mpsc::Sender<WindowInfo>) {
    let tx_atspi = tx_window.clone();
    std::thread::spawn(move || {
        if let Err(err) = super::atspi::monitor_window_events(tx_atspi) {
            tracing::warn!("at-spi monitor exited: {err}");
        }
    });

    std::thread::spawn(move || {
        if let Err(err) = super::x11::monitor_active_window(tx_window) {
            tracing::warn!("x11 fallback monitor exited: {err}");
        }
    });
}

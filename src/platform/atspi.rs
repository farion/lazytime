use anyhow::Result;
use dbus::blocking::Connection as DbusConnection;
use dbus::message::MatchRule;
use std::sync::mpsc;

use super::types::WindowInfo;

pub fn monitor_window_events(tx_window: mpsc::Sender<WindowInfo>) -> Result<()> {
    let conn = DbusConnection::new_session()?;

    let focus_rule = MatchRule::new_signal("org.a11y.atspi.Event.Object", "StateChanged");
    let tx_focus = tx_window.clone();
    conn.add_match(focus_rule, move |_: (), _, msg| {
        let member = msg.member().map(|m| m.to_string()).unwrap_or_default();
        if member.contains("StateChanged")
            && let Some(info) = window_info_from_message(msg)
        {
            let _ = tx_focus.send(info);
        }
        true
    })?;

    let title_rule = MatchRule::new_signal("org.a11y.atspi.Event.Object", "PropertyChange");
    conn.add_match(title_rule, move |_: (), _, msg| {
        if let Some(interface) = msg.interface()
            && interface.to_string() == "org.a11y.atspi.Event.Object"
            && let Some(info) = window_info_from_message(msg)
        {
            let _ = tx_window.send(info);
        }
        true
    })?;

    tracing::info!("window monitor: listening for AT-SPI object events");
    loop {
        let _ = conn.process(std::time::Duration::from_millis(1000))?;
    }
}

fn window_info_from_message(msg: &dbus::Message) -> Option<WindowInfo> {
    let app_id = msg
        .sender()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty());

    let title = msg
        .path()
        .map(|p| p.to_string())
        .unwrap_or_else(|| "atspi".to_string());

    Some(WindowInfo {
        app_id,
        instance: None,
        class: None,
        title,
        workspace: None,
        output: None,
    })
}

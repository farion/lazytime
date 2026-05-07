use std::process::Command;
use std::sync::mpsc;

use super::types::{OutputRect, WindowInfo};

pub fn monitor_window_events(tx_window: mpsc::Sender<WindowInfo>) {
    tracing::info!("macos window monitor: CGWindow fallback polling active");
    let mut last_sig = String::new();
    loop {
        if let Some(info) = frontmost_window_info() {
            let sig = format!(
                "{}|{}|{}",
                info.app_id.clone().unwrap_or_default(),
                info.class.clone().unwrap_or_default(),
                info.title
            );
            if sig != last_sig {
                let _ = tx_window.send(info);
                last_sig = sig;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(600));
    }
}

pub fn output_rect(output_name: &str) -> Option<OutputRect> {
    if output_name != "main" {
        return None;
    }
    Some(OutputRect {
        x: 0,
        y: 0,
        width: 1440,
        height: 900,
    })
}

fn frontmost_window_info() -> Option<WindowInfo> {
    let script = r#"
tell application "System Events"
  set frontApp to first process whose frontmost is true
  set appName to name of frontApp
  set winTitle to ""
  try
    set winTitle to name of front window of frontApp
  end try
  return appName & tab & winTitle
end tell
"#;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let line = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let mut parts = line.split('\t');
    let app_name = parts.next().unwrap_or_default().trim().to_string();
    let title = parts.next().unwrap_or_default().trim().to_string();
    if app_name.is_empty() && title.is_empty() {
        return None;
    }

    Some(WindowInfo {
        app_id: if app_name.is_empty() {
            None
        } else {
            Some(app_name.clone())
        },
        instance: if app_name.is_empty() {
            None
        } else {
            Some(app_name.clone())
        },
        class: if app_name.is_empty() {
            None
        } else {
            Some(app_name.clone())
        },
        title: if title.is_empty() { app_name } else { title },
        workspace: None,
        output: Some("main".to_string()),
    })
}

use anyhow::{Result, anyhow};
use std::process::Command;
use std::sync::mpsc;

use super::types::WindowInfo;

pub fn monitor_window_events(tx_window: mpsc::Sender<WindowInfo>) -> Result<()> {
    if !accessibility_permission_granted() {
        return Err(anyhow!(
            "accessibility permission not granted; falling back to CGWindow polling"
        ));
    }

    tracing::info!("macos window monitor: accessibility polling active");
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
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
}

fn accessibility_permission_granted() -> bool {
    let script = r#"
tell application "System Events"
  return UI elements enabled
end tell
"#;
    let output = Command::new("osascript").arg("-e").arg(script).output();
    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout)
                .trim()
                .to_ascii_lowercase();
            text == "true"
        }
        _ => false,
    }
}

pub fn frontmost_window_info() -> Option<WindowInfo> {
    let script = r#"
tell application "System Events"
  set frontApp to first process whose frontmost is true
  set appName to name of frontApp
  set winTitle to ""
  try
    set winTitle to name of front window of frontApp
  end try
  try
    set bundleId to bundle identifier of application process appName
  on error
    set bundleId to ""
  end try
  try
    set appPath to POSIX path of (file of application process appName as alias)
  on error
    set appPath to ""
  end try
  return bundleId & tab & appPath & tab & winTitle & tab & appName
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
    parse_frontmost_payload(&line)
}

fn parse_frontmost_payload(line: &str) -> Option<WindowInfo> {
    let mut parts = line.split('\t');
    let bundle = parts.next().unwrap_or_default().trim();
    let app_path = parts.next().unwrap_or_default().trim();
    let title = parts.next().unwrap_or_default().trim().to_string();
    let app_name = parts.next().unwrap_or_default().trim().to_string();

    if title.is_empty() && app_name.is_empty() {
        return None;
    }

    let app_id = if !bundle.is_empty() {
        Some(bundle.to_string())
    } else {
        normalize_exec_path(app_path)
    };

    let class = if app_name.is_empty() {
        None
    } else {
        Some(app_name.clone())
    };

    Some(WindowInfo {
        app_id,
        instance: class.clone(),
        class,
        title: if title.is_empty() { app_name } else { title },
        workspace: None,
        output: Some("main".to_string()),
    })
}

pub fn normalize_exec_path(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::{normalize_exec_path, parse_frontmost_payload};

    #[test]
    fn parse_prefers_bundle_id() {
        let info =
            parse_frontmost_payload("com.apple.Safari\t/Applications/Safari.app/\tExample\tSafari")
                .expect("expected parsed info");
        assert_eq!(info.app_id.as_deref(), Some("com.apple.Safari"));
        assert_eq!(info.title, "Example");
    }

    #[test]
    fn parse_falls_back_to_exec_path() {
        let info = parse_frontmost_payload("\t/opt/homebrew/bin/wezterm\tTerminal\tWezTerm")
            .expect("expected parsed info");
        assert_eq!(info.app_id.as_deref(), Some("/opt/homebrew/bin/wezterm"));
    }

    #[test]
    fn normalize_exec_path_none_for_empty() {
        assert_eq!(normalize_exec_path("   "), None);
    }
}

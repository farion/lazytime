use std::process::Command;
use std::sync::mpsc;

use super::types::{LockEvent, LockSource};

pub fn spawn_lock_monitor(tx_lock: mpsc::Sender<LockEvent>) {
    std::thread::spawn(move || {
        tracing::info!("macos lock monitor: session polling active");
        let mut last_locked = current_locked_state();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(800));
            let locked = current_locked_state();
            if locked != last_locked {
                let event = if locked {
                    LockEvent::Locked(LockSource::MacosSession)
                } else {
                    LockEvent::Unlocked(LockSource::MacosSession)
                };
                let _ = tx_lock.send(event);
                last_locked = locked;
            }
        }
    });
}

fn current_locked_state() -> bool {
    let script = r#"
tell application "System Events"
  set isLocked to false
  try
    set isLocked to (name of first process whose name is "loginwindow") is "loginwindow"
  end try
  return isLocked
end tell
"#;

    let output = Command::new("osascript").arg("-e").arg(script).output();
    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_ascii_lowercase();
            text == "true"
        }
        _ => false,
    }
}

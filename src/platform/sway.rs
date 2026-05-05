use serde_json::Value;
use std::process::Command;
use std::sync::mpsc;
use std::thread;

use super::types::{LockEvent, LockSource, OutputRect, WindowInfo};

pub fn is_available() -> bool {
    std::env::var("SWAYSOCK")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
        || swayipc::Connection::new().is_ok()
}

fn value_path<'a>(v: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = v;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn value_str(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).map(ToString::to_string)
}

fn parse_window_info(event: &swayipc::Event) -> Option<WindowInfo> {
    let swayipc::Event::Window(w) = event else {
        return None;
    };

    let raw = serde_json::to_value(w).ok()?;
    let container = value_path(&raw, &["container"])?;

    let title = value_str(container.get("name")).unwrap_or_default();
    let app_id = value_str(container.get("app_id"));
    let window_props = container.get("window_properties");
    let instance = value_str(window_props.and_then(|v| v.get("instance")));
    let class = value_str(window_props.and_then(|v| v.get("class")));

    let workspace = value_str(container.get("current_workspace")).or_else(|| {
        value_str(container.get("workspace_name")).or_else(|| value_str(container.get("workspace")))
    });
    let output = value_str(container.get("output"));

    let change = value_str(raw.get("change"));
    if let Some(ref ch) = change {
        if ch != "focus" && ch != "title" {
            tracing::debug!("skipping non-focus/non-title window event change={ch}");
            return None;
        }
        if ch == "title" {
            tracing::debug!("window title change event detected (change=title)");
        }
    }

    Some(WindowInfo {
        app_id,
        instance,
        class,
        title,
        workspace,
        output,
    })
}

pub fn spawn_sway_monitors(tx_lock: mpsc::Sender<LockEvent>, tx_window: mpsc::Sender<WindowInfo>) {
    spawn_lock_monitor_threads(tx_lock.clone());
    spawn_swaylock_process_monitor_thread(tx_lock);
    spawn_sway_window_monitor_thread(tx_window);
}

pub fn output_rect(output_name: &str) -> Option<OutputRect> {
    let mut conn = swayipc::Connection::new().ok()?;
    let outputs = conn.get_outputs().ok()?;
    let out = outputs
        .into_iter()
        .find(|o| o.name == output_name && o.active)?;
    Some(OutputRect {
        x: out.rect.x,
        y: out.rect.y,
        width: out.rect.width as i32,
        height: out.rect.height as i32,
    })
}

fn spawn_sway_window_monitor_thread(tx_window: mpsc::Sender<WindowInfo>) {
    thread::spawn(move || {
        use swayipc::{Connection, EventType};
        loop {
            let connection = match Connection::new() {
                Ok(conn) => conn,
                Err(err) => {
                    tracing::warn!("failed to connect sway IPC for window monitor: {err}");
                    thread::sleep(std::time::Duration::from_secs(1));
                    continue;
                }
            };

            let mut events = match connection.subscribe([EventType::Window]) {
                Ok(ev) => ev,
                Err(err) => {
                    tracing::warn!("failed to subscribe sway window events: {err}");
                    thread::sleep(std::time::Duration::from_secs(1));
                    continue;
                }
            };
            tracing::info!("subscribed to sway window events");

            loop {
                match events.next() {
                    Some(Ok(event)) => {
                        if let Some(info) = parse_window_info(&event) {
                            let _ = tx_window.send(info);
                        }
                    }
                    Some(Err(err)) => {
                        tracing::warn!("sway event error: {err}");
                    }
                    None => {
                        tracing::warn!("sway event stream ended; reconnecting");
                        break;
                    }
                }
            }

            thread::sleep(std::time::Duration::from_secs(1));
        }
    });
}

fn spawn_lock_monitor_threads(tx_lock: mpsc::Sender<LockEvent>) {
    #[cfg(all(feature = "backend-linux", target_os = "linux"))]
    {
        let tx_session = tx_lock.clone();
        thread::spawn(move || {
            if let Err(err) = super::linux_lock::monitor_screensaver(tx_session) {
                tracing::warn!("screensaver lock monitor stopped: {err}");
            }
        });

        thread::spawn(move || {
            if let Err(err) = super::linux_lock::monitor_login1(tx_lock) {
                tracing::warn!("login1 lock monitor stopped: {err}");
            }
        });
    }
}

fn spawn_swaylock_process_monitor_thread(tx_lock: mpsc::Sender<LockEvent>) {
    thread::spawn(move || {
        tracing::info!("lock monitor: watching swaylock process state");
        let mut was_running = is_swaylock_running();
        loop {
            thread::sleep(std::time::Duration::from_millis(500));
            let is_running = is_swaylock_running();
            if is_running != was_running {
                let event = if is_running {
                    LockEvent::Locked(LockSource::Swaylock)
                } else {
                    LockEvent::Unlocked(LockSource::Swaylock)
                };
                let _ = tx_lock.send(event);
                was_running = is_running;
            }
        }
    });
}

fn is_swaylock_running() -> bool {
    Command::new("pgrep")
        .arg("-x")
        .arg("swaylock")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{parse_window_info, value_path};
    use serde_json::json;

    #[test]
    fn value_path_handles_nested_lookup() {
        let v = json!({"a": {"b": {"c": 1}}});
        assert_eq!(
            value_path(&v, &["a", "b", "c"]).and_then(|x| x.as_i64()),
            Some(1)
        );
        assert!(value_path(&v, &["a", "x"]).is_none());
    }

    #[test]
    fn parse_window_info_compiles_for_window_event_shape() {
        let raw = json!({
            "change": "focus",
            "container": {
                "app_id": "app",
                "name": "title",
                "current_workspace": "2",
                "output": "HDMI-A-1",
                "window_properties": {"instance": "inst", "class": "class"}
            }
        });
        let parsed = serde_json::from_value::<swayipc::Event>(json!({"window": raw}));
        if let Ok(ev) = parsed {
            let info = parse_window_info(&ev);
            assert!(info.is_some());
        }
    }

    #[test]
    fn parse_window_info_handles_title_change_event() {
        let raw = json!({
            "change": "title",
            "container": {
                "app_id": "app",
                "name": "new title",
                "window_properties": {"instance": "inst", "class": "class"}
            }
        });
        let parsed = serde_json::from_value::<swayipc::Event>(json!({"window": raw}));
        if let Ok(ev) = parsed {
            let info = parse_window_info(&ev);
            assert!(info.is_some());
            let info = info.unwrap();
            assert_eq!(info.title, "new title");
        }
    }
}

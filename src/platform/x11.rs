use anyhow::{Result, bail};
use std::sync::mpsc;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

use super::types::WindowInfo;

pub fn monitor_active_window(tx_window: mpsc::Sender<WindowInfo>) -> Result<()> {
    if std::env::var("DISPLAY")
        .map(|v| v.trim().is_empty())
        .unwrap_or(true)
    {
        tracing::info!("x11 fallback monitor: DISPLAY not set; skipping X11 fallback");
        return Ok(());
    }

    let (conn, screen_num) = x11rb::connect(None)?;
    let root = conn.setup().roots[screen_num].root;
    let atom_active = conn
        .intern_atom(false, b"_NET_ACTIVE_WINDOW")?
        .reply()?
        .atom;
    let atom_name = conn.intern_atom(false, b"_NET_WM_NAME")?.reply()?.atom;
    let atom_utf8 = conn.intern_atom(false, b"UTF8_STRING")?.reply()?.atom;

    tracing::info!("x11 fallback monitor: polling _NET_ACTIVE_WINDOW");
    let mut last_window: Option<u32> = None;
    loop {
        let reply = conn
            .get_property(false, root, atom_active, AtomEnum::WINDOW, 0, 1)?
            .reply()?;

        let active_window = reply.value32().and_then(|mut v| v.next());
        if active_window != last_window {
            last_window = active_window;
            if let Some(window) = active_window {
                let title = window_title(&conn, window, atom_name, atom_utf8)
                    .unwrap_or_else(|| "x11-window".to_string());
                let class = window_class(&conn, window).ok();

                let _ = tx_window.send(WindowInfo {
                    app_id: class.clone(),
                    instance: class.clone(),
                    class,
                    title,
                    workspace: None,
                    output: None,
                });
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(600));
    }
}

fn window_title<C: Connection>(
    conn: &C,
    window: u32,
    atom_name: u32,
    atom_utf8: u32,
) -> Option<String> {
    let reply = conn
        .get_property(false, window, atom_name, atom_utf8, 0, 1024)
        .ok()?
        .reply()
        .ok()?;
    if !reply.value.is_empty() {
        return String::from_utf8(reply.value).ok();
    }

    let fallback = conn
        .get_property(false, window, AtomEnum::WM_NAME, AtomEnum::STRING, 0, 1024)
        .ok()?
        .reply()
        .ok()?;
    String::from_utf8(fallback.value).ok()
}

fn window_class<C: Connection>(conn: &C, window: u32) -> Result<String> {
    let reply = conn
        .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 1024)?
        .reply()?;
    if reply.value.is_empty() {
        bail!("WM_CLASS is empty")
    }

    let parts: Vec<&[u8]> = reply
        .value
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .collect();
    let class = if parts.len() > 1 { parts[1] } else { parts[0] };
    Ok(String::from_utf8(class.to_vec())?)
}

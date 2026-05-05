use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MONITORINFOEXW, MonitorFromWindow,
};
use windows::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification, WTSUnRegisterSessionNotification,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EVENT_OBJECT_NAMECHANGE,
    EVENT_SYSTEM_FOREGROUND, GetClassNameW, GetForegroundWindow, GetMessageW, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, HWND_MESSAGE, MSG, TranslateMessage,
    WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS, WM_WTSSESSION_CHANGE, WTS_SESSION_LOCK,
    WTS_SESSION_UNLOCK,
};

use super::types::{LockEvent, LockSource, OutputRect, WindowInfo};

pub fn spawn_windows_monitors(tx_lock: mpsc::Sender<LockEvent>, tx_window: mpsc::Sender<WindowInfo>) {
    let tx_hook = tx_window.clone();
    std::thread::spawn(move || {
        if let Err(err) = monitor_window_hooks(tx_hook) {
            tracing::warn!("windows hook monitor failed; falling back to polling: {err}");
        }
    });

    std::thread::spawn(move || {
        if let Err(err) = monitor_lock_events(tx_lock) {
            tracing::warn!("windows lock monitor failed: {err}");
        }
    });
}

pub fn output_rect(output_name: &str) -> Option<OutputRect> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }
    let (monitor_name, rect) = monitor_info(hwnd)?;
    if monitor_name.eq_ignore_ascii_case(output_name) {
        return Some(rect);
    }
    None
}

fn monitor_window_hooks(tx_window: mpsc::Sender<WindowInfo>) -> anyhow::Result<()> {
    if let Ok(mut slot) = window_event_slot().lock() {
        *slot = Some(tx_window.clone());
    }

    let foreground_hook = unsafe {
        SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            None,
            Some(win_event_callback),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    let title_hook = unsafe {
        SetWinEventHook(
            EVENT_OBJECT_NAMECHANGE,
            EVENT_OBJECT_NAMECHANGE,
            None,
            Some(win_event_callback),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };

    if foreground_hook.is_invalid() && title_hook.is_invalid() {
        if let Ok(mut slot) = window_event_slot().lock() {
            *slot = None;
        }
        monitor_window_polling(tx_window)?;
        return Ok(());
    }

    tracing::info!("windows window monitor: SetWinEventHook active");
    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, HWND::default(), 0, 0) }.as_bool() {
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    if !foreground_hook.is_invalid() {
        unsafe {
            let _ = UnhookWinEvent(foreground_hook);
        }
    }
    if !title_hook.is_invalid() {
        unsafe {
            let _ = UnhookWinEvent(title_hook);
        }
    }
    if let Ok(mut slot) = window_event_slot().lock() {
        *slot = None;
    }

    Ok(())
}

fn monitor_window_polling(tx_window: mpsc::Sender<WindowInfo>) -> anyhow::Result<()> {
    tracing::info!("windows window monitor: polling fallback active");
    let mut last_sig = String::new();
    loop {
        let hwnd = unsafe { GetForegroundWindow() };
        if !hwnd.is_invalid() {
            if let Some(info) = collect_window_info(hwnd) {
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
        }
        std::thread::sleep(std::time::Duration::from_millis(450));
    }
}

fn monitor_lock_events(tx_lock: mpsc::Sender<LockEvent>) -> anyhow::Result<()> {
    let hwnd = unsafe {
        CreateWindowExW(
            Default::default(),
            windows::core::w!("STATIC"),
            windows::core::w!("lazytime-wts"),
            Default::default(),
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            None,
            None,
            None,
        )
    }?;

    if hwnd.is_invalid() {
        anyhow::bail!("failed to create message-only window for WTS notifications");
    }

    let registered = unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION) }.is_ok();
    if !registered {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        anyhow::bail!("failed to register WTS session notifications");
    }

    tracing::info!("windows lock monitor: listening for WTS session lock/unlock");
    let mut msg = MSG::default();
    while unsafe { GetMessageW(&mut msg, hwnd, 0, 0) }.as_bool() {
        if msg.message == WM_WTSSESSION_CHANGE {
            let event = match msg.wParam {
                WPARAM(w) if w as u32 == WTS_SESSION_LOCK => Some(LockEvent::Locked(LockSource::WtsSession)),
                WPARAM(w) if w as u32 == WTS_SESSION_UNLOCK => {
                    Some(LockEvent::Unlocked(LockSource::WtsSession))
                }
                _ => None,
            };
            if let Some(event) = event {
                let _ = tx_lock.send(event);
            }
        }

        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    unsafe {
        let _ = WTSUnRegisterSessionNotification(hwnd);
        let _ = DestroyWindow(hwnd);
    }

    Ok(())
}

fn collect_window_info(hwnd: HWND) -> Option<WindowInfo> {
    let title = window_text(hwnd);
    if title.trim().is_empty() {
        return None;
    }

    let class = class_name(hwnd);
    let (_, app_id) = process_identity(hwnd);
    let output = monitor_info(hwnd).map(|(name, _)| name);

    Some(WindowInfo {
        app_id,
        instance: class.clone(),
        class,
        title,
        workspace: None,
        output,
    })
}

fn process_identity(hwnd: HWND) -> (u32, Option<String>) {
    let mut pid = 0_u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
    }
    if pid == 0 {
        return (0, None);
    }

    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(handle) if !handle.is_invalid() => handle,
        _ => return (pid, None),
    };
    if handle.is_invalid() {
        return (pid, None);
    }

    let mut buffer = vec![0_u16; 32768];
    let mut size = buffer.len() as u32;
    let queried = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    }
    .is_ok();

    unsafe {
        let _ = CloseHandle(handle);
    }

    if queried {
        let raw = String::from_utf16_lossy(&buffer[..size as usize]);
        return (pid, Some(normalize_windows_app_id(&raw)));
    }

    (pid, None)
}

fn window_text(hwnd: HWND) -> String {
    let len = unsafe { GetWindowTextLengthW(hwnd) };
    if len <= 0 {
        return String::new();
    }
    let mut buffer = vec![0_u16; len as usize + 1];
    let written = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    if written <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buffer[..written as usize])
}

fn class_name(hwnd: HWND) -> Option<String> {
    let mut buffer = vec![0_u16; 256];
    let written = unsafe { GetClassNameW(hwnd, &mut buffer) };
    if written <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buffer[..written as usize]))
}

fn monitor_info(hwnd: HWND) -> Option<(String, OutputRect)> {
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_invalid() {
        return None;
    }

    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    let ok = unsafe {
        GetMonitorInfoW(
            monitor,
            &mut info as *mut MONITORINFOEXW as *mut MONITORINFO,
        )
    }
    .as_bool();
    if !ok {
        return None;
    }

    let mut end = 0_usize;
    while end < info.szDevice.len() && info.szDevice[end] != 0 {
        end += 1;
    }
    let name = String::from_utf16_lossy(&info.szDevice[..end]);
    let rect = info.monitorInfo.rcMonitor;
    Some((
        name,
        OutputRect {
            x: rect.left,
            y: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        },
    ))
}

fn normalize_windows_app_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut normalized = trimmed.replace('/', "\\").to_ascii_lowercase();
    if normalized.len() >= 2 && normalized.as_bytes()[1] == b':' {
        let drive = normalized.as_bytes()[0] as char;
        normalized.replace_range(0..1, &drive.to_ascii_uppercase().to_string());
    }
    normalized
}

unsafe extern "system" fn win_event_callback(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    _id_object: i32,
    _id_child: i32,
    _thread_id: u32,
    _time: u32,
) {
    if hwnd.is_invalid() {
        return;
    }

    if let Some(info) = collect_window_info(hwnd) {
        let tx = window_event_slot()
            .lock()
            .ok()
            .and_then(|guard| guard.as_ref().cloned());
        if let Some(tx) = tx {
            let _ = tx.send(info);
        }
    }
}

fn window_event_slot() -> &'static Mutex<Option<mpsc::Sender<WindowInfo>>> {
    static WINDOW_EVENT_TX: OnceLock<Mutex<Option<mpsc::Sender<WindowInfo>>>> = OnceLock::new();
    WINDOW_EVENT_TX.get_or_init(|| Mutex::new(None))
}

#[allow(dead_code)]
unsafe extern "system" fn passthrough_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::normalize_windows_app_id;

    #[test]
    fn normalize_windows_app_id_handles_separators() {
        assert_eq!(
            normalize_windows_app_id("C:/Program Files/App/app.exe"),
            "C:\\program files\\app\\app.exe"
        );
    }

    #[test]
    fn normalize_windows_app_id_keeps_drive_uppercase() {
        assert_eq!(normalize_windows_app_id("d:\\Work\\Tool.EXE"), "D:\\work\\tool.exe");
    }

    #[test]
    fn normalize_windows_app_id_empty_input() {
        assert_eq!(normalize_windows_app_id("   "), "");
    }
}

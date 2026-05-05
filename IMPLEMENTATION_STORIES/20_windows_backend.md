# 20 Windows Backend

Status: Planned

Goal
------
Add a compile-time Windows backend feature `backend-windows` that supplies WindowInfo (focus/title/class/app identity) and LockEvent (session lock/unlock) to the daemon core. The backend will use native Win32 APIs for real-time events (SetWinEventHook for window/focus/title and WTS session notifications for lock/unlock). Provide UI Automation (UIA) as an optional enhancement and a polling fallback when hooks are not possible.

Confirmed decisions (from user)
--------------------------------
1. Primary event source: SetWinEventHook (EVENT_SYSTEM_FOREGROUND, EVENT_OBJECT_NAMECHANGE). UIA (IUIAutomation) optional.
2. app_id semantics: use the full normalized executable path as the app_id. If a rule uses app_id == "*" it means "match any app; title-only" (treated like a fallback rule).
3. IPC transport: `ipc-tcp` will be preferred on Windows builds; project will continue to support both `ipc-unix` and `ipc-tcp` behind features.
4. Feature name: use `backend-windows` for the Windows-specific backend.

Why this approach
-------------------
- SetWinEventHook gives low-latency, reliable foreground and name change notifications for most desktop applications on Windows.
- WTS session notifications are the standard way to detect session lock/unlock reliably.
- Using the full process image path as `app_id` gives a globally unique identifier for most desktop apps. Mapping "*" to fallback rules preserves existing title-only matching semantics.
- Implementing hooks in a dedicated thread and sending events to the daemon via mpsc channels keeps the core async runtime unchanged and mirrors other backends' architecture.

Crate / feature suggestions (story only)
----------------------------------------
- Add optional dependency in Cargo.toml:

```toml
[dependencies]
windows = { version = "0.48", optional = true }

[features]
backend-windows = ["windows"]
```

(`windows` is a common name for windows-rs; pick the version used in your ecosystem.)

High-level design
-------------------
- New file: `src/platform/windows.rs` (#[cfg(feature = "backend-windows")])
  - Expose `pub fn spawn_windows_backend(tx_window: mpsc::Sender<WindowInfo>, tx_lock: mpsc::Sender<LockEvent>) -> Vec<std::thread::JoinHandle<()>>` or similar
  - The function spawns a dedicated thread that:
    - Initializes COM if UIA will be used (CoInitializeEx)
    - Registers SetWinEventHook for EVENT_SYSTEM_FOREGROUND and EVENT_OBJECT_NAMECHANGE
    - Creates a message-only window and calls WTSRegisterSessionNotification for lock events
    - Runs a standard Win32 message loop (GetMessage/DispatchMessage)
    - On hook callbacks or WM_WTSSESSION_CHANGE, builds WindowInfo / LockEvent and sends them on the provided channels
    - If hook registration or message loop fails, starts a polling fallback thread using GetForegroundWindow + GetWindowTextW at a tunable interval

Event mapping (populate WindowInfo)
-----------------------------------
- hwnd -> title: GetWindowTextW
- hwnd -> class: GetClassNameW
- hwnd -> pid -> full exe path: GetWindowThreadProcessId + QueryFullProcessImageName (PROCESS_QUERY_LIMITED_INFORMATION). Normalize result (lowercase, consistent separators). If full path unavailable, fallback to exe basename (foo.exe).
- app_id string: the normalized full exe path
- instance/class: populate from GetClassNameW or exe basename if class is not useful
- workspace/output: compute monitor using MonitorFromWindow + GetMonitorInfo when popup needs placement; otherwise None

App id wildcard semantics
-------------------------
- Stored rule with `app_id == "*"` should be treated as a fallback (title-only) rule. Implementation plan:
  - When loading rules from DB, if rule.app_id.is_some() and rule.app_id.trim() == "*", treat it as `None` and put compiled rule into `fallback_rules` (same bucket as rules without app_id).
  - This keeps precedence and matching logic unchanged.

Implementation checklist (ordered, minimal risk steps)
-----------------------------------------------------
1) Add platform types and traits (if missing)
   - `src/platform/types.rs` (WindowInfo, WindowEventInfo, LockEvent)
   - `src/platform/traits.rs` (WindowEventSource, LockEventSource, OutputLocator)

2) Add Windows backend skeleton
   - Add `src/platform/windows.rs` behind `#[cfg(feature = "backend-windows")]`.
   - Implement `spawn_windows_backend(...)` which starts the message-loop thread and returns join handles.
   - Export helper functions for unit tests where feasible.

3) Hook registration
   - Register SetWinEventHook for EVENT_SYSTEM_FOREGROUND and EVENT_OBJECT_NAMECHANGE. Use WINEVENT_OUTOFCONTEXT and WINEVENT_SKIPOWNPROCESS to avoid deadlocks.
   - The hook callback should filter to top-level windows (check OBJID or use GetAncestor(hwnd, GA_ROOT) to confirm top-level). For OBJECT_NAMECHANGE, ensure the event pertains to the window's name.

4) Lock notifications
   - Create a message-only window and call WTSRegisterSessionNotification to receive WM_WTSSESSION_CHANGE messages. Translate WTS_SESSION_LOCK -> LockEvent::Locked, WTS_SESSION_UNLOCK -> LockEvent::Unlocked.

5) App id normalization helper
   - Implement a function that takes a process id and returns a normalized app_id string:
     - QueryFullProcessImageName
     - Normalize path (lowercase drive letter, replace forward/back slashes with backslashes consistently)
     - If failing, return exe basename

6) OutputLocator
   - Implement monitor geometry helper using MonitorFromWindow + GetMonitorInfo to return monitor center for popup placement.

7) Polling fallback
   - If hooks cannot be installed (restricted environment), start a polling thread that queries GetForegroundWindow every N ms and emits changes.

8) Rules wildcard handling
   - Modify rule-loading logic so stored `app_id == "*"` is moved to fallback rules. See code sketch below.

9) Tests
   - Unit test: rule load where app_id == "*" lands in fallback_rules.
   - Unit test: app_id normalization with a few example paths and fallback to exe basename.

10) Integration / wiring
   - Wire `spawn_windows_backend` into `platform` factory and call from `daemon::run_daemon` as part of platform backend selection.

Code sketch: wildcard handling in load_rules
------------------------------------------
This pseudocode mirrors the planned change in `src/rules.rs::load_rules`:

```rust
let effective_app_id = match rule.app_id.as_deref().map(|s| s.trim()) {
    Some("*") => None,
    Some(s) if !s.is_empty() => Some(s.to_string()),
    _ => None,
};

if let Some(app_id) = effective_app_id {
    app_rules.entry(app_id).or_default().push(compiled_rule);
} else {
    fallback_rules.push(compiled_rule);
}
```

Threading and runtime model
----------------------------
- Keep the same pattern used by other platform backends: spawn blocking OS listeners on dedicated threads and use `tokio::sync::mpsc` channels to deliver WindowInfo/LockEvent to the daemon core.
- The message-loop thread must remain alive; the spawn function returns join handles for lifecycle management.

Edge cases and notes
---------------------
- QueryFullProcessImageName might fail for protected/system processes; fall back gracefully to basename.
- Some UWP apps may not expose titles via GetWindowTextW; UIA can be used as an optional enhancement to read accessible names.
- Hooks require the process running in an interactive session. Services or detached contexts will not receive per-user window events.

Acceptance criteria
--------------------
1. `cargo build --no-default-features` succeeds.
2. `cargo build --features backend-windows,ipc-tcp,popup-ui` succeeds (Windows CI job expected separately).
3. On Windows with `backend-windows` enabled the daemon receives WindowInfo events for foreground changes and LockEvent for session lock/unlock.
4. Rules with `app_id == "*"` are treated as fallback title-only rules.

Next steps
-----------
If you want I will persist this story file (IMPLEMENTATION_STORIES/20_windows_backend.md) — which I will do now — and then we can plan the implementation steps (minimal skeletons, tests, wiring). You already asked me to persist this story; it is now saved.

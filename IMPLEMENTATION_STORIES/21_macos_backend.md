# 21 macOS Backend

Status: Planned

Goal
------
Add a compile-time macOS backend feature `backend-macos` that supplies WindowInfo (focus/title/class/app identity) and LockEvent (screen lock/unlock) to the daemon core. The backend will use macOS Accessibility (AXObserver) as the primary event source and fall back to CoreGraphics window snapshots (CGWindowList) when Accessibility permission is not granted. Lock/unlock detection will use Distributed Notifications and fall back to session dictionary checks where necessary.

Confirmed decisions (user)
--------------------------------
1. Primary window source: AXObserver (Accessibility APIs). CGWindowList snapshot/polling is fallback.
2. Lock detection: Distributed Notifications primary; CGSessionCopyCurrentDictionary fallback.
3. app_id: prefer CFBundleIdentifier, fall back to executable path.
4. Wildcard semantics: stored rule with `app_id == "*"` must be treated as a title-only fallback rule across all backends (consistent behavior).
5. Persist this story file (confirmed).

Why this approach
-------------------
- AXObserver provides event-driven, low-latency window focus and title change notifications across Cocoa/Carbon/most toolkits when Accessibility permission is granted.
- CGWindowList snapshots work without Accessibility permission in many situations and provide a robust fallback on macOS.
- CFBundleIdentifier is a stable identifier for apps on macOS; falling back to executable path covers non-bundled binaries.
- Treating `app_id == "*"` as a fallback title-only rule keeps rule matching consistent between platforms.

Crate / feature suggestions (story only)
----------------------------------------
- Optional dependencies in Cargo.toml (example):

```toml
[dependencies]
core-foundation = { version = "0.9", optional = true }
core-graphics = { version = "0.22", optional = true }
objc = { version = "0.2", optional = true }

[features]
backend-macos = ["core-foundation", "core-graphics", "objc"]
```

Pick crate versions consistent with the project ecosystem.

High-level design
-------------------
- New platform namespace: `src/platform/*` will contain types, traits and the macOS backend implementation.
  - `platform/macos.rs` — macOS backend glue (feature-gated)
  - `platform/ax.rs` — AXObserver helpers to subscribe to focus/title events
  - `platform/cgwindow.rs` — CGWindowList polling fallback
  - `platform/macos_lock.rs` — Distributed notifications and session-dictionary fallback for lock detection
- Backend implements the platform traits: WindowEventSource, LockEventSource, OutputLocator, and sends events via channels to daemon core.

Implementation checklist (ordered, incremental)
------------------------------------------------
1) Platform types & traits
   - Ensure `src/platform/types.rs` and `src/platform/traits.rs` exist (WindowInfo, WindowEventInfo, LockEvent, WindowEventSource, LockEventSource, OutputLocator). If not present add them. Core daemon and rules should import these types, not platform-specific crates.

2) macOS backend skeleton
   - Add `src/platform/macos.rs` behind `#[cfg(feature = "backend-macos")]` that exposes `spawn_macos_backend(tx_window, tx_lock)` which starts AXObserver and/or CGWindowList threads and returns handles.

3) AXObserver integration
   - Add `src/platform/ax.rs`.
   - Use AXObserver to subscribe to kAXFocusedWindowChanged and kAXTitleChanged notifications. Create observer(s) that feed top-level window title and application attributes into WindowInfo.
   - Run AX observers on a thread with a CFRunLoop so callbacks are delivered.
   - When Accessibility permission is missing, log a warning and start the CGWindowList fallback.

4) CGWindowList fallback
   - Add `src/platform/cgwindow.rs`.
   - Implement a polling snapshot loop using CGWindowListCopyWindowInfo at a configurable interval. Diff consecutive snapshots to detect foreground window/title changes and emit WindowInfo.

5) Lock detection (macos_lock)
   - Add `src/platform/macos_lock.rs`.
   - Subscribe to Distributed Notifications commonly used for lock/unlock (e.g., screensaver/loginwindow notifications). Also implement a CGSessionCopyCurrentDictionary-based poll as a fallback to detect locked sessions.

6) OutputLocator / popup placement
   - Implement monitor geometry lookup using `CGGetActiveDisplayList` + `CGDisplayBounds` or `NSScreen` if using Cocoa. Provide a function to compute monitor center for popup placement when the backend can determine the appropriate output.

7) app_id handling and wildcard mapping
   - Populate WindowInfo.app_id from CFBundleIdentifier when available (NSRunningApplication::bundleIdentifier). If not available fall back to normalized executable path.
   - At rule load time, map stored `app_id == "*"` to a fallback rule (same bucket as rules with no app_id) so title-only matching works across backends.

8) Tests
   - Unit test: rules loader treats `app_id == "*"` as fallback rule.
   - Unit test: app_id normalization function behavior (bundle id vs executable path fallback).

9) Wiring into daemon
   - Update `src/daemon/mod.rs` (or platform factory) to spawn macOS backend when `backend-macos` is compiled in and macOS runtime is detected.

10) Documentation
   - Document Accessibility permission steps in docs/README: users must grant Accessibility permission for AXObserver to work; otherwise CGWindowList will be used as fallback.

File-by-file mapping (story-level)
---------------------------------
- Add: `src/platform/macos.rs` (main glue)
- Add: `src/platform/ax.rs` (AXObserver helpers)
- Add: `src/platform/cgwindow.rs` (CGWindowList fallback)
- Add: `src/platform/macos_lock.rs` (lock detection)
- Update: `src/platform/mod.rs` re-exports and platform factory
- Update: `src/daemon/mod.rs` to call the platform factory (tiny change)
- Update: `src/rules.rs` load_rules to map `app_id == "*"` to fallback rules (if not already updated for other backends)

Runtime notes & caveats
-----------------------
- Accessibility permission is required for AXObserver; the backend must detect permission absence and use fallback. Provide clear logs and user documentation.
- AXObserver is event-driven; CGWindowList is polling. Prefer event-driven when possible for responsiveness and CPU efficiency.
- Some apps may withhold titles or present titles differently; implement defensive defaults and robust logging for debugging.

Acceptance criteria
--------------------
1. `cargo build --no-default-features` succeeds (core only).
2. `cargo build --features backend-macos,ipc-tcp,popup-ui` succeeds.
3. With Accessibility permission granted, the daemon receives WindowInfo events for focus/title changes and they are processed by daemon.state.
4. Without Accessibility permission, the CGWindowList fallback emits WindowInfo via polling.
5. Lock/unlock events are delivered via Distributed Notifications or session-dictionary fallback.
6. Rules with `app_id == "*"` are treated as title-only fallback rules uniformly across all backends.

Next steps
-----------
This story is persisted. I can proceed to implement the initial minimal change set (platform types + NullBackend + macOS skeleton) if you want, or create CI stories and follow-up tasks. Which next step would you like?

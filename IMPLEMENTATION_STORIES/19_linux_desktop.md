# 19 Linux Desktop (GNOME & Plasma) Backend

Status: Planned

Goal
------
Add a new compile-time backend feature `backend-linux` that provides window/focus/title events and lock events for GNOME and KDE/Plasma desktops. The backend will: use AT‑SPI (session D‑Bus) as the primary event source, fall back to X11 `_NET_ACTIVE_WINDOW` when X11 is present, and reuse the existing `login1` / `org.freedesktop.ScreenSaver` lock hooks. This backend must coexist with the existing `backend-sway` feature and should be gated behind a Cargo feature flag so that binaries only include what is compiled in.

Confirmed Decisions
--------------------
1. Keep `backend-sway` as a separate, compile-time feature — do not remove or replace it. Default features remain sway-first during migration.
2. Keep using the existing `dbus` crate (blocking) for the initial implementation; isolate any dbus usage inside platform modules so it can be replaced with `zbus` later if desired.
3. Support both `ipc-unix` and `ipc-tcp` transports; the IPC facade will select the compiled transport.

Why this approach
-------------------
- AT‑SPI provides a cross‑toolkit, cross‑desktop way to observe focus and title changes for GUI applications on both Wayland and X11 in most setups.
- X11 fallback ensures coverage for pure X11 sessions where AT‑SPI may not be available or accessible.
- Keeping dbus blocking reduces refactor cost and matches the repo's current code style.
- Feature-gating keeps binaries small and backwards-compatible.

High-level Design
------------------
- New platform namespace: `src/platform/*` will contain types, traits and backends.
  - `platform/types.rs` — WindowInfo and WindowEventInfo (moved/aliased from daemon/state and rules)
  - `platform/traits.rs` — WindowEventSource, LockEventSource, OutputLocator traits
  - `platform/desktop.rs` — glue that wires AT‑SPI + X11 fallback and exports a backend factory
  - `platform/atspi.rs` — AT‑SPI subscriber implementation (session D‑Bus, `dbus::blocking`)
  - `platform/x11.rs` — X11 `_NET_ACTIVE_WINDOW` listener (x11rb)
  - `platform/linux_lock.rs` — ScreenSaver/login1 listeners (move from daemon/sway.rs)
- Each backend implements the same small interface (spawn listeners and send events via channels). The daemon core reads from these channels and does not depend on dbus/x11/sway crates.

Feature flags (recommended)
---------------------------
- `backend-sway` (existing)
- `backend-linux` (new) — depends on `dbus` and `x11rb` optional deps
- `ipc-unix` and `ipc-tcp` (existing plan)
- `popup-ui` (existing)

Cargo snippets (example, story only — do not edit Cargo.toml yet)

```toml
[dependencies]
swayipc = { version = "3.0", optional = true }
dbus = { version = "0.9", optional = true }
x11rb = { version = "0.10", optional = true }

[features]
default = ["backend-sway", "ipc-unix", "popup-ui"]
backend-sway = ["swayipc"]
backend-linux = ["dbus", "x11rb"]
ipc-unix = []
ipc-tcp = []
popup-ui = ["eframe"]
```

Implementation Checklist (ordered, small steps)
------------------------------------------------
1) Add platform types and traits
   - Add files:
     - `src/platform/mod.rs` (re-exports)
     - `src/platform/types.rs` (WindowInfo, WindowEventInfo, LockEvent)
     - `src/platform/traits.rs` (WindowEventSource, LockEventSource, OutputLocator)
   - Move or alias the existing WindowInfo in `src/daemon/state.rs` and WindowEventInfo in `src/rules.rs` into `platform/types.rs`. Update imports in core code to use the new path.

2) Add a NullBackend to allow `--no-default-features` builds
   - Implement a minimal NullBackend under `src/platform/null.rs` compiled when no backend features are enabled. It should spawn no listeners and close immediately.

3) Implement desktop glue module
   - Create `src/platform/desktop.rs` behind `#[cfg(feature = "backend-linux")]`.
   - It should expose a factory function `pub fn spawn_desktop_backends(tx_window: mpsc::Sender<WindowInfo>, tx_lock: mpsc::Sender<LockEvent>)` (or similar) which starts AT‑SPI + X11 fallback + login1/screen-saver monitors on threads and returns handles if needed.

4) Implement AT‑SPI subscriber (session D‑Bus)
   - Add `src/platform/atspi.rs` behind `#[cfg(feature = "backend-linux")]`.
   - Use `dbus::blocking::Connection::new_session()` to connect to session bus and add matches for AT‑SPI events.
   - Subscribe to events: focus/state-changed, object:property-change:accessible-name, window:activate (or the AT‑SPI signals relevant for focused window and accessible name changes).
   - Resolve an event to the toplevel accessible (walk parents), query application name and accessible name/title, then send a `WindowInfo` containing best-effort `app_id`, `instance`/`class` (if available) and `title` via the provided channel.
   - Emit logs when AT‑SPI is not available and fall back.

5) Implement X11 _NET_ACTIVE_WINDOW fallback
   - Add `src/platform/x11.rs` behind `#[cfg(feature = "backend-linux")]`.
   - Use `x11rb` to connect when DISPLAY is present and subscribe to PropertyChange on the root window for `_NET_ACTIVE_WINDOW`.
   - On change, query `_NET_WM_NAME`/`WM_NAME`, `WM_CLASS` and convert them to `WindowInfo` and send via channel.
   - x11rb code should be optional and only compiled when `x11rb` feature is present.

6) Move/enhance lock watchers
   - Move `monitor_screensaver` and `monitor_login1` functions from `src/daemon/sway.rs` into `src/platform/linux_lock.rs` and gate with `#[cfg(feature = "backend-linux")]`.
   - These connect to session and system bus respectively and send `LockEvent` over the channel.

7) Wire desktop backend into daemon
   - Edit `src/daemon/mod.rs` (or a platform factory file) to call the appropriate platform factory based on compiled features and runtime detection (see runtime selection rules below). This should be small: a few lines that create the channels and call `spawn_*` functions implemented in platform modules.

8) Popup output placement
   - Replace direct `swayipc` usage in `src/popup.rs` with an optional `OutputLocator` API from `platform/traits.rs`. The desktop backend can optionally provide a no-op or compositor-enhanced locator. If not provided, `popup` falls back to letting compositor decide placement.

9) IPC transport
   - Ensure existing `ipc` code is feature-gated or facaded so that `ipc-unix` remains default on Unix and `ipc-tcp` can be selected for cross-platform builds. The desktop backend doesn't change IPC behavior, but story should document that `ipc` must remain a cross-platform facade.

10) CI and compile checks
   - Add compile-only CI jobs for:
     - `cargo check --no-default-features` (core-only)
     - `cargo check --features backend-sway,ipc-unix,popup-ui` (Sway build)
     - `cargo check --no-default-features --features backend-linux,ipc-unix,popup-ui` (Linux desktop)
     - Optionally: `cargo check --no-default-features --features ipc-tcp,popup-ui` (cross-platform IPC)

File-by-file mapping (what to add/change)
---------------------------------------
- Add: `src/platform/mod.rs` (re-export types and factories)
- Add: `src/platform/types.rs` (WindowInfo, WindowEventInfo, LockEvent)
- Add: `src/platform/traits.rs` (traits for sources)
- Add: `src/platform/null.rs` (NullBackend for no-backend builds)
- Add: `src/platform/desktop.rs` (glue for AT‑SPI + X11 fallback)
- Add: `src/platform/atspi.rs` (AT‑SPI subscriber)
- Add: `src/platform/x11.rs` (X11 fallback)
- Add: `src/platform/linux_lock.rs` (moved login1/screen-saver watchers)
- Update: `src/daemon/mod.rs` to spawn platform backends (small change)
- Update: `src/popup.rs` to use OutputLocator (small change)
- Update: `src/ipc/mod.rs` to be a facade (if not already)

Runtime backend selection rules
--------------------------------
At runtime choose the best available backend among compiled ones:
- If `backend-sway` compiled and sway detection looks valid (SWAYSOCK present or swayipc::Connection::new() succeeds) → use sway backend.
- Else if `backend-linux` compiled → use desktop backend (AT‑SPI/X11 fallback).
- Else → NullBackend (no auto-tracking; CLI/TUI still usable).

Event mapping rules (how to populate WindowInfo)
-----------------------------------------------
- app_id: prefer application identifier from AT‑SPI (if available) or second element of WM_CLASS on X11.
- instance/class: WM_CLASS or AT‑SPI application name best-effort mapping.
- title: accessible name (AT‑SPI) or `_NET_WM_NAME` / `WM_NAME` on X11.
- workspace/output: optional; populate when compositor exposes it (not always available via AT‑SPI). Desktop backend may leave them None.

Robustness notes & caveats
---------------------------
- AT‑SPI may be disabled or unavailable in some environments (flatpak sandbox, accessibility disabled). Desktop backend must log and fall back to X11 or compositor hooks.
- AT‑SPI events can be chatty — do light filtering in the backend and rely on existing `tracking_stability_seconds` in `daemon.state` for higher-level debouncing.
- XWayland/X11 mixing: some Wayland sessions still run XWayland apps — the fallback covers them, but mapping app ids may be imperfect.

Acceptance criteria
--------------------
1. `cargo build --no-default-features` succeeds (core only).
2. `cargo build --features backend-linux,ipc-unix,popup-ui` succeeds and the binary includes the desktop backend.
3. When desktop backend enabled and running on a GNOME/Plasma session with AT‑SPI available, the daemon receives window focus/title events and they appear as `WindowInfo` in daemon logs.
4. When desktop backend enabled on an X11 session, `_NET_ACTIVE_WINDOW` fallback produces window events.
5. Lock events (ScreenSaver/login1) are emitted and handled the same way as existing implementation.
6. Sway backend remains operational and unchanged when compiled as `backend-sway`.

Next steps (implementation-ready)
---------------------------------
If you'd like me to implement this story, the minimal first change is to add the `src/platform` types and a NullBackend, plus the `platform` module re-exports and small wiring in `daemon/mod.rs`. That will make it possible to compile with `--no-default-features` and iterate safely.

This story documents the technical choices you confirmed (keep sway default, keep dbus, support both ipc transports) and provides an ordered checklist suitable for incremental implementation.

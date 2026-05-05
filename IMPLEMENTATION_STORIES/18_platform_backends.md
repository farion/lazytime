# 18 Platform Backends and Feature-Gated OS Encapsulation

Goal
------
Encapsulate all OS- and compositor-specific code (Sway, Wayland/X11 placement, lock detection, IPC transport) behind a clear platform abstraction layer and expose per-backend implementations via Cargo features. This keeps the core tracking logic portable and allows building lazytime for Linux (Sway/Hyprland/GNOME/Plasma), macOS and Windows.

Target architecture
-------------------
- Core tracking logic remains backend-agnostic: src/daemon/state.rs, src/rules.rs, src/db.rs
- Add a new platform layer: src/platform/* that exposes trait-based backends for events, locks and optional output lookup
- Implement per-backend modules (sway, hyprland, gnome, plasma, macos, windows) under src/platform and gate them with features
- Make platform-specific deps optional in Cargo.toml and bound to features (swayipc, dbus, etc.)

Feature matrix (recommended)
-----------------------------
- backend-sway: Sway/compositor event source (swayipc, swaylock helpers)
- backend-hyprland: Hyprland-specific event source (if implementation differs)
- backend-gnome: GNOME desktop events (if needed)
- backend-plasma: KDE/Plasma integration (if needed)
- backend-macos: macOS event source (Accessibility API / Cocoa)
- backend-windows: Windows event source (Win32 / UIAutomation)
- ipc-unix: Unix domain socket IPC transport (current implementation)
- ipc-tcp: TCP IPC transport (cross-platform fallback)
- popup-ui: GUI popups (eframe/egui) — already present
- popup-output-placement: optional feature for output-aware popup placement (depends on per-backend OutputLocator)

High-level steps
-----------------
1. Cargo feature metadata
   - Make platform-specific deps optional and tied to features:
     - swayipc = { version = "3.0", optional = true }
     - dbus = { version = "0.9", optional = true }
   - Add feature groups in Cargo.toml: e.g. default = ["backend-sway", "ipc-unix", "popup-ui"] (or keep default minimal and explicit)

2. Add platform abstraction API
   - Create `src/platform/mod.rs` and `src/platform/types.rs`:
     - Reuse the existing WindowInfo/WindowEventInfo shapes (app_id, instance, class, title, workspace, output)
     - Define traits:
       - WindowEventSource: start() -> mpsc::Receiver<WindowInfo> or spawn into provided channel
       - LockEventSource: emits lock/unlock events
       - OutputLocator: optional trait to resolve output geometry by name (used for popup placement)
   - These traits are the only place that imports platform-specific crates in the platform modules. The rest of the daemon talks to the above traits via channels and trait objects (or feature-gated static implementations).

3. Extract Sway implementation
   - Move `src/daemon/sway.rs` -> `src/platform/sway.rs` behind `#[cfg(feature = "backend-sway")]`
   - Implement the WindowEventSource and LockEventSource traits (or functions) that the daemon will consume
   - Keep parsing helpers (value_path, parse_window_info) local to this module
   - Move swaylock process watcher into this module (is_swaylock_running, spawn_* functions)

4. Replace direct calls in daemon entrypoint
   - Edit `src/daemon/mod.rs` to call a factory like `platform::build_backend(config, tx_channels...)` or directly call `platform::sourced::run_event_loop(...)` depending on features
   - The core loop (previously in sway::run_event_loop) should be generic and moved into a new `src/daemon/loop.rs` if needed. The platform backend should only provide channels for WindowInfo and LockEvents.

5. Encapsulate popup placement
   - Remove direct `swayipc::Connection::get_outputs()` from `src/popup.rs`
   - Accept an optional OutputLocator from platform:: (or use a simple function pointer) to resolve output geometry
   - If no OutputLocator is available, fall back to compositor/default placement

6. Make IPC transport platform-neutral
   - Split `src/ipc/*` into `src/ipc/unix.rs` (ipc-unix) and `src/ipc/tcp.rs` (ipc-tcp)
   - Keep a facade `src/ipc/mod.rs` that re-exports the chosen implementation via feature flags
   - Update `src/config.rs` to use per-OS default: Unix socket on Unix + TCP loopback on Windows/macOS (when ipc-tcp) or respect explicit config path

7. Move DBus lock monitoring behind linux-only gate
   - The DBus login1/screensaver listeners (monitor_login1, monitor_screensaver) depend on `dbus` and only make sense on Linux. Place them in `src/platform/linux_lock.rs` and gate with `#[cfg(all(feature = "backend-sway", target_os = "linux"))]` or similar.

8. Tests and CI
   - Add matrix builds into CI for at least:
     - Core-only: `--no-default-features` (ensure core compiles without any backends)
     - Linux sway build: `--features backend-sway,ipc-unix,popup-ui`
     - Generic linux build (no sway): `--no-default-features --features ipc-unix`
     - macOS/Windows cross-platform check: `--no-default-features --features ipc-tcp,popup-ui` (run where applicable)

Detailed file-by-file migration checklist (order matters)
-----------------------------------------------------
1. Add platform dir and types
   - Add: `src/platform/mod.rs`, `src/platform/types.rs`
   - Move the `WindowInfo` and `WindowEventInfo` definitions to `src/platform/types.rs` and re-export

2. Add platform traits and small shim
   - Add: `src/platform/traits.rs` or place traits in `platform/mod.rs`
   - Implement a simple `NullBackend` behind `#[cfg(not(any(feature = "backend-sway", feature = "backend-hyprland", feature = "backend-gnome", feature = "backend-plasma", feature = "backend-macos", feature = "backend-windows")))]` which returns no events (useful for `--no-default-features` builds)

3. Move sway code
   - Create `src/platform/sway.rs` with `#[cfg(feature = "backend-sway")]`
   - Copy the existing logic from `src/daemon/sway.rs`, but export an implementation of the traits or provide functions to spawn the relevant monitor threads and return channels

4. Update daemon::mod and run_daemon
   - Replace direct `sway::run_event_loop(config, cache, daemon_state).await?;` with a platform-driven factory call, e.g. `platform::run_backend(config, cache, daemon_state).await?;`
   - Keep the core logic in `src/daemon/state.rs` (unchanged). The backend should only push WindowInfo/LockEvent into channels consumed by DaemonState.

5. Refactor popup.rs
   - Remove swayipc call in `output_center_position` and instead call `platform::output_locator().get_output_center(output_name)` behind an Option
   - Gate any backend-specific placement code behind `#[cfg(feature = "popup-output-placement")]` if desired

6. Split IPC transport and update config
   - Move current `src/ipc/server.rs` and `src/ipc/client.rs` to `src/ipc/unix.rs` and gate with `#[cfg(feature = "ipc-unix")]`
   - Implement `src/ipc/tcp.rs` for cross-platform fallback and gate with `#[cfg(feature = "ipc-tcp")]`
   - Update `src/ipc/mod.rs` to re-export selected implementation via features
   - Update `src/config.rs` default socket to be OS-aware

7. Update Cargo.toml
   - Make swayipc/dbus optional and add features section (example below)

```toml
[dependencies]
swayipc = { version = "3.0", optional = true }
dbus = { version = "0.9", optional = true }

[features]
default = ["backend-sway", "ipc-unix", "popup-ui"]
backend-sway = ["swayipc", "ipc-unix"]
backend-hyprland = []
backend-gnome = ["dbus"]
backend-plasma = ["dbus"]
backend-macos = []
backend-windows = []
ipc-unix = []
ipc-tcp = []
popup-ui = ["eframe"]
popup-output-placement = ["backend-sway"]
```

8. CI changes
   - Add job matrix entries for the builds described above and ensure `cargo check --all-features` is exercised appropriately per matrix axis

Acceptance criteria
-------------------
- `cargo build --no-default-features` succeeds (core only)
- `cargo build --features backend-sway,ipc-unix,popup-ui` succeeds (Sway build)
- `cargo build --no-default-features --features ipc-tcp,popup-ui` succeeds (cross-platform build)
- No usages of swayipc/dbus/winit-specific platform APIs remain in core files (daemon/state.rs, rules.rs, db.rs)
- Popup placement works when a backend provides an OutputLocator; otherwise it falls back silently

Notes and rationale
--------------------
- Keep changes incremental and small: first add the platform traits and a NullBackend, then migrate Sway into new module and wire factories. This reduces risk.
- Prefer channels (mpsc) for event flow between backend and daemon core instead of direct trait calls to simplify ownership and avoid blocking API boundaries.
- Preserve existing runtime behavior by keeping default features pointing at current Sway/Unix setup, then shrink defaults later if desired.

Next steps (if you want me to implement):
1. Create the platform module and move `WindowInfo`/`WindowEventInfo` types there.
2. Create `src/platform/sway.rs` and gate it with feature `backend-sway`.
3. Update `src/daemon/mod.rs` to call into `platform` factory.
4. Make swayipc/dbus optional in Cargo.toml and add features block.

This story documents the migration plan and a clear checklist to implement it.

# LazyTime

## What it is

LazyTime is an automatic, rule-driven time tracking assistant that listens to window focus and title changes and starts/stops trackings accordingly. It aims to work across compositors and operating systems; the core is platform-agnostic and platform-specific integrations are implemented in gated backends.

## Quick Start (current)

Build the project:

```bash
cargo build
```

Run the daemon (Linux):

```bash
./target/debug/lazytime --daemon
```

Open the interactive terminal UI:

```bash
./target/debug/lazytime --tui
```

Print a single JSON object for waybar state:

```bash
./target/debug/lazytime --waybar_state
```

Other modes:

- `--summary` prints a textual summary of today’s trackings (use `--watch` to refresh)
- `--report` generates a report for a date range
- `--jira-sync` runs scheduled Jira synchronization (if configured)

## Configuration

- Default config path: `~/.config/lazytime/config.json` (created automatically on first run if missing)
- Default DB path: `~/.local/share/lazytime/lazytime.db` (or derived next to the config file if data dir unavailable)
- You can override config path with `--config path/to/config.json`.

## Config & Data File Locations By OS

The defaults below are used when not overridden in the config or via CLI. You can always set `ipc_socket_path` in the config to change the IPC location.

- Linux (default):

  - Config: `~/.config/lazytime/config.json`
  - Data DB: `~/.local/share/lazytime/lazytime.db`
  - IPC (unix socket, default when `ipc-unix`): `~/.local/run/lazytime.sock` or the path set in `ipc_socket_path`

- macOS:

  - Config: `~/Library/Application Support/lazytime/config.json` (when available) or `~/.config/lazytime/config.json`
  - Data DB: `~/Library/Application Support/lazytime/lazytime.db` or `~/.local/share/lazytime/lazytime.db` fallback
  - IPC (recommended on macOS): loopback TCP (e.g., `127.0.0.1:43123`) when `ipc-tcp` is enabled. If `ipc-unix` is used, a path under `/tmp` will be used.

- Windows:
  - Config: `%APPDATA%\lazytime\config.json` (e.g., `C:\Users\User\AppData\Roaming\lazytime\config.json`)
  - Data DB: `%LOCALAPPDATA%\lazytime\lazytime.db` (e.g., `C:\Users\User\AppData\Local\lazytime\lazytime.db`)
  - IPC: loopback TCP (recommended for cross-process on Windows) when `ipc-tcp` is enabled (e.g., `127.0.0.1:43123`). If `ipc-unix` is selected (rare on Windows), a fallback path under `%TEMP%` will be used.

Notes:

- The application will try to pick sensible OS-standard locations via the `dirs` crate. If a preferred location is not available (e.g., `dirs::data_local_dir()` returns None), LazyTime falls back to relative paths next to the config file.
- To explicitly set the IPC endpoint, edit `ipc_socket_path` in your config or set the environment/CLI options as documented in the stories.

## IPC

LazyTime uses an IPC channel to accept runtime notifications (for example to reload project rules). On Linux it currently uses a Unix domain socket. Cross-platform support (loopback TCP) is planned as an alternate transport.

## Troubleshooting stale daemon lock

LazyTime stores a daemon runtime lock in the DB (`config_store.key = "daemon_runtime_lock"`) to prevent multiple daemon instances. If a daemon crashes, this lock can remain and block start/stop operations.

1. Confirm no daemon is running:

```bash
pgrep -af "lazytime.*--daemon"
```

2. Inspect the lock value:

```bash
sqlite3 ~/.local/share/lazytime/lazytime.db "SELECT key, value, last_updated FROM config_store WHERE key='daemon_runtime_lock';"
```

3. If step 1 returned nothing, remove the stale lock row:

```bash
sqlite3 ~/.local/share/lazytime/lazytime.db "DELETE FROM config_store WHERE key='daemon_runtime_lock';"
```

4. Start daemon again (`--daemon`) or from the TUI daemon view.

Notes:

- Adjust the DB path if you set a custom `db_file` in your config.
- On Windows, run the same SQL against your `%LOCALAPPDATA%\\lazytime\\lazytime.db` path using your preferred SQLite client.

## Popup UI

The resume/reminder popups are implemented with egui/eframe. The popup placement uses compositor-specific APIs when available to attempt output-aware placement. You can force the popup backend on Linux with `LAZYTIME_POPUP_BACKEND=wayland` or `LAZYTIME_POPUP_BACKEND=x11`.

## Logging

You can increase verbosity with the CLI `--loglevel` flag (e.g. `--loglevel=DEBUG`) or by setting `RUST_LOG` in your environment. The binary will respect either and configure tracing accordingly.

## Platform Backends (planned & stories)

The project is being refactored so all compositor/OS-specific code lives behind a small platform abstraction. The following implementation stories document the plan and details for each platform integration:

- Platform abstraction and feature-gating plan: `IMPLEMENTATION_STORIES/18_platform_backends.md`
- Linux desktop (GNOME & Plasma) backend using AT‑SPI + X11 fallback: `IMPLEMENTATION_STORIES/19_linux_desktop.md`
- Windows backend (SetWinEventHook + WTS notifications): `IMPLEMENTATION_STORIES/20_windows_backend.md`
- macOS backend (AXObserver + CGWindowList fallback): `IMPLEMENTATION_STORIES/21_macos_backend.md`

These stories describe the decisions, APIs, feature flags, and migration checklists. They are the authoritative design notes for cross-platform support.

## Rules and app_id semantics

- Rules map window attributes (app_id, title regex) to projects.
- For consistency across platforms, `app_id` semantics are:
  - Windows: full normalized executable path (fallback to basename)
  - macOS: prefer `CFBundleIdentifier` (fallback to executable path)
  - Linux (AT‑SPI/X11): use toolkit/compositor-provided app id or WM_CLASS
- Special wildcard: a stored rule with `app_id == "*"` means “match any application; apply rule by title only”. This wildcard is treated like a fallback, title-only rule on all backends.

## Development & feature flags (roadmap)

The repository is moving toward a small set of compile-time backend features (examples in the stories):

- `backend-sway` — Sway-specific subscriber (existing implementation)
- `backend-linux` — GNOME/Plasma desktop backend (AT‑SPI + X11 fallback)
- `backend-windows` — Windows backend using native Win32 hooks
- `backend-macos` — macOS backend using Accessibility APIs
- `ipc-unix` / `ipc-tcp` — IPC transports
- `popup-ui` — GUI popups (egui/eframe)

Note: these feature names are documented in the implementation stories and will be introduced incrementally. Consult the stories for exact Cargo feature wiring and CI matrix recommendations.

## Where to look next

- Code: `src/daemon`, `src/rules.rs`, `src/popup.rs`, `src/ipc`
- Stories: `IMPLEMENTATION_STORIES/*.md` (especially 18..21 for platform work)
- Specification: `SPEC.md` for behavioral details

## Contributing

- Follow the incremental migration checklist in the platform stories: add small, buildable changes that keep `cargo build --no-default-features` passing.
- Keep platform-specific dependencies contained in `src/platform/*` so the core code remains portable.

Example: build core-only

```bash
cargo build --no-default-features
```

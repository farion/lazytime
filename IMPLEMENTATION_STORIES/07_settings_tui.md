Title: TUI Settings View

Summary
- Add a new TUI view opened with the key `x` for editing the application's JSON config.
- The view is a scrollable form with one entry per Config option. Required fields have an asterisk.
- The working_hours field is edited via a structured modal with weekdays by name (Sun..Sat).
- Save (`s`) validates and writes the config JSON back to the same path the app was started with and notifies other components best-effort. Reset (`r`) reloads the form from disk.

Design
- New module: `src/tui/settings.rs` implementing `SettingsState` and a `WorkingHoursModal`.
- Small API change: `tui::run` now accepts `config_path: Option<&str>` so the settings view can persist to the correct file.
- Jira token is masked by default in the UI with a reveal toggle.

Field mapping
- All Config fields are present. `default_project` is required (label `default_project*`).
- Numeric fields presented as numeric textboxes. Optional string fields accept empty string -> None.

Working hours editor
- Modal-based editor with weekdays shown by name on the left and a per-day list of time ranges on the right.
- Controls: Up/Down to navigate, Left/Right to switch focus, `a` add, `d` delete, Enter edit range, `s` save modal, Esc cancel modal.

Save & Reset
- Save: assemble a Config struct, call `Config::validate()`, write pretty JSON to the config path (creating parent directory if needed), and set a status message.
- Save will also attempt an IPC notify (best-effort) to signal other components; some changes may still require restart.
- Reset: reload config from disk using `Config::from_path(Some(path))` and reset the form fields.

Notes
- Implementation favors minimal, robust edits. Full hot-reload of all subsystems would need additional IPC handlers and is out of scope for this story.

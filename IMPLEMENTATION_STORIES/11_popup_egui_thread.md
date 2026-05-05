Title: Real egui Popup in Dedicated Thread

Summary
Replace the current popup placeholder behavior with a real egui modal running in a dedicated thread, returning Yes/No/Snooze via in-process channel.

Acceptance Criteria
- Popup thread launches a real egui window and renders the configured prompt text.
- Buttons Yes/No/Snooze are clickable and send `PopupAction::{Yes,No,Snooze}` through channel.
- Daemon consumes the action and applies expected scheduling/tracking behavior.
- Popup can target active output/screen as best-effort and closes cleanly after selection.

Tasks
1. Implement egui app state and event loop in `src/popup.rs`.
2. Wire thread-safe action callback from egui UI to daemon channel.
3. Add timeout/close handling (treat close as No by default, documented).
4. Integrate popup lifecycle logging in daemon.
5. Add tests for action handling and close/default path.

Files/Modules
- src/popup.rs
- src/daemon/sway.rs
- src/daemon/state.rs

Estimate: 4-8 hours

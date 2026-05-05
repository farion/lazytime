Title: Daemon: SwayIPC Subscription and Tracking Logic

Summary
Implement the long-running daemon that subscribes to Sway window events, detects project changes using the in-memory rules, applies the stability debounce logic, and persists trackings in the database.

Acceptance Criteria
- The daemon subscribes to sway window events and receives focus change events.
- When a stable project change is detected (per tracking_stability_seconds), the daemon closes the previous tracking and inserts a new tracking row with window fields recorded.
- When no rule matches, the daemon starts a tracking for config.default_project.
- The daemon respects working_hours when scheduling popup reminders.

Tasks
1. Implement sway subscription loop (src/daemon/sway.rs) using swayipc-rs (or plain IPC) and parse relevant fields.
2. Integrate with RuleSet to detect projects from events.
3. Implement tracking state machine with debounce (last_detected_project / last_detected_at) and atomic DB transactions for closing/starting trackings.
4. Implement reminder scheduling logic tied to working_hours and popup launching (spawn egui thread).
5. Add integration tests (where feasible) and unit tests for the state machine.

Files/Modules
- src/daemon/sway.rs
- src/daemon/state.rs

Estimate: 8-12 hours

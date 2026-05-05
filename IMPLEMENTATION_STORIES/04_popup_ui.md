Title: Popup Reminder UI (egui) Thread

Summary
Create an egui-based popup running in a dedicated thread that displays Yes/No/Snooze and reports the chosen action back to the daemon.

Acceptance Criteria
- The daemon can spawn a popup thread with context (output identifier and prompt metadata).
- The popup shows the three buttons and sends the result back over an in-process channel.
- The daemon recognizes the response and acts accordingly (start tracking or schedule next reminder).

Tasks
1. Create a popup module (src/popup.rs) that runs egui in a dedicated thread.
2. Implement callback mechanism using mpsc channel for popup result events (Yes/No/Snooze).
3. Ensure the popup positions on the desired Sway output using available windowing options (best effort).
4. Add tests for popup result handling.

Files/Modules
- src/popup.rs

Estimate: 4-6 hours

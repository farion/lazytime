# Daemon Lock/Resume Story

Summary
- When the screen is locked or the system suspends, the daemon must stop the active tracking, log the event, and remember a lightweight in-memory paused record. After unlock/resume, the daemon must spawn a daemon-owned egui dialog (not the GUI module) on the active output asking the user to: continue from lock time, continue from now, or ignore. All lock/unlock signals and user choices are logged at info level. No DB schema changes; paused state is in-memory only.

Motivation
- Prevent tracking time being incorrectly recorded while the user is away and give the user a simple way to decide whether the gap should be merged into the previous tracking or treated as a break and resumed from now. Keep a clear audit trail via logs.

Scope
- In scope:
  1. Detect lock and suspend signals (session and system signals).
  2. Close the active tracking when lock/suspend occurs (set end_ts = now).
  3. Store an in-memory PausedTracking in DaemonState.
  4. On unlock/resume, spawn a daemon-owned popup (popup.rs) that asks: continue from lock time, continue from now, or ignore.
  5. Apply the chosen action using existing db helpers (reopen old tracking, start new tracking, or nothing).
  6. Emit structured tracing::info! logs for every signal, DB action, popup spawn, and user choice.
  7. Use dbus::blocking on a dedicated thread to listen for signals.
- Out of scope:
  1. Persisting paused state across daemon restarts.
  2. Reusing or modifying GUI module code for daemon dialogs.
  3. Adding a lock event DB table (logs only unless requested later).

Design

Detection
- Run a dedicated lock-monitor thread that uses dbus::blocking to listen on:
  - Session bus: org.freedesktop.ScreenSaver ActiveChanged(bool)
  - System bus: org.freedesktop.login1.Manager PrepareForSleep(bool)
- When a signal is received emit tracing::info! and send a small LockEvent (Locked/Unlocked) to the daemon via mpsc.

Pausing
- On Locked or PrepareForSleep(true): if an active tracking exists, set its end_ts = now (db::update_tracking_times) and store PausedTracking { id, project_name, start_ts, paused_at, output } into DaemonState.paused. Log the action.

Resume dialog
- On Unlocked or PrepareForSleep(false): if DaemonState.paused.is_some(), spawn spawn_resume_popup_thread(request, tx_resume) in popup.rs (same spawn-thread pattern as existing reminders).
- ResumePopupApp (daemon-only) presents three buttons:
  - Continue from lock time (reopen paused tracking and remove end_ts)
  - Continue from now (start a new tracking with the same project at now)
  - Ignore (do nothing)
- Popup sends ResumeAction back over an mpsc channel to the daemon.
- Log dialog spawn and user selection.

Applying choice
- Continue from lock time: call db::update_tracking_times(conn, paused.id, paused.project_name, paused.start_ts, None) — reopen the tracking (remove end_ts).
- Continue from now: call db::start_tracking(conn, paused.project_name, "daemon", ...) with now; keep old row closed.
- Ignore: no DB change.
- Log each action at info level.

Placement
- Attempt to place the popup on the recorded output by querying sway outputs (swayipc) for geometry and setting window position if possible. If not possible, fall back to letting the compositor place the window. Keep placement logic inside popup.rs so GUI module is not touched.

Error handling / fallback
- If popup-ui feature is disabled or eframe fails to create a window, fallback to doing nothing (log the failure and treat the choice as Ignore). This mirrors existing reminder fallback behavior.
- If DB calls fail, log error and preserve the paused state so the user can retry or the daemon can recover safely.

Files to change (summary)
- src/daemon/state.rs
  - Add PausedTracking struct.
  - Add Option<PausedTracking> paused field and small helpers (mark_paused, take_paused).
  - Add tracing::info! calls where paused is set/taken.
- src/daemon/sway.rs
  - Add lock-monitor thread (dbus::blocking) producing LockEvent via mpsc.
  - Wire rx_lock.try_recv() into the existing run_event_loop loop alongside rx_popup.
  - On lock: close active tracking, store paused state, log.
  - On unlock: spawn resume popup (popup::spawn_resume_popup_thread) and handle responses over a resume channel; apply DB actions accordingly and log.
- src/popup.rs
  - Add ResumeAction enum and ResumePopupRequest struct.
  - Add spawn_resume_popup_thread similar to spawn_popup_thread.
  - Add ResumePopupApp eframe::App implementation that shows the three buttons, sends chosen ResumeAction, and closes the window. Attempt output placement using swayipc output geometry.
  - Add tracing::info! logs for popup spawn and chosen action.
- Cargo.toml
  - Add dependency: dbus = "0.10" (blocking) — used only for the monitor thread.

Acceptance Criteria
- Lock or suspend signals produce tracing::info logs with a consistent format (for example: "lock_event: type=locked source=ScreenSaver output=... time=...").
- When a lock/suspend occurs and a tracking is active, that tracking has end_ts set to the lock time and a "tracking_paused" info log is emitted.
- On unlock/resume, the daemon spawns a resume popup (or logs a clear fallback if popup-ui is missing or fails).
- User selection results in the correct DB action and an info log recording the choice and applied action.
- No changes are made to GUI code; daemon popups use popup.rs only.
- No DB schema changes and no persistence of paused state across restarts.

Tests
- Unit:
  1. DaemonState tests for mark_paused / take_paused.
  2. Popup thread unit tests: create a ResumePopupApp instance and simulate the button click handler to confirm it sends the right ResumeAction on click (where practical).
- Integration (manual recommended):
  1. Start daemon with popup-ui feature.
  2. Start a tracking, lock the screen (swaylock or loginctl lock-session). Confirm:
     - active tracking row now has end_ts ~= lock time,
     - Daemon emitted lock_event and tracking_paused logs.
  3. Unlock; confirm resume dialog appears on expected output:
     - Click Continue from lock time: confirm the original tracking end_ts removed (open), resume_action log emitted.
     - Click Continue from now: confirm a new tracking row is added with start_ts ~= unlock time and resume_action log emitted.
     - Click Ignore: confirm no DB changes beyond initial closure.
  4. Test suspend/resume (PrepareForSleep) similar to lock/unlock.
  5. Test when no active tracking is present: lock/unlock should not spawn dialog and should log the signals.
- Automated:
  1. A test harness can inject LockEvent messages into the run_event_loop channel and assert DB and log side effects.

Logging format (examples)
- lock_event: type=locked source=ScreenSaver output=HDMI-A-1 time=2026-05-01T12:34:56Z
- tracking_paused: id=42 project="Alpha" start_ts=2026-05-01T09:00:00Z paused_at=2026-05-01T12:34:56Z output=HDMI-A-1
- resume_dialog: spawned for tracking id=42 project="Alpha" paused_at=2026-05-01T12:34:56Z output=HDMI-A-1
- resume_choice: id=42 project="Alpha" choice=ContinueFromNow choice_time=2026-05-01T12:35:14Z
- resume_action: started new tracking project="Alpha" start_at=2026-05-01T12:35:14Z replaced_paused_id=42

Edge cases & notes
- The dbus signals may not be emitted by every lock implementation; PrepareForSleep covers suspend/lid-close, ActiveChanged covers common lockers. If missing signals are observed in deployments, add alternate hooks.
- Multiple rapid signals: treat a new lock event while paused as a no-op (keep the paused record unchanged); log duplicates.
- Daemon restart while paused: paused state is lost (the closed tracking remains closed). If persistence is desired later, add a small DB flag/table.
- If eframe/window placement requires extra winit feature flags, implement best-effort center-on-output and fall back gracefully.
- If popup fails to start (feature disabled), log the failure and leave paused tracking closed.

Estimated effort
- Implementation: 2–4 hours.
- Manual testing: 30–60 minutes.
- Extra time if placing the window reliably per-output requires dealing with winit/eframe internals.

Suggested commit message (conventional)
- feat(daemon): pause tracking on lock/suspend and add resume dialog; log lock/unlock events

Next steps
1. Implement the changes (files listed above). This story is persisted in the repository so it can be referenced and implemented.
2. If you want paused state persisted across restarts or lock events recorded in the DB, open a follow-up story and I will add a migration and DB writes.

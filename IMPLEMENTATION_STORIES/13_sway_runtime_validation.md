Title: Sway Runtime Event Validation and Field Completion

Summary
Validate and harden Sway event parsing against real runtime payloads; ensure workspace/output extraction is implemented and persisted.

Acceptance Criteria
- Daemon correctly extracts app_id, instance, class, title, workspace, and output from focus/window events.
- Event handling is robust to null/missing fields and non-focus window events.
- Extracted workspace/output are saved in tracking rows.
- Added debug logs are sufficient to troubleshoot detection mismatches.

Tasks
1. Capture sample sway events (with and without xwayland) and document parsing assumptions.
2. Update parsing logic to handle all expected event variants safely.
3. Persist workspace/output on tracking start/switch.
4. Add unit/integration tests for parsing edge cases.
5. Add runtime guardrails for malformed events (skip with warning, do not crash).

Files/Modules
- src/daemon/sway.rs
- src/daemon/state.rs
- src/db.rs
- tests/daemon_state.rs (extend)

Estimate: 6-10 hours

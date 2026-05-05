Title: Rule Cache Reload & Signaling

Summary
Implement the IPC-driven mechanism the daemon uses to detect project/rule changes written by the TUI/CLI: on each change the TUI/CLI sends a JSON notification to a UNIX domain socket; the daemon replies with an ack and reloads rules atomically.

Acceptance Criteria
- After the TUI/CLI updates mapping tables, it sends an IPC message {"type":"projects_updated","timestamp":"..."} to the daemon. The daemon replies {"status":"ok"}.
- The daemon reloads rules immediately upon receiving the notification and swaps the in-memory RuleSet atomically.
- The daemon handles multiple rapid notifications gracefully (coalescing if necessary).

Tasks
1. Implement an IPC client helper used by the TUI/CLI to send notifications and wait for ack (src/ipc/client.rs).
2. Implement an IPC server in the daemon that listens on a configurable UNIX domain socket and dispatches notifications (src/ipc/server.rs).
3. Ensure reload swaps in-memory RuleSet atomically (RwLock or similar) to avoid races.
4. Add tests that send IPC notifications and verify the daemon reloads rules.

Files/Modules
- src/ipc/client.rs
- src/ipc/server.rs
- src/daemon/reload.rs (invokes rules.reload())

Estimate: 4-6 hours

Title: Extended Tests for CLI Reports and Daemon Integration

Summary
Add richer automated coverage for report output correctness and daemon integration behavior (event flow, debounce transitions, IPC-triggered rule reload).

Acceptance Criteria
- Report tests verify aggregation correctness across date boundaries and open/closed trackings.
- Daemon integration tests validate project switching with debounce timings.
- IPC notification tests verify rule reload affects subsequent detection logic.
- CI includes these tests and remains stable.

Tasks
1. Add deterministic fixtures for report query testing.
2. Add integration tests simulating event sequences and expected tracking transitions.
3. Add integration test for IPC `projects_updated` + rule cache swap behavior.
4. Capture and assert key CLI output lines for `--report` and `--summary`.
5. Tune CI workflow runtime and reliability (timeouts/retries where needed).

Files/Modules
- tests/daemon_integration.rs (new)
- tests/report_output.rs (new)
- tests/ipc_notify.rs (extend)
- .github/workflows/ci.yml

Estimate: 6-10 hours

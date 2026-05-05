Title: Jira Sync (CLI + TUI)

Summary
Implement a repeatable Jira sync that pushes unsynced, finished trackings from the local database to Jira.
This includes a CLI entrypoint (--jira-sync), a TUI view (open with `j`) where the user can start the sync with `s`, unified logging for both CLI and TUI, and a cross-process lock so only one sync runs at a time.

High-level behavior
- The sync takes all finished trackings where jira_synced = 0.
- For each tracking:
  - Resolve the project's sap_number via the projects table.
  - If sap_number is missing: emit a WARNING for that tracking and skip it (do not abort the whole run).
  - Search Jira for an existing issue in config.jira_project matching the SAP field and the current user using JQL that contains `assignee = currentUser()`.
  - If no issue found, create a new issue in config.jira_project with minimal fields (project, summary, description, issuetype, the configured SAP field) and assign it to the authenticated user.
  - Add a worklog to the issue with the tracking's start and the computed duration (end - start). Use RFC3339/ISO timestamps including timezone for the worklog `started` field.
  - On successful worklog creation, mark the tracking as synced (db::mark_synced) and store the jira_issue_key and jira_worklog_id.

Auth and API details
- Support Basic auth (email:api_token) when `config.jira_email` is set; otherwise fall back to bearer token using `config.jira_token`.
- Use the Jira Cloud pattern of getting the authenticated user's accountId via GET /rest/api/3/myself and assign created issues using `assignee: { accountId: "..." }`.
- When searching issues use a JQL like (example):
  project = CP1234 AND "SAP-Nr-Projektaufgabe[Short text]" ~ "SAP-42" AND assignee = currentUser() ORDER BY updated DESC
- `config.jira_sap_field` may be either a human-readable name with spaces (e.g., "SAP-Nr-Projektaufgabe[Short text]") or a customfield id (e.g., `customfield_10054`). The implementation must handle quoting the field name in JQL when necessary.

Concurrency and trigger semantics
- CLI trigger: `lazytime --jira-sync` (existing flag). CLI runs the unified runner and prints the same textual logs the TUI would show.
- TUI trigger: press `j` to open the Jira Sync view. In that view press `s` to start the sync. The view shows a scrolling log area and a footer with progress (processed/total and current status). If a sync is already running it must not be possible to start another one (the UI must set a running flag immediately on `s`).
- Cross-process mutual exclusion: use the existing `config_store` table as a lock store. Use a unique `key` value (e.g., `jira_sync_lock`) and attempt an `INSERT`. If insert fails due to uniqueness, treat that as "another sync is running". On normal completion, delete the lock row. Document stale-lock handling (see notes).
- Dry-run (`--dry-run`): simulate the actions and print the same messages but do not modify the DB. Dry-run should not acquire the global DB lock (so it doesn't block a real sync).

Retry and error handling
- API calls implement retries with exponential backoff for transient failures (network errors, HTTP 5xx, and 429). Default: up to 3 attempts with backoffs 500ms, 1s, 2s.
- Per-tracking failures are logged and do not mark the tracking as synced. The runner continues with remaining trackings.
- If a tracking has non-positive duration or invalid timestamps, log an error and skip that tracking.

Logging and UI
- CLI and TUI must display identical log lines for the same events. The runner will send structured events and a single textual formatter will be used by both the CLI and TUI to produce the same lines.
- The TUI Jira Sync view displays a scrollable list of log lines and a footer with progress: "Processed: X / Y — STATUS". The log buffer is capped to a reasonable number of lines (e.g., last 2000) to avoid unbounded growth.

Acceptance criteria
1. `lazytime --jira-sync` performs the sync and prints per-tracking messages. On success each synced tracking has jira_synced=1 and jira_issue_key/jira_worklog_id set.
2. TUI: pressing `j` opens the Jira Sync view; pressing `s` starts a sync, showing logs and progress in real time. While running, pressing `s` again does nothing.
3. When a project's sap_number is missing, the runner logs a WARNING for that tracking and continues (other trackings are processed).
4. Search JQL uses `assignee = currentUser()`; no accountId required for searching. Created issues are assigned to authenticated user via GET /rest/api/3/myself -> accountId.
5. Worklog `started` is sent in RFC3339/ISO with timezone; `timeSpentSeconds` is used for duration.
6. Transient network/API failures are retried up to 3 times with exponential backoff; retried attempts are logged.
7. CLI and TUI display identical textual messages for the same events.
8. Dry-run prints intended actions and does not write to DB or acquire the global lock.

Tasks
1. Add/extend config
   - Add optional `jira_email: Option<String>` to `Config` to support Basic auth (email:api_token).
   - Document `jira_sap_field` examples (human-readable name or `customfield_XXXXX`).

2. DB lock helpers (src/db.rs)
   - Add helpers: `try_acquire_jira_sync_lock(conn: &Connection) -> Result<bool>` and `release_jira_sync_lock(conn: &Connection) -> Result<()>` that use `config_store` with key `jira_sync_lock`.
   - Optionally add a `jira_sync_lock_timestamp(conn)` helper for stale-lock inspection.

3. Jira runner and events (new module or extend src/jira.rs)
   - Factor the existing CLI implementation into a reusable async runner that accepts an optional event sink (channel sender).
   - Define `JiraSyncEvent` enum (Log(String), Progress{processed, total, current_tracking_id}, Finished(Result<()>)).
   - Runner responsibilities:
     - Optionally acquire DB lock (unless dry-run).
     - list_unsynced_finished, compute total, emit initial Progress event.
     - For each tracking: emit Log("processing ..."), find/create issue, add worklog, mark_synced, emit Log("synced ...") and Progress.
     - On per-tracking error emit Log("error ...") and continue.
     - Release lock on completion (in finally block) and send Finished event.
   - Implement API request helper with Basic or Bearer auth selection and retry/backoff behaviour.
   - Ensure JSON payload keys match Jira API expectations (camelCase: `maxResults`, `timeSpentSeconds`, etc.) when building the request. Prefer building serde_json::Value/Map explicitly to avoid serde rename surprises.

4. CLI integration (src/main.rs)
   - Replace the ad-hoc run_jira_sync with a thin wrapper that calls the new runner and prints event lines to stdout in the same textual format.

5. TUI view (new file src/tui/jira_sync.rs + wire into src/tui/mod.rs)
   - Add a new ViewMode variant `JiraSync` and a TUI module that renders the logs area and footer progress.
   - Key `j` opens the view. In the view key `s` starts the sync. `s` is ignored if a run is already in progress.
   - When `s` is pressed spawn a background thread that creates a tokio runtime, runs the async runner, and forwards `JiraSyncEvent`s to the UI via a std::sync::mpsc channel.
   - UI polls the receiver in its render loop and appends incoming logs and updates progress.

6. Tests (tests/jira_client.rs and new tests)
   - Add tests that mock the search endpoint to assert the JQL contains `currentUser()` and that the configured SAP field (quoted when needed) appears in the body.
   - Add tests that mock the worklog endpoint and assert the presence and format of `started` (RFC3339 w/ timezone) and `timeSpentSeconds` fields.
   - Add tests for retry behaviour by returning 500 on first request(s) and 200 afterwards.
   - Add tests to validate DB lock helpers (try_acquire_jira_sync_lock and release) using an in-memory sqlite connection.

7. Documentation
   - Update SPEC.md and the default config template to document `jira_email`, `jira_sap_field` examples, CLI flag usage, and the TUI key `j`.

Implementation notes and small details
- JQL quoting: when `jira_sap_field` contains characters other than [A-Za-z0-9_] (spaces, hyphens, brackets), quote it with double quotes in the JQL string. If it is `customfield_XXXXX` no quotes are necessary.
- Search JSON must use `maxResults` (camelCase). Build the search payload using serde_json::json! with explicit keys.
- Use `GET /rest/api/3/myself` once at the runner start (cache the accountId) for assigning created issues.
- Use chrono DateTime::to_rfc3339() for the worklog started timestamp.
- Dry-run: do not call create_issue/add_worklog/db::mark_synced and do not acquire the cross-process lock.

Optional / follow-ups (not required for MVP)
- Add stale-lock takeover policy (e.g., if existing lock timestamp is older than configurable threshold, treat as stale and allow takeover). Default MVP behaviour: do not implement takeover; require manual removal of stale lock.
- Add cancellation support in TUI (allow user to abort running sync). This requires cooperative cancellation in the async runner.

Files / Modules to change
- src/config.rs (add jira_email, document jira_sap_field)
- src/db.rs (add lock helpers)
- src/jira.rs (extend client helper / retries) or add src/jira_sync.rs (runner + events)
- src/main.rs (CLI wrapper)
- src/tui/mod.rs (add ViewMode::JiraSync + key `j`)
- src/tui/jira_sync.rs (new TUI view)
- tests/jira_client.rs (extend tests)
- SPEC.md, IMPLEMENTATION_STORIES/06_jira_sync.md (update / link to this detailed story)

Estimate
- Implementation, tests, and basic UI: 6-10 hours. More time if stale-lock takeover or cancellation is required.

Notes
- The detailed implementation will prefer small, local changes and keep the existing structure where possible. The runner will reuse the existing Jira client where feasible but add a robust request helper to standardize auth, retries, and JSON construction.

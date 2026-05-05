Title: CLI: Summary, Report, Watch

Summary
Implement CLI commands and flags required by the spec: --summary, --watch, --report, and --jira-sync.

Acceptance Criteria
- `lazytime --summary` prints today's trackings in a table.
- `lazytime --summary --watch` refreshes every summary_update_seconds.
- `lazytime --report --start YYYY-MM-dd --end YYYY-MM-dd` prints aggregated hours per day/project.
- `lazytime --jira-sync` pushes unsynced trackings to Jira (respecting dry-run flag).

Tasks
1. Implement CLI parsing (structopt/clap) and dispatch to flags.
2. Implement summary output formatting (text table) and watch loop.
3. Implement report aggregation SQL queries.
4. Implement Jira sync logic invocation from CLI (tie into Jira module).
5. Add unit/integration tests for CLI behavior.

Files/Modules
- src/bin/main.rs
- src/cli.rs

Estimate: 4-6 hours

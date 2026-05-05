Title: DB Initialization & Schema

Summary
Create SQLite initialization and migration code that ensures the database schema from SPEC.md exists and can be created on first run. Provide a small migration mechanism for future changes.

Acceptance Criteria
- On startup (daemon or CLI/TUI/CLI tools), code creates the SQLite file if absent and runs migrations to ensure tables: projects, project_rules, trackings, config_store exist.
- Migrations are idempotent and logged.
- No dedicated init command is required; initialization happens automatically on first run.

Tasks
1. Add a DB module (src/db.rs) that opens sqlite (from config.db_file) and runs migrations.
2. Implement migrations using embedded SQL files or hard-coded statements matching SPEC.md schema.
3. Ensure every entry point (daemon, summary, report, jira-sync, TUI) opens the DB and runs migrations before use.
4. Add unit tests for the migration runner (create temp DB, run migration, assert tables exist).

Files/Modules
- src/db.rs
- src/bin/main.rs (CLI flag handling)
- migrations/ (optional SQL files)

Estimate: 3-5 hours

Title: TUI Interactive CRUD for Trackings, Projects, and Rules

Summary
Implement fully interactive TUI workflows (not just read-only views) for managing trackings, projects, and rules, including DB writes and validations.

Acceptance Criteria
- Trackings: user can add/edit/delete entries (project, start, end) from TUI day view.
- Projects: user can add/edit/delete project name and sap_number.
- Rules: user can add/edit/delete/reorder rules with precedence updates.
- Regex is validated before save; invalid regex shows error and does not persist.
- All successful writes persist in SQLite and are reflected in UI without restart.

Tasks
1. Add keyboard/mouse navigation state and selection model.
2. Implement modal forms for Add/Edit operations.
3. Implement delete confirmation dialogs for projects/rules/trackings.
4. Implement precedence reorder operations (move up/down) and DB updates.
5. Add form/input validation and user-facing error messages.

Files/Modules
- src/tui/mod.rs
- src/tui/trackings.rs
- src/tui/projects.rs
- src/db.rs

Estimate: 10-18 hours

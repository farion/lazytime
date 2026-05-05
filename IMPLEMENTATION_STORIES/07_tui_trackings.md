Title: TUI: Trackings Management

Summary
Implement the ratatui-based TUI for viewing and editing trackings: day view, range view, filtering, and per-tracking edit actions.

Acceptance Criteria
- The TUI lists trackings for a selected day and allows editing start/end/project and deleting trackings.
- The range view aggregates hours per project over a selected range.
- CRUD operations update the SQLite trackings table.

Tasks
1. Create TUI main loop (src/tui.rs) with navigation and panes.
2. Implement day view with per-tracking actions (edit/delete/change project).
3. Implement range view with aggregation queries.
4. Add keyboard and mouse support for common actions.
5. Add unit tests for DB interactions called by the TUI.

Files/Modules
- src/tui.rs

Estimate: 8-16 hours

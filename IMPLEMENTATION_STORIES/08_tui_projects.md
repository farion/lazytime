Title: TUI: Project & Rules Management

Summary
Implement the project and rule management screens in the TUI as specified: list/add/edit/delete projects and rules, test regex, reorder rules, and update config_store.projects_updated_at.

Acceptance Criteria
- Projects list shows all projects with name and sap_number. Add/Edit/Delete works and persists to DB.
- Rules list for a selected project is editable; Add/Edit/Delete/Move Up/Move Down updates project_rules table and precedence values.
- After any change, config_store.projects_updated_at is updated so the daemon reloads rules.

Tasks
1. Implement projects list pane with Add/Edit/Delete.
2. Implement project detail pane showing rules with reordering and test input.
3. Implement modals for Add/Edit project and Add/Edit rule with validation and regex test.
4. Ensure atomic DB updates and update config_store.projects_updated_at after changes.
5. Add tests for DB updates and config_store modifications.

Files/Modules
- src/tui/projects.rs (or integrated in src/tui.rs)

Estimate: 6-12 hours

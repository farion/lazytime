Visual Day View & Project Color Support
======================================

Goal
----
Add a "Visual Day" Gantt-like view to the GUI and persistent project color support so trackings are shown as colored bars with drag/resize interactions.

Confirmed Decisions
-------------------
- Clamp drag/resize strictly to the selected day.
- Project.color stored as hex string `#RRGGBB` (TEXT, nullable).
- DB migration `004_project_color` is accepted.
- Overlap prevention applied globally (no two trackings may overlap on the same day).
- Overlap handling: reject & revert on drop; show invalid (red) feedback while dragging.
- Drag/resize rounding: 1 minute granularity. Minimum tracking length: 1 minute.
- Commit changes automatically on drag-end when placement is valid.
- Deterministic color generator for missing colors (no external randomness crate).

Migration
---------
Migration id: 004_project_color

SQL:

  ALTER TABLE projects ADD COLUMN color TEXT;

Notes:
- Column is nullable. Backfill colors in application code (generate deterministic color from project.name) rather than in SQL.

DB API Changes (summary)
------------------------
- Add `color: Option<String>` to Project struct.
- db::add_project(conn, name, sap_number, color: Option<&str>)
- db::update_project(conn, id, name, sap_number, color: Option<&str>)
- db::project_color(conn, project_id) -> Option<String>
- db::list_trackings_for_date(conn, date) -> Vec<Tracking> (include project name + color)
- db::update_tracking_times(conn, tracking_id, new_start_ts, new_end_ts)
- db::has_overlap_for_day(conn, day_start_ts, day_end_ts, exclude_tracking_id, candidate_start, candidate_end) -> bool

Color Helpers
-------------
Purpose: convert hex <-> egui::Color32 and generate deterministic fallback color from project name.

API (suggested)
- fn color32_from_hex(hex: &str) -> Option<egui::Color32>
- fn hex_from_color32(col: egui::Color32) -> String  // `#RRGGBB`
- fn generate_color_for_name(name: &str) -> String  // deterministic `#RRGGBB`

Generator algorithm (implementation note):
- Use a small deterministic hash (FNV-1a 32-bit) over UTF-8 bytes of project name.
- Map hash -> hue = hash % 360, fix S/L (e.g. S=60%, L=50%), convert HSL->RGB and output hex.

Visual Day View (spec)
----------------------
File: src/gui/views/visual_day.rs (new)

Rendering:
- Horizontal timeline mapping seconds-since-midnight -> x pixels (00:00..24:00).
- Bars: one per tracking (clamped to selected day); color from projects.color or generator fallback.
- Working-hours shading from config.working_hours for that weekday.

Interactions:
- Hover -> tooltip (project, start, end, duration, comment).
- Left-click -> open tracking edit modal (duplicate existing modal into visual_day.rs initially).
- Right-click -> context menu (Edit / Delete / Copy / Storno).
- Drag-move: drag body to move interval, preserve duration, clamp to day, round to 1 minute, minimum 1 minute.
- Resize: drag left/right edge to change start/end with same constraints.
- While dragging/resizing: query db::has_overlap_for_day; if overlapping show invalid red state.
- On drag-end: if valid -> db::update_tracking_times(...); else -> revert and show message.

Overlap Rules
-------------
Two intervals [a_start,a_end) and [b_start,b_end) overlap when a_start < b_end AND b_start < a_end.

Enforcement:
- While dragging/resizing, compute candidate interval and call db::has_overlap_for_day(...) excluding the moving tracking.
- If overlap detected: show invalid visual; on drop, revert to original and present a message explaining why.
- Validate same rule on creating/editing via modal and prevent save if it would overlap.

UI Changes (summary)
--------------------
- src/gui/views/projects.rs: add color input to Project create/edit modal and persist projects.color; show swatch in projects table.
- src/gui/views/trackings.rs: add Color swatch column to trackings list.
- src/gui/table.rs: render color swatch cells.
- src/gui/views/mod.rs and src/gui/app.rs: add ViewMode::VisualDay and sidebar entry (icons::CHART_BAR_HORIZONTAL).
- src/gui/views/onboarding.rs: set default color for initial project via generator.

Implementation Stories (small, actionable)
----------------------------------------
1) Migration + DB model (medium)
   - Add migrations/004_project_color.sql with ALTER TABLE statement.
   - Update src/db.rs: Project struct, add_project, update_project, queries.
   - Implement db::has_overlap_for_day.
   - Add a small backfill function to assign generated colors for NULL rows.
   - Tests: unit test for has_overlap_for_day.

2) Color helpers (small)
   - Add src/gui/color.rs with conversion and generator functions.
   - Unit tests for conversions and generator stability.

3) Projects UI (small)
   - Add color control (egui color picker and/or hex input) to projects create/edit modal.
   - Persist color and show swatch in projects table.

4) Trackings list swatch (small)
   - Add Color column to trackings list and use project color for swatch.

5) Visual Day basic view + wiring (medium)
   - Create src/gui/views/visual_day.rs with toolbar, timeline rendering and colored bars (read-only at first).
   - Wire view into src/gui/views/mod.rs and src/gui/app.rs.

6) Drag & resize interactions (large)
   - Implement move & resize with clamp-to-day, 1-minute rounding, min length.
   - Realtime overlap checks (db::has_overlap_for_day) and invalid-state visuals.
   - Commit on drag-end if valid; otherwise revert.

7) Duplicate tracking-edit modal into visual_day.rs (small)
   - Provide inline edit from Visual Day; refactor later to share common component.

8) Tests & QA (small)
   - Unit tests for helpers and overlap logic.
   - Manual QA checklist below.

9) Polish & PR (small)
   - Adjust UX, add changelog, create PR when authorized.

Acceptance Criteria
-------------------
- projects table has `color` column after migration.
- Projects get deterministic colors when not explicitly set.
- Visual Day shows colored trackings and honors working-hours shading.
- Drag/resize is clamped to day, rounds to 1 minute, enforces min length, and prevents overlaps.
- Invalid drag results revert and inform the user; valid drag commits automatically.

Manual QA Checklist
-------------------
1. Run migration 004_project_color.sql and backfill NULL project colors.
2. Create projects and verify colors appear in Projects UI and Trackings list.
3. Open Visual Day and verify bars are colored and working-hours shaded.
4. Drag a bar — verify rounding to minute, clamp to day, commit on valid drop.
5. Attempt to create an overlap — confirm red invalid feedback and revert on drop.

Next Step
---------
I saved this plan at IMPLEMENTATION_STORIES/25_visual_day.md. Tell me which implementation story to start with (recommended: #1 Migration + DB model). Reply with "Start story #1" or another story number, or "Hold" to pause.

23 - GUI (egui) Plan
=====================

Goal
----
Implement a GUI version of LazyTime using egui (eframe) that has exactly the same
features and behaviour as the existing TUI. The GUI is the default app mode; the
TUI remains available behind `--tui`.

High-level decisions (finalized)
--------------------------------
- UI framework: egui via eframe (native desktop app)
- Tables: egui_table (or egui_extras TableBuilder as fallback)
- Icons: egui_phosphor_icons
- Sidebar: collapsible (expanded by default); icons + text, icons-only when collapsed
- Keyboard: preserve all TUI keyboard shortcuts exactly
- Theme: Auto (follow OS) + manual override (Light/Dark) persisted in Config
- Daemon behaviour: identical to TUI (auto-start semantics and locking)
- Startup: GUI is default when running `lazytime` with no mode; `--tui` still launches TUI

User-specified icon mapping
---------------------------
- Current: clock
- Trackings: calendar
- Projects: package
- Jira: cloud-arrow-up
- Daemon: hammer

Styling constraints (exact)
--------------------------
- Buttons: horizontal padding = 8px, vertical padding = 5px
- Textfields: same padding as buttons
- Table cells: horizontal padding = 8px, vertical padding = 5px
- Table rows: selection highlights the entire row (full-row selection)
- Dialogs: modal (backdrop darkened; no interaction with background)
- Forms: margin around field blocks = 5px
- Dialog content margin = 5px

Autodetect theme
----------------
- Behavior: Auto/Light/Dark option; Auto follows OS theme where possible.
- Persistence: user choice persisted into Config (theme_preference).
- Implementation: use a small helper crate if available; otherwise per-OS fallback.

Where GUI code will live
------------------------
Create a new gui subtree under src:

src/gui/
- mod.rs                 (exports)
- app.rs                 (eframe::App implementation, top-level state)
- styles.rs              (style constants: paddings, colors, selection color)
- components.rs          (padded_button, padded_textfield, field_block, modal manager,
                         table wrapper, selection painting helper)
- sidebar.rs             (collapsible sidebar widget)
- topbar.rs              (title/header rendering)
- statusbar.rs           (bottom status bar)
- icons.rs               (icon mapping using egui_phosphor_icons)
- views/
  - current.rs
  - trackings.rs
  - projects.rs
  - jira_sync.rs
  - daemon_control.rs
  - settings.rs          (includes working-hours modal)

Integration with existing code
--------------------------------
- Reuse crate::db, crate::jira_sync, crate::daemon and other domain logic directly;
  GUI is a thin UI layer that calls into existing functions.
- Background tasks (Jira sync, daemon stdout) will run in threads and send events to
  the GUI through std::sync::mpsc channels. The GUI polls receivers each frame.

Config changes (small and backwards-compatible)
---------------------------------------------
Two small fields will be added to Config and the default template so GUI preferences
are persisted in the existing config.json used by the app.

Add:

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum ThemePreference { Auto, Light, Dark }
impl Default for ThemePreference { fn default() -> Self { ThemePreference::Auto } }

And to the Config struct:

#[serde(default)]
pub theme_preference: ThemePreference,
#[serde(default)]
pub sidebar_collapsed: bool,

And in default_config_template(...):
theme_preference: ThemePreference::Auto,
sidebar_collapsed: false,

Reasoning: serde defaults keep older config files working (no migration required).

Runtime note (tokio + eframe)
----------------------------
eframe requires running on the OS main thread in some environments (macOS). The
project currently uses #[tokio::main] on main. The minimal safe change is to run
the tokio runtime on the current thread so GUI can be started on the main thread.

Chosen approach (minimal change): change attribute to
`#[tokio::main(flavor = "current_thread")]` which keeps the main function async
but ensures the runtime is current-thread based. This is a small change and keeps
the rest of the code unchanged.

If that proves problematic on a particular platform we can switch to creating the
runtime explicitly and making main synchronous, but Option A is the minimal path.

Component details
-----------------
- padded_button(text) -> draws a Button inside a Frame with inner_margin =
  { left: 8., right: 8., top: 5., bottom: 5. }
- padded_textfield(&mut String) -> TextEdit inside same inner_margin
- field_block(ui, |ui| { ... }) -> Frame with inner_margin = 5.0 for visual grouping
- modal_manager -> draws an opaque backdrop (alpha 0.6) and centers the dialog
  with dialog inner_margin = 5. Backdrop prevents interaction with the UI below.
- table_wrapper -> wraps egui_table/egui_extras to provide:
  - full-row selection painting via ui.painter().rect_filled(row_rect, ...)
  - cell padding using Frame inner_margin (8/5)
  - resizable columns (user adjustable)
  - keyboard navigation (j/k/up/down/PageUp/PageDown) and same TUI shortcuts
- sidebar -> collapsible; expanded width ≈ 180 px, collapsed ≈ 56 px, icon size ≈ 22 px
  - when collapsed show tooltips on hover for each icon
  - smooth toggle with small animation optional

View specific notes
-------------------
All views must preserve the TUI behaviour and keyboard shortcuts.

Current
- Big duration rendering, active project line, Start (modal) and Stop.
- Start modal selects a project; identical logic to TUI.

Trackings
- Table with columns: Project, Start, End, Duration (right aligned), Description,
  Sync, By.
- Toolbar with Add/Edit/Delete/Filter/Gaps/Cleanup/Storno. Filter modal is the same
  structure as TUI filter dialog.

Projects
- Two-pane layout (projects left, rules right). Add/Edit/Delete and confirm modals.
- Regex validation on Rule creation (reuse Regex::new validation like TUI).

Jira Sync
- Log view and progress footer; Start button spawns the same jira_sync logic in a
  thread and receives events by channel.

Daemon
- Status badge: outside/running/stopped. Start/Stop buttons and logs identical to TUI.
- Auto-start behaviour: call same helper used by TUI on app startup.

Settings
- Full form with same fields as TUI plus Appearance (Auto/Light/Dark) and
  a persisted sidebar_collapsed preference.
- Working hours modal mirrors the TUI modal exactly.

Behavior parity (must-haves)
---------------------------
- Every action available in the TUI is reachable in the GUI via mouse or preserved
  keyboard shortcuts.
- Read-only behaviour for Jira-synced trackings (cannot edit/delete) preserved.
- Modal behaviour (blocking, backdrop) preserved.
- Daemon auto-start/stop semantics preserved.

Accessibility & polish
-----------------------
- Selection color: TUI-like yellow (adjustable constant in styles.rs). Default hex
  suggestion: #F2C94C.
- Modal backdrop alpha: 0.6 (semi-opaque). Content margin: 5px.
- Tables use full-row selection; on hover row highlights slightly for visual feedback.
- Provide right-click context menu on table rows: Edit / Delete / Copy.

Dependencies (to be added to Cargo.toml)
---------------------------------------
- eframe (egui)
- egui_table or egui_extras (table & column resizing)
- egui_phosphor_icons (icons)
- optionally a cross-platform small crate for system theme detection; fallback
  to platform-specific code if not available

Milestones
----------
1. Add Config fields + default template change (persist theme + sidebar state)
2. Add GUI dependencies to Cargo.toml and create src/gui/ skeleton with app + layout
3. Implement components (padded controls, modal manager, table-wrapper)
4. Current view (Start/Stop modal)
5. Trackings view (table + CRUD modals + filter + cleanup + storno)
6. Projects view (projects + rules + modals + regex validation)
7. Jira sync and Daemon views (background threads + logs + progress)
8. Settings view + working-hours modal + theme autodetect and override
9. Polishing: icons, sidebar collapse, context menus, keyboard parity pass

Estimates
---------
- Skeleton + helpers: 1–2 days
- Each major view: 1–2 days (Trackings and Projects are the largest)
- Polishing + tests: 2–4 days
- Total: approx. 1–2 weeks depending on iteration and QA

Acceptance criteria
-------------------
- GUI offers every TUI action via mouse and preserved keyboard shortcuts.
- Visual & interaction rules (padding, modal, full-row selection) implemented as specified.
- Theme Auto works and manual override persists to config.
- Daemon behaviour identical to TUI including auto-start/lock semantics.

Open questions and answers
--------------------------
- Q: Where to store GUI prefs? A: In Config (theme_preference & sidebar_collapsed).
- Q: Sidebar default state? A: Expanded by default.
- Q: Run-time change for tokio? A: Use current_thread tokio runtime via
  #[tokio::main(flavor = "current_thread")] (minimal needed change).
- Q: Are keyboard shortcuts required? A: Yes — exact parity with TUI.

Status
------
No further questions outstanding. The plan above reflects all decisions made.
The next step is implementation; the repository will be updated with a src/gui/ tree
and small, non-destructive edits to src/config.rs and src/main.rs to enable GUI as
the default. Git/branching will be handled by the repository owner as requested.

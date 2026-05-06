use chrono::{Duration, Local, NaiveDate};
use eframe::egui;
use egui_phosphor_icons::icons;

use crate::config::Config;
use crate::db;
use crate::tui::trackings_cleanup::cleanup_today_unsynced_trackings;
use crate::tui::trackings_rows::{DisplayRow, display_rows, extract_date};
use crate::tui::trackings_storno::storno_tracking;

use super::super::style;
use super::super::table::{self, RowAction};

#[derive(Default)]
pub struct TrackingsView {
    selected: usize,
    show_gaps: bool,
    filter_start: String,
    filter_end: String,
    filter_modal: bool,
    edit_modal: Option<TrackingForm>,
    confirm_delete_id: Option<i64>,
}

#[derive(Clone)]
struct TrackingForm {
    id: Option<i64>,
    project_name: String,
    start: String,
    end: String,
    description: String,
    synced: bool,
    projects: Vec<String>,
    selected_project: usize,
}

impl TrackingsView {
    pub fn ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, config: &Config) -> Option<String> {
        if self.filter_start.is_empty() || self.filter_end.is_empty() {
            let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
            self.filter_start = today.clone();
            self.filter_end = today;
            self.show_gaps = true;
        }

        let conn = db::open(config.db_path()).ok()?;
        let rows = display_rows(
            &conn,
            config,
            &self.filter_start,
            &self.filter_end,
            self.show_gaps,
        )
        .unwrap_or_default();
        if self.selected >= rows.len() {
            self.selected = rows.len().saturating_sub(1);
        }

        let mut message = None;
        self.handle_keys(ctx, &rows, &conn, config, &mut message);

        ui.horizontal(|ui| {
            if ui
                .button(style::icon_label(ui, icons::PLUS, "Add"))
                .clicked()
            {
                self.edit_modal = Some(self.new_form(&conn, None));
            }
            if ui
                .button(style::icon_label(ui, icons::PENCIL_SIMPLE, "Edit"))
                .clicked()
            {
                if let Some(form) = self.form_from_selected(&rows, &conn) {
                    self.edit_modal = Some(form);
                } else {
                    message = Some("readonly: synced tracking cannot be changed".to_string());
                }
            }
            if ui
                .button(style::icon_label(ui, icons::TRASH_SIMPLE, "Delete"))
                .clicked()
                && let Some(id) = self.selected_tracking_id(&rows)
            {
                if self.selected_tracking_synced(&rows) {
                    message = Some("readonly: synced tracking cannot be changed".to_string());
                } else {
                    self.confirm_delete_id = Some(id);
                }
            }
            if ui
                .button(style::icon_label(ui, icons::SLIDERS, "Filter"))
                .clicked()
            {
                self.filter_modal = true;
            }
            if ui
                .button(style::icon_label(ui, icons::RECTANGLE_DASHED, "Gaps"))
                .clicked()
            {
                self.show_gaps = !self.show_gaps;
                message = Some(if self.show_gaps {
                    "gaps shown".to_string()
                } else {
                    "gaps hidden".to_string()
                });
            }
            if ui
                .button(style::icon_label(ui, icons::BROOM, "Cleanup"))
                .clicked()
            {
                if let Ok(stats) = cleanup_today_unsynced_trackings(&conn, config) {
                    message = Some(if stats.removed_rows == 0 {
                        "cleanup: nothing to merge".to_string()
                    } else {
                        format!(
                            "cleanup: merged {} groups, removed {} rows",
                            stats.merged_groups, stats.removed_rows
                        )
                    });
                }
            }
            if ui
                .button(style::icon_label(ui, icons::ARROW_U_DOWN_LEFT, "Storno"))
                .clicked()
                && let Some(DisplayRow::Tracking(t)) = rows.get(self.selected)
            {
                message = Some(match storno_tracking(&conn, config, t) {
                    Ok(msg) => msg,
                    Err(err) => format!("error: {err}"),
                });
            }
        });

        let table_rows = to_table_rows(&rows, &self.filter_start, &self.filter_end);
        let dim_rows: Vec<bool> = rows
            .iter()
            .map(|row| matches!(row, DisplayRow::Gap(_)))
            .collect();
        let action = table::render_table(
            ui,
            "trackings_table",
            &["Project", "Start", "End", "Hours", "Desc", "Sync", "Source"],
            &table_rows,
            Some(self.selected),
            true,
            Some(&dim_rows),
        );
        if let Some(action) = action {
            match action {
                RowAction::Select(idx) => self.selected = idx,
                RowAction::Edit(idx) => {
                    self.selected = idx;
                    if let Some(form) = self.form_from_selected(&rows, &conn) {
                        self.edit_modal = Some(form);
                    }
                }
                RowAction::Delete(idx) => {
                    self.selected = idx;
                    if let Some(id) = self.selected_tracking_id(&rows)
                        && !self.selected_tracking_synced(&rows)
                    {
                        self.confirm_delete_id = Some(id);
                    }
                }
                RowAction::Copy(idx) => {
                    self.selected = idx;
                    if let Some(row) = table_rows.get(idx) {
                        ctx.copy_text(row.join(" | "));
                        message = Some("row copied".to_string());
                    }
                }
            }
        }

        if self.filter_modal {
            let esc_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            style::draw_modal_backdrop(ctx);
            egui::Window::new("Filter")
                .order(egui::Order::Foreground)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(style::DIALOG_MARGIN))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("Start");
                                style::padded_text_edit(ui, &mut self.filter_start);
                            });
                            ui.horizontal(|ui| {
                                ui.label("End");
                                style::padded_text_edit(ui, &mut self.filter_end);
                            });
                            ui.separator();
                            ui.horizontal(|ui| {
                                if ui.button("Today").clicked() {
                                    let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
                                    self.filter_start = today.clone();
                                    self.filter_end = today;
                                    self.filter_modal = false;
                                    self.selected = 0;
                                }
                                if ui
                                    .button(style::icon_label(ui, icons::CHECK, "OK"))
                                    .clicked()
                                {
                                    self.filter_modal = false;
                                    self.selected = 0;
                                }
                                if ui
                                    .button(style::icon_label(ui, icons::X, "Cancel"))
                                    .clicked()
                                    || esc_pressed
                                {
                                    self.filter_modal = false;
                                }
                            });
                        });
                });
        }

        if let Some(mut form) = self.edit_modal.clone() {
            let mut keep_modal_open = true;
            let esc_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            style::draw_modal_backdrop(ctx);
            egui::Window::new(if form.id.is_some() {
                "Edit tracking"
            } else {
                "Add tracking"
            })
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::same(style::DIALOG_MARGIN))
                    .show(ui, |ui| {
                        if !form.projects.is_empty() {
                            egui::ComboBox::from_label("Project")
                                .selected_text(
                                    form.projects
                                        .get(form.selected_project)
                                        .cloned()
                                        .unwrap_or_default(),
                                )
                                .show_ui(ui, |ui| {
                                    for (idx, p) in form.projects.iter().enumerate() {
                                        ui.selectable_value(&mut form.selected_project, idx, p);
                                    }
                                });
                            if let Some(name) = form.projects.get(form.selected_project) {
                                form.project_name = name.clone();
                            }
                        } else {
                            ui.horizontal(|ui| {
                                ui.label("Project");
                                style::padded_text_edit(ui, &mut form.project_name);
                            });
                        }
                        ui.horizontal(|ui| {
                            ui.label("Start");
                            style::padded_text_edit(ui, &mut form.start);
                        });
                        ui.horizontal(|ui| {
                            ui.label("End");
                            style::padded_text_edit(ui, &mut form.end);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Description");
                            style::padded_text_edit(ui, &mut form.description);
                        });
                        ui.checkbox(&mut form.synced, "Synced");
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui
                                .button(style::icon_label(ui, icons::CHECK, "OK"))
                                .clicked()
                            {
                                match save_form(&conn, &form) {
                                    Ok(msg) => {
                                        if let Some(id) = form.id {
                                            let _ = db::set_tracking_synced(
                                                &conn,
                                                id,
                                                if form.synced { 1 } else { 0 },
                                            );
                                        }
                                        message = Some(msg);
                                        keep_modal_open = false;
                                    }
                                    Err(err) => message = Some(err),
                                }
                            }
                            if ui
                                .button(style::icon_label(ui, icons::X, "Cancel"))
                                .clicked()
                                || esc_pressed
                            {
                                keep_modal_open = false;
                            }
                        });
                    });
            });
            self.edit_modal = if keep_modal_open { Some(form) } else { None };
        }

        if let Some(id) = self.confirm_delete_id {
            let esc_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            style::draw_modal_backdrop(ctx);
            egui::Window::new("Confirm delete")
                .order(egui::Order::Foreground)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(style::DIALOG_MARGIN))
                        .show(ui, |ui| {
                            ui.label("Delete selected tracking?");
                            ui.horizontal(|ui| {
                                if ui
                                    .button(style::icon_label(ui, icons::CHECK, "OK"))
                                    .clicked()
                                {
                                    if db::delete_tracking(&conn, id).is_ok() {
                                        message = Some("tracking deleted".to_string());
                                    }
                                    self.confirm_delete_id = None;
                                    self.selected = self.selected.saturating_sub(1);
                                }
                                if ui
                                    .button(style::icon_label(ui, icons::X, "Cancel"))
                                    .clicked()
                                    || esc_pressed
                                {
                                    self.confirm_delete_id = None;
                                }
                            });
                        });
                });
        }

        message
    }

    fn handle_keys(
        &mut self,
        ctx: &egui::Context,
        rows: &[DisplayRow],
        conn: &rusqlite::Connection,
        config: &Config,
        message: &mut Option<String>,
    ) {
        if self.filter_modal || self.edit_modal.is_some() || self.confirm_delete_id.is_some() {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::J))
            && !rows.is_empty()
        {
            self.selected = (self.selected + 1).min(rows.len() - 1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::K)) {
            self.selected = self.selected.saturating_sub(1);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
            shift_filter_window(&mut self.filter_start, &mut self.filter_end, false);
            self.selected = 0;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
            shift_filter_window(&mut self.filter_start, &mut self.filter_end, true);
            self.selected = 0;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F)) {
            self.filter_modal = true;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::G)) {
            self.show_gaps = !self.show_gaps;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::L))
            && let Ok(stats) = cleanup_today_unsynced_trackings(conn, config)
        {
            *message = Some(if stats.removed_rows == 0 {
                "cleanup: nothing to merge".to_string()
            } else {
                format!(
                    "cleanup: merged {} groups, removed {} rows",
                    stats.merged_groups, stats.removed_rows
                )
            });
        }
        if ctx.input(|i| i.key_pressed(egui::Key::S))
            && let Some(DisplayRow::Tracking(t)) = rows.get(self.selected)
        {
            *message = Some(match storno_tracking(conn, config, t) {
                Ok(msg) => msg,
                Err(err) => format!("error: {err}"),
            });
        }
        if ctx.input(|i| i.key_pressed(egui::Key::A)) {
            self.edit_modal = Some(self.new_form(conn, rows.get(self.selected)));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::E)) {
            if let Some(form) = self.form_from_selected(rows, conn) {
                self.edit_modal = Some(form);
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::D))
            && let Some(id) = self.selected_tracking_id(rows)
            && !self.selected_tracking_synced(rows)
        {
            self.confirm_delete_id = Some(id);
        }
    }

    fn selected_tracking_id(&self, rows: &[DisplayRow]) -> Option<i64> {
        match rows.get(self.selected) {
            Some(DisplayRow::Tracking(t)) => Some(t.id),
            _ => None,
        }
    }

    fn selected_tracking_synced(&self, rows: &[DisplayRow]) -> bool {
        match rows.get(self.selected) {
            Some(DisplayRow::Tracking(t)) => t.jira_synced != 0,
            _ => false,
        }
    }

    fn new_form(&self, conn: &rusqlite::Connection, row: Option<&DisplayRow>) -> TrackingForm {
        let projects: Vec<String> = db::projects(conn)
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.name)
            .collect();
        let mut form = TrackingForm {
            id: None,
            project_name: projects.first().cloned().unwrap_or_default(),
            start: crate::time::format_ts(&chrono::Utc::now()),
            end: String::new(),
            description: String::new(),
            synced: false,
            projects,
            selected_project: 0,
        };
        if let Some(DisplayRow::Gap(g)) = row {
            form.start = g.start_ts.clone();
            form.end = g.end_ts.clone();
            if let Some(idx) = form.projects.iter().position(|p| p == &g.after_project) {
                form.selected_project = idx;
                form.project_name = g.after_project.clone();
            }
        }
        form
    }

    fn form_from_selected(
        &self,
        rows: &[DisplayRow],
        conn: &rusqlite::Connection,
    ) -> Option<TrackingForm> {
        let DisplayRow::Tracking(t) = rows.get(self.selected)? else {
            return None;
        };
        if t.jira_synced != 0 {
            return None;
        }
        let projects: Vec<String> = db::projects(conn)
            .unwrap_or_default()
            .into_iter()
            .map(|p| p.name)
            .collect();
        let selected_project = projects
            .iter()
            .position(|p| p == &t.project_name)
            .unwrap_or(0);
        Some(TrackingForm {
            id: Some(t.id),
            project_name: t.project_name.clone(),
            start: t.start_ts.clone(),
            end: t.end_ts.clone().unwrap_or_default(),
            description: t.notes.clone().unwrap_or_default(),
            synced: t.jira_synced != 0,
            projects,
            selected_project,
        })
    }
}

fn shift_filter_window(filter_start: &mut String, filter_end: &mut String, right: bool) {
    let parse_day = |s: &str| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok();
    let today = Local::now().date_naive();
    let sday = parse_day(filter_start).unwrap_or(today);
    let eday = parse_day(filter_end).unwrap_or(sday);
    let (from, to) = if sday <= eday {
        (sday, eday)
    } else {
        (eday, sday)
    };
    let len = (to - from).num_days() + 1;
    let (new_from, new_to) = if right {
        (to + Duration::days(1), to + Duration::days(len))
    } else {
        (from + Duration::days(-len), from + Duration::days(-1))
    };
    *filter_start = new_from.format("%Y-%m-%d").to_string();
    *filter_end = new_to.format("%Y-%m-%d").to_string();
}

fn to_table_rows(rows: &[DisplayRow], filter_start: &str, filter_end: &str) -> Vec<Vec<String>> {
    let single_day_filter = match (
        NaiveDate::parse_from_str(filter_start.trim(), "%Y-%m-%d"),
        NaiveDate::parse_from_str(filter_end.trim(), "%Y-%m-%d"),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    rows.iter()
        .map(|row| match row {
            DisplayRow::Tracking(t) => {
                let start = if single_day_filter {
                    extract_time(&t.start_ts)
                } else {
                    t.start_ts.clone()
                };
                let end_raw = t.end_ts.clone().unwrap_or_else(|| "(open)".to_string());
                let end = if single_day_filter && end_raw != "(open)" {
                    extract_time(&end_raw)
                } else {
                    end_raw
                };
                let desc = t
                    .notes
                    .clone()
                    .map(|v| truncate(&v, 15))
                    .unwrap_or_default();
                vec![
                    t.project_name.clone(),
                    start,
                    end,
                    duration(&t.start_ts, t.end_ts.as_deref()),
                    desc,
                    if t.jira_synced != 0 {
                        "1".to_string()
                    } else {
                        "0".to_string()
                    },
                    t.created_by.clone(),
                ]
            }
            DisplayRow::Gap(g) => vec![
                String::new(),
                if single_day_filter {
                    extract_time(&g.start_ts)
                } else {
                    g.start_ts.clone()
                },
                if single_day_filter {
                    extract_time(&g.end_ts)
                } else {
                    g.end_ts.clone()
                },
                duration(&g.start_ts, Some(&g.end_ts)),
                String::new(),
                "0".to_string(),
                String::new(),
            ],
            DisplayRow::Separator => vec![
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ],
        })
        .collect()
}

fn duration(start: &str, end: Option<&str>) -> String {
    let Ok(start_dt) = crate::time::parse_local_ts(start) else {
        return String::new();
    };
    let end_dt = if let Some(end) = end {
        crate::time::parse_local_ts(end).ok()
    } else {
        Some(chrono::Utc::now())
    };
    let Some(end_dt) = end_dt else {
        return String::new();
    };
    let secs = end_dt.signed_duration_since(start_dt).num_seconds().max(0);
    format!("{}:{:02}", secs / 3600, (secs % 3600) / 60)
}

fn extract_time(ts: &str) -> String {
    if let Ok(dt) = crate::time::parse_ts(ts) {
        return dt.with_timezone(&Local).format("%H:%M").to_string();
    }
    extract_date(ts).unwrap_or_else(|| ts.to_string())
}

fn truncate(s: &str, limit: usize) -> String {
    if s.chars().count() > limit {
        format!("{}…", s.chars().take(limit).collect::<String>())
    } else {
        s.to_string()
    }
}

fn save_form(conn: &rusqlite::Connection, form: &TrackingForm) -> Result<String, String> {
    let project = form.project_name.trim();
    if project.is_empty() {
        return Err("project must not be empty".to_string());
    }
    let start = crate::time::parse_local_ts(form.start.trim())
        .map_err(|_| "invalid start timestamp".to_string())?;
    let end = if form.end.trim().is_empty() {
        None
    } else {
        Some(
            crate::time::parse_local_ts(form.end.trim())
                .map_err(|_| "invalid end timestamp".to_string())?,
        )
    };
    if let Some(end) = end
        && end <= start
    {
        return Err("end must be after start".to_string());
    }
    let start_s = crate::time::format_ts(&start);
    let end_s = end.map(|d| crate::time::format_ts(&d));
    let notes = if form.description.trim().is_empty() {
        None
    } else {
        Some(form.description.trim())
    };

    if let Some(id) = form.id {
        db::update_tracking_times(conn, id, project, &start_s, end_s.as_deref(), notes)
            .map_err(|e| e.to_string())?;
        Ok("tracking updated".to_string())
    } else {
        db::add_manual_tracking(conn, project, &start_s, end_s.as_deref(), notes)
            .map_err(|e| e.to_string())?;
        Ok("tracking added".to_string())
    }
}

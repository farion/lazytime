use chrono::{Duration, Local, NaiveDate, NaiveTime, Timelike};
use eframe::egui;
use egui_extras::DatePickerButton;
use egui_phosphor_icons::icons;

use crate::config::Config;
use crate::db;
use crate::tui::trackings_cleanup::cleanup_today_unsynced_trackings;
use crate::tui::trackings_rows::{DisplayRow, display_rows, extract_date};
use crate::tui::trackings_storno::storno_tracking;

use super::super::style;
use super::super::table::{self, ContextMenuConfig, ContextMenuState, RowAction};

const DIALOG_LABEL_WIDTH: f32 = 110.0;
const DATE_FIELD_WIDTH: f32 = 124.0;
const TIME_FIELD_WIDTH: f32 = 96.0;

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
    start_date: NaiveDate,
    start_time: String,
    end_enabled: bool,
    end_date: NaiveDate,
    end_time: String,
    description: String,
    synced: bool,
    projects: Vec<String>,
    selected_project: usize,
}

impl TrackingsView {
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        config: &Config,
    ) -> Option<String> {
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

        let selected_tracking = matches!(rows.get(self.selected), Some(DisplayRow::Tracking(_)));
        let selected_tracking_unsynced = selected_tracking && !self.selected_tracking_synced(&rows);

        let filter_label = if self.filter_start == self.filter_end {
            self.filter_start.clone()
        } else {
            format!("{}..{}", self.filter_start, self.filter_end)
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Trackings").size(18.0).strong());
            if ui
                .button(style::icon_label(ui, icons::ARROW_LEFT, ""))
                .on_hover_text("Previous range")
                .clicked()
            {
                shift_filter_window(&mut self.filter_start, &mut self.filter_end, false);
                self.selected = 0;
            }
            if ui
                .button(style::icon_label(ui, icons::SLIDERS, &filter_label))
                .on_hover_text("Filter range")
                .clicked()
            {
                self.filter_modal = true;
            }
            if ui
                .button(style::icon_label(ui, icons::ARROW_RIGHT, ""))
                .on_hover_text("Next range")
                .clicked()
            {
                shift_filter_window(&mut self.filter_start, &mut self.filter_end, true);
                self.selected = 0;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let gap_changed = ui
                    .checkbox(&mut self.show_gaps, "Show Gaps")
                    .on_hover_text("Show not tracked ranges during the working hours.")
                    .changed();
                if gap_changed {
                    message = Some(if self.show_gaps {
                        "gaps shown".to_string()
                    } else {
                        "gaps hidden".to_string()
                    });
                }
                if ui
                    .button(style::icon_label(ui, icons::BROOM, ""))
                    .on_hover_text(
                        "Cleanup. Merge multiple following trackings for the same project.",
                    )
                    .clicked()
                    && let Ok(stats) = cleanup_today_unsynced_trackings(&conn, config)
                {
                    message = Some(if stats.removed_rows == 0 {
                        "cleanup: nothing to merge".to_string()
                    } else {
                        format!(
                            "cleanup: merged {} groups, removed {} rows",
                            stats.merged_groups, stats.removed_rows
                        )
                    });
                }
                ui.separator();
                if ui
                    .add_enabled(
                        selected_tracking_unsynced,
                        egui::Button::new(style::icon_label(ui, icons::ARROW_U_DOWN_LEFT, "")),
                    )
                    .on_hover_text("Storno in Jira")
                    .clicked()
                    && let Some(DisplayRow::Tracking(t)) = rows.get(self.selected)
                {
                    message = Some(match storno_tracking(&conn, config, t) {
                        Ok(msg) => msg,
                        Err(err) => format!("error: {err}"),
                    });
                }
                if ui
                    .add_enabled(
                        selected_tracking_unsynced,
                        egui::Button::new(style::icon_label(ui, icons::TRASH_SIMPLE, "")),
                    )
                    .on_hover_text("Delete")
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
                    .add_enabled(
                        selected_tracking_unsynced,
                        egui::Button::new(style::icon_label(ui, icons::PENCIL_SIMPLE, "")),
                    )
                    .on_hover_text("Edit")
                    .clicked()
                {
                    if let Some(form) = self.form_from_selected(&rows, &conn) {
                        self.edit_modal = Some(form);
                    } else {
                        message = Some("readonly: synced tracking cannot be changed".to_string());
                    }
                }
                if ui
                    .button(style::icon_label(ui, icons::PLUS, ""))
                    .on_hover_text("Add")
                    .clicked()
                {
                    self.edit_modal = Some(self.new_form(&conn, rows.get(self.selected)));
                }
            });
        });

        let table_rows = to_table_rows(&rows, &self.filter_start, &self.filter_end);
        let dim_rows: Vec<bool> = rows
            .iter()
            .map(|row| matches!(row, DisplayRow::Gap(_)))
            .collect();
        let context_state: Vec<ContextMenuState> = rows
            .iter()
            .map(|row| {
                let is_tracking = matches!(row, DisplayRow::Tracking(_));
                let is_unsynced_tracking =
                    matches!(row, DisplayRow::Tracking(t) if t.jira_synced == 0);
                ContextMenuState {
                    edit_enabled: is_unsynced_tracking,
                    delete_enabled: is_unsynced_tracking,
                    copy_enabled: true,
                    storno_enabled: is_tracking,
                }
            })
            .collect();
        let action = table::render_table(
            ui,
            "trackings_table",
            &["Project", "Start", "End", "Hours", "Desc", "Sync", "Source"],
            &table_rows,
            Some(self.selected),
            Some(ContextMenuConfig {
                edit: true,
                delete: true,
                copy: false,
                storno: true,
            }),
            Some(&context_state),
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
                RowAction::Storno(idx) => {
                    self.selected = idx;
                    if let Some(DisplayRow::Tracking(t)) = rows.get(idx) {
                        message = Some(match storno_tracking(&conn, config, t) {
                            Ok(msg) => msg,
                            Err(err) => format!("error: {err}"),
                        });
                    }
                }
            }
        }

        if self.filter_modal {
            let esc_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            let today = Local::now().date_naive();
            let parse_day = |s: &str| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok();
            let mut start_date = parse_day(&self.filter_start).unwrap_or(today);
            let mut end_date = parse_day(&self.filter_end).unwrap_or(start_date);
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
                            style::setting_row(ui, "Start", "", DIALOG_LABEL_WIDTH, |ui| {
                                ui.horizontal(|ui| {
                                    ui.set_min_height(style::text_field_height(ui));
                                    ui.add_sized(
                                        [DATE_FIELD_WIDTH, style::text_field_height(ui)],
                                        DatePickerButton::new(&mut start_date)
                                            .id_salt("trackings_filter_start_date"),
                                    );
                                });
                            });

                            style::setting_row(ui, "End", "", DIALOG_LABEL_WIDTH, |ui| {
                                ui.horizontal(|ui| {
                                    ui.set_min_height(style::text_field_height(ui));
                                    ui.add_sized(
                                        [DATE_FIELD_WIDTH, style::text_field_height(ui)],
                                        DatePickerButton::new(&mut end_date)
                                            .id_salt("trackings_filter_end_date"),
                                    );
                                });
                            });

                            self.filter_start = start_date.format("%Y-%m-%d").to_string();
                            self.filter_end = end_date.format("%Y-%m-%d").to_string();

                            ui.separator();
                            ui.horizontal(|ui| {
                                if ui
                                    .button(style::icon_label(ui, icons::CHECK, "OK"))
                                    .clicked()
                                {
                                    self.filter_start = start_date.format("%Y-%m-%d").to_string();
                                    self.filter_end = end_date.format("%Y-%m-%d").to_string();
                                    self.filter_modal = false;
                                    self.selected = 0;
                                }
                                if ui
                                    .button(style::icon_label(ui, icons::CALENDAR_DOT, "Today"))
                                    .clicked()
                                {
                                    self.filter_start = today.format("%Y-%m-%d").to_string();
                                    self.filter_end = self.filter_start.clone();
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
            let esc_closes_dialog =
                esc_pressed && !egui::Popup::is_any_open(ctx) && !ctx.wants_keyboard_input();
            style::draw_modal_backdrop(ctx);
            egui::Window::new(if form.id.is_some() {
                "Edit tracking"
            } else {
                "Add tracking"
            })
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .min_width(480.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::same(style::DIALOG_MARGIN))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        if !form.projects.is_empty() {
                            style::setting_row(
                                ui,
                                "Project",
                                "Project for this tracking entry.",
                                DIALOG_LABEL_WIDTH,
                                |ui| {
                                    ui.set_min_width(ui.available_width());
                                    egui::ComboBox::from_id_salt("tracking_form_project")
                                        .width(ui.available_width())
                                        .selected_text(
                                            form.projects
                                                .get(form.selected_project)
                                                .cloned()
                                                .unwrap_or_default(),
                                        )
                                        .show_ui(ui, |ui| {
                                            for (idx, p) in form.projects.iter().enumerate() {
                                                ui.selectable_value(
                                                    &mut form.selected_project,
                                                    idx,
                                                    p,
                                                );
                                            }
                                        });
                                },
                            );
                            if let Some(name) = form.projects.get(form.selected_project) {
                                form.project_name = name.clone();
                            }
                        } else {
                            style::setting_text_row(
                                ui,
                                "Project",
                                "Project for this tracking entry.",
                                DIALOG_LABEL_WIDTH,
                                &mut form.project_name,
                            );
                        }
                        let start_error = validate_hhmm_text(&form.start_time, "start");
                        style::setting_row_with_desc_color(
                            ui,
                            "Start",
                            start_error
                                .as_deref()
                                .unwrap_or("Pick local start date and time (HH:mm)."),
                            DIALOG_LABEL_WIDTH,
                            start_error
                                .as_ref()
                                .map(|_| style::validation_palette(ui).description),
                            |ui| {
                                let field_height = style::text_field_height(ui);
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.add_sized(
                                            [DATE_FIELD_WIDTH, field_height],
                                            DatePickerButton::new(&mut form.start_date)
                                                .id_salt("tracking_start_date"),
                                        );
                                        style::padded_text_edit_sized_validated(
                                            ui,
                                            &mut form.start_time,
                                            TIME_FIELD_WIDTH,
                                            start_error.as_deref(),
                                        );
                                    },
                                );
                            },
                        );

                        let end_error = if form.end_enabled {
                            validate_hhmm_text(&form.end_time, "end")
                                .or_else(|| validate_end_after_start(&form))
                        } else {
                            None
                        };
                        style::setting_row_with_desc_color(
                            ui,
                            "End",
                            end_error
                                .as_deref()
                                .unwrap_or("Optional local end date and time (HH:mm)."),
                            DIALOG_LABEL_WIDTH,
                            end_error
                                .as_ref()
                                .map(|_| style::validation_palette(ui).description),
                            |ui| {
                                let field_height = style::text_field_height(ui);
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::Center),
                                    |ui| {
                                        ui.checkbox(&mut form.end_enabled, "");
                                        if form.end_enabled {
                                            ui.add_sized(
                                                [DATE_FIELD_WIDTH, field_height],
                                                DatePickerButton::new(&mut form.end_date)
                                                    .id_salt("tracking_end_date"),
                                            );
                                            style::padded_text_edit_sized_validated(
                                                ui,
                                                &mut form.end_time,
                                                TIME_FIELD_WIDTH,
                                                end_error.as_deref(),
                                            );
                                        }
                                    },
                                );
                            },
                        );

                        style::setting_row(
                            ui,
                            "Description",
                            "Optional notes.",
                            DIALOG_LABEL_WIDTH,
                            |ui| {
                                style::padded_text_edit_fill(ui, &mut form.description);
                            },
                        );
                        style::setting_row(
                            ui,
                            "Synced",
                            "Whether already synced to Jira.",
                            DIALOG_LABEL_WIDTH,
                            |ui| {
                                ui.checkbox(&mut form.synced, "");
                            },
                        );
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
                                || esc_closes_dialog
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
            start_date: Local::now().date_naive(),
            start_time: format_hhmm(Local::now().time()),
            end_enabled: false,
            end_date: Local::now().date_naive(),
            end_time: format_hhmm(Local::now().time()),
            description: String::new(),
            synced: false,
            projects,
            selected_project: 0,
        };
        if let Some(DisplayRow::Gap(g)) = row {
            if let Some((start_date, start_time)) = parse_local_parts(&g.start_ts) {
                form.start_date = start_date;
                form.start_time = format_hhmm(start_time);
            }
            if let Some((end_date, end_time)) = parse_local_parts(&g.end_ts) {
                form.end_enabled = true;
                form.end_date = end_date;
                form.end_time = format_hhmm(end_time);
            }
            if let Some(idx) = form.projects.iter().position(|p| p == &g.previous_project) {
                form.selected_project = idx;
                form.project_name = g.previous_project.clone();
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
        let now_local = Local::now();
        let (start_date, start_time) =
            parse_local_parts(&t.start_ts).unwrap_or((now_local.date_naive(), now_local.time()));
        let (end_enabled, end_date, end_time) = if let Some(end_ts) = t.end_ts.as_ref() {
            if let Some((date, time)) = parse_local_parts(end_ts) {
                (true, date, time)
            } else {
                (false, now_local.date_naive(), now_local.time())
            }
        } else {
            (false, now_local.date_naive(), now_local.time())
        };
        Some(TrackingForm {
            id: Some(t.id),
            project_name: t.project_name.clone(),
            start_date,
            start_time: format_hhmm(start_time),
            end_enabled,
            end_date,
            end_time: format_hhmm(end_time),
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
    let start_time = parse_hhmm_text(&form.start_time, "start")?;
    let start = compose_local_ts(form.start_date, start_time)
        .ok_or_else(|| "invalid start timestamp".to_string())?;
    let end = if form.end_enabled {
        let end_time = parse_hhmm_text(&form.end_time, "end")?;
        Some(
            compose_local_ts(form.end_date, end_time)
                .ok_or_else(|| "invalid end timestamp".to_string())?,
        )
    } else {
        None
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

fn parse_local_parts(raw: &str) -> Option<(NaiveDate, NaiveTime)> {
    let dt = crate::time::parse_local_ts(raw).ok()?.with_timezone(&Local);
    Some((dt.date_naive(), dt.time()))
}

fn format_hhmm(time: NaiveTime) -> String {
    time.format("%H:%M").to_string()
}

fn parse_hhmm_text(raw: &str, label: &str) -> Result<NaiveTime, String> {
    let (hour, minute) = crate::config::parse_hhmm(raw)
        .map_err(|_| format!("invalid {} time; expected HH:mm", label))?;
    NaiveTime::from_hms_opt(hour, minute, 0)
        .ok_or_else(|| format!("invalid {} time; expected HH:mm", label))
}

fn validate_hhmm_text(raw: &str, label: &str) -> Option<String> {
    parse_hhmm_text(raw, label).err()
}

fn validate_end_after_start(form: &TrackingForm) -> Option<String> {
    let start_time = parse_hhmm_text(&form.start_time, "start").ok()?;
    let end_time = parse_hhmm_text(&form.end_time, "end").ok()?;
    let start = compose_local_ts(form.start_date, start_time)?;
    let end = compose_local_ts(form.end_date, end_time)?;
    if end <= start {
        Some("end must be after start".to_string())
    } else {
        None
    }
}

fn compose_local_ts(date: NaiveDate, time: NaiveTime) -> Option<chrono::DateTime<chrono::Utc>> {
    let raw = format!(
        "{} {:02}:{:02}:00",
        date.format("%Y-%m-%d"),
        time.hour(),
        time.minute()
    );
    crate::time::parse_local_ts(&raw).ok()
}

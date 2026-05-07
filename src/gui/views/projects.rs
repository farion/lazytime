use eframe::egui;
use egui_phosphor_icons::icons;
use regex::Regex;

use crate::db;

use super::super::style;
use super::super::table::{self, RowAction};

const DIALOG_LABEL_WIDTH: f32 = 110.0;

#[derive(Default)]
pub struct ProjectsView {
    selected_project: usize,
    selected_rule: usize,
    project_modal: Option<ProjectForm>,
    rule_modal: Option<RuleForm>,
    confirm_modal: Option<ConfirmAction>,
    rules_modal_open: bool,
}

#[derive(Clone, Default)]
struct ProjectForm {
    id: Option<i64>,
    name: String,
    sap: String,
}

#[derive(Clone, Default)]
struct RuleForm {
    id: Option<i64>,
    project_id: i64,
    app_id: String,
    name_regex: String,
    precedence: String,
}

#[derive(Clone, Copy)]
enum ConfirmAction {
    DeleteProject(i64),
    DeleteRule(i64),
}

impl ProjectsView {
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        config: &crate::config::Config,
    ) -> Option<String> {
        let conn = db::open(config.db_path()).ok()?;
        let mut projects = db::projects(&conn).unwrap_or_default();
        if projects.is_empty() {
            self.selected_project = 0;
        } else {
            self.selected_project = self.selected_project.min(projects.len() - 1);
        }

        let mut message = None;

        self.handle_keys(ctx, &projects, &conn);

        ui.horizontal(|ui| {
            if ui
                .button(style::icon_label(ui, icons::PLUS, "Add"))
                .clicked()
            {
                self.project_modal = Some(ProjectForm::default());
            }
            if ui
                .button(style::icon_label(ui, icons::PENCIL_SIMPLE, "Edit"))
                .clicked()
            {
                if let Some(p) = projects.get(self.selected_project) {
                    self.project_modal = Some(ProjectForm {
                        id: Some(p.id),
                        name: p.name.clone(),
                        sap: p.sap_number.clone().unwrap_or_default(),
                    });
                }
            }
            if ui
                .button(style::icon_label(ui, icons::TRASH_SIMPLE, "Delete"))
                .clicked()
            {
                if let Some(p) = projects.get(self.selected_project) {
                    self.confirm_modal = Some(ConfirmAction::DeleteProject(p.id));
                }
            }
            if ui
                .button(style::icon_label(ui, icons::LIST_BULLETS, "Rules"))
                .clicked()
            {
                self.rules_modal_open = true;
                self.selected_rule = 0;
            }
        });

        let rows: Vec<Vec<String>> = projects
            .iter()
            .map(|p| vec![p.name.clone(), p.sap_number.clone().unwrap_or_default()])
            .collect();
        if let Some(action) = table::render_table(
            ui,
            "projects_table",
            &["Project", "SAP Number"],
            &rows,
            Some(self.selected_project),
            true,
            None,
        ) {
            match action {
                RowAction::Select(i) => self.selected_project = i,
                RowAction::Edit(i) => {
                    self.selected_project = i;
                    if let Some(p) = projects.get(i) {
                        self.project_modal = Some(ProjectForm {
                            id: Some(p.id),
                            name: p.name.clone(),
                            sap: p.sap_number.clone().unwrap_or_default(),
                        });
                    }
                }
                RowAction::Delete(i) => {
                    self.selected_project = i;
                    if let Some(p) = projects.get(i) {
                        self.confirm_modal = Some(ConfirmAction::DeleteProject(p.id));
                    }
                }
                RowAction::Copy(i) => {
                    self.selected_project = i;
                    if let Some(p) = projects.get(i) {
                        ctx.copy_text(format!(
                            "{} | {}",
                            p.name,
                            p.sap_number.clone().unwrap_or_default()
                        ));
                        message = Some("row copied".to_string());
                    }
                }
            }
        }

        // Rules dialog
        if self.rules_modal_open {
            let esc_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape))
                && self.rule_modal.is_none()
                && self.confirm_modal.is_none();
            style::draw_modal_backdrop(ctx);
            let title = if let Some(p) = projects.get(self.selected_project) {
                format!("Rules for {}", p.name)
            } else {
                "Rules".to_string()
            };
            let available = ctx.input(|i| i.viewport_rect());
            let max_h = available.height() * 0.85;
            egui::Window::new(title)
                .order(egui::Order::Foreground)
                .collapsible(false)
                .resizable(false)
                .min_width(500.0)
                .max_height(max_h)
                .scroll(egui::Vec2b::new(false, false))
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    self.rules_dialog_ui(ui, ctx, &conn, &projects, &mut message, max_h);
                    ui.separator();
                    if ui
                        .button(style::icon_label(ui, icons::X, "Close"))
                        .clicked()
                        || esc_pressed
                    {
                        self.rules_modal_open = false;
                    }
                });
        }

        // Project edit/add modal
        if let Some(mut modal) = self.project_modal.clone() {
            let mut keep_modal_open = true;
            let esc_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            style::draw_modal_backdrop(ctx);
            egui::Window::new(if modal.id.is_some() {
                "Edit project"
            } else {
                "Add project"
            })
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .min_width(400.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::same(style::DIALOG_MARGIN))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        style::setting_text_row(
                            ui,
                            "Name",
                            "Name used in trackings and reports.",
                            DIALOG_LABEL_WIDTH,
                            &mut modal.name,
                        );
                        style::setting_text_row(
                            ui,
                            "SAP",
                            "Optional SAP number.",
                            DIALOG_LABEL_WIDTH,
                            &mut modal.sap,
                        );
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui
                                .button(style::icon_label(ui, icons::CHECK, "OK"))
                                .clicked()
                            {
                                if modal.name.trim().is_empty() {
                                    message = Some("project name must not be empty".to_string());
                                } else {
                                    let sap = if modal.sap.trim().is_empty() {
                                        None
                                    } else {
                                        Some(modal.sap.trim())
                                    };
                                    let res = if let Some(id) = modal.id {
                                        db::update_project(&conn, id, modal.name.trim(), sap)
                                    } else {
                                        db::add_project(&conn, modal.name.trim(), sap)
                                    };
                                    if let Err(err) = res {
                                        message = Some(format!("error: {err}"));
                                    } else {
                                        message = Some(if modal.id.is_some() {
                                            "project updated".to_string()
                                        } else {
                                            "project added".to_string()
                                        });
                                        keep_modal_open = false;
                                    }
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
            self.project_modal = if keep_modal_open { Some(modal) } else { None };
        }

        // Rule edit/add modal
        if let Some(mut modal) = self.rule_modal.clone() {
            let mut keep_modal_open = true;
            let esc_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            style::draw_modal_backdrop(ctx);
            egui::Window::new(if modal.id.is_some() {
                "Edit rule"
            } else {
                "Add rule"
            })
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .min_width(460.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::same(style::DIALOG_MARGIN))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        style::setting_text_row(
                            ui,
                            "app_id",
                            "Optional app identifier.",
                            DIALOG_LABEL_WIDTH,
                            &mut modal.app_id,
                        );
                        style::setting_text_row(
                            ui,
                            "name_regex",
                            "Regex matched against title.",
                            DIALOG_LABEL_WIDTH,
                            &mut modal.name_regex,
                        );
                        style::setting_text_row(
                            ui,
                            "precedence",
                            "Higher number means higher priority.",
                            DIALOG_LABEL_WIDTH,
                            &mut modal.precedence,
                        );
                        ui.separator();
                        ui.horizontal(|ui| {
                            if ui
                                .button(style::icon_label(ui, icons::CHECK, "OK"))
                                .clicked()
                            {
                                if let Err(err) = Regex::new(modal.name_regex.trim()) {
                                    message = Some(format!("error: {err}"));
                                } else {
                                    let precedence = match modal.precedence.trim().parse::<i64>() {
                                        Ok(v) => v,
                                        Err(_) => {
                                            message =
                                                Some("precedence must be a number".to_string());
                                            return;
                                        }
                                    };
                                    let app_id = if modal.app_id.trim().is_empty() {
                                        None
                                    } else {
                                        Some(modal.app_id.trim())
                                    };
                                    let res = if let Some(id) = modal.id {
                                        db::update_rule(
                                            &conn,
                                            id,
                                            app_id,
                                            None,
                                            modal.name_regex.trim(),
                                            precedence,
                                        )
                                    } else {
                                        db::add_rule(
                                            &conn,
                                            modal.project_id,
                                            app_id,
                                            None,
                                            modal.name_regex.trim(),
                                            precedence,
                                        )
                                    };
                                    if let Err(err) = res {
                                        message = Some(format!("error: {err}"));
                                    } else {
                                        message = Some(if modal.id.is_some() {
                                            "rule updated".to_string()
                                        } else {
                                            "rule added".to_string()
                                        });
                                        keep_modal_open = false;
                                    }
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
            self.rule_modal = if keep_modal_open { Some(modal) } else { None };
        }

        // Confirm delete modal
        if let Some(action) = self.confirm_modal {
            let esc_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            style::draw_modal_backdrop(ctx);
            egui::Window::new("Confirm")
                .order(egui::Order::Foreground)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(style::DIALOG_MARGIN))
                        .show(ui, |ui| {
                            ui.label("Delete selected item?");
                            ui.horizontal(|ui| {
                                if ui
                                    .button(style::icon_label(ui, icons::CHECK, "OK"))
                                    .clicked()
                                {
                                    let res = match action {
                                        ConfirmAction::DeleteProject(id) => {
                                            db::delete_project(&conn, id)
                                        }
                                        ConfirmAction::DeleteRule(id) => db::delete_rule(&conn, id),
                                    };
                                    if let Err(err) = res {
                                        message = Some(format!("error: {err}"));
                                    } else {
                                        message = Some(match action {
                                            ConfirmAction::DeleteProject(_) => {
                                                self.selected_project =
                                                    self.selected_project.saturating_sub(1);
                                                self.selected_rule = 0;
                                                "project deleted".to_string()
                                            }
                                            ConfirmAction::DeleteRule(_) => {
                                                self.selected_rule =
                                                    self.selected_rule.saturating_sub(1);
                                                "rule deleted".to_string()
                                            }
                                        });
                                    }
                                    self.confirm_modal = None;
                                }
                                if ui
                                    .button(style::icon_label(ui, icons::X, "Cancel"))
                                    .clicked()
                                    || esc_pressed
                                {
                                    self.confirm_modal = None;
                                }
                            });
                        });
                });
        }

        projects.shrink_to_fit();
        message
    }

    fn rules_dialog_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        conn: &rusqlite::Connection,
        projects: &[db::Project],
        message: &mut Option<String>,
        max_h: f32,
    ) {
        let selected = projects.get(self.selected_project).cloned();
        let rules = selected
            .as_ref()
            .and_then(|p| db::rules_for_project(conn, p.id).ok())
            .unwrap_or_default();
        if self.selected_rule >= rules.len() {
            self.selected_rule = rules.len().saturating_sub(1);
        }

        ui.horizontal(|ui| {
            if ui
                .button(style::icon_label(ui, icons::PLUS, "Add"))
                .clicked()
            {
                if let Some(p) = selected.as_ref() {
                    self.rule_modal = Some(RuleForm {
                        id: None,
                        project_id: p.id,
                        app_id: String::new(),
                        name_regex: String::new(),
                        precedence: "0".to_string(),
                    });
                }
            }
            if ui
                .button(style::icon_label(ui, icons::PENCIL_SIMPLE, "Edit"))
                .clicked()
            {
                if let Some(p) = selected.as_ref() {
                    if let Some(r) = rules.get(self.selected_rule) {
                        self.rule_modal = Some(RuleForm {
                            id: Some(r.id),
                            project_id: p.id,
                            app_id: r.app_id.clone().unwrap_or_default(),
                            name_regex: r.name_regex.clone(),
                            precedence: r.precedence.to_string(),
                        });
                    }
                }
            }
            if ui
                .button(style::icon_label(ui, icons::TRASH_SIMPLE, "Delete"))
                .clicked()
            {
                if let Some(r) = rules.get(self.selected_rule) {
                    self.confirm_modal = Some(ConfirmAction::DeleteRule(r.id));
                }
            }
        });

        let rows: Vec<Vec<String>> = rules
            .iter()
            .map(|r| {
                vec![
                    r.app_id.clone().unwrap_or_default(),
                    r.name_regex.clone(),
                    r.precedence.to_string(),
                ]
            })
            .collect();
        // Reserve ~100px for toolbar + close button + margins
        let table_max_h = (max_h - 100.0).max(100.0);
        let action = egui::ScrollArea::vertical()
            .max_height(table_max_h)
            .show(ui, |ui| {
                table::render_table(
                    ui,
                    "rules_table",
                    &["app_id", "name_regex", "prec"],
                    &rows,
                    Some(self.selected_rule),
                    true,
                    None,
                )
            })
            .inner;
        if let Some(action) = action {
            match action {
                RowAction::Select(i) => self.selected_rule = i,
                RowAction::Edit(i) => {
                    self.selected_rule = i;
                    if let Some(sel) = selected.as_ref()
                        && let Some(r) = rules.get(i)
                    {
                        self.rule_modal = Some(RuleForm {
                            id: Some(r.id),
                            project_id: sel.id,
                            app_id: r.app_id.clone().unwrap_or_default(),
                            name_regex: r.name_regex.clone(),
                            precedence: r.precedence.to_string(),
                        });
                    }
                }
                RowAction::Delete(i) => {
                    self.selected_rule = i;
                    if let Some(r) = rules.get(i) {
                        self.confirm_modal = Some(ConfirmAction::DeleteRule(r.id));
                    }
                }
                RowAction::Copy(i) => {
                    self.selected_rule = i;
                    if let Some(r) = rules.get(i) {
                        ctx.copy_text(format!(
                            "{} | {} | {}",
                            r.app_id.clone().unwrap_or_default(),
                            r.name_regex,
                            r.precedence
                        ));
                        *message = Some("row copied".to_string());
                    }
                }
            }
        }
    }

    fn handle_keys(
        &mut self,
        ctx: &egui::Context,
        projects: &[db::Project],
        conn: &rusqlite::Connection,
    ) {
        if self.project_modal.is_some() || self.rule_modal.is_some() || self.confirm_modal.is_some()
        {
            return;
        }

        if self.rules_modal_open {
            if ctx.input(|i| i.key_pressed(egui::Key::A))
                && let Some(p) = projects.get(self.selected_project)
            {
                self.rule_modal = Some(RuleForm {
                    id: None,
                    project_id: p.id,
                    app_id: String::new(),
                    name_regex: String::new(),
                    precedence: "0".to_string(),
                });
            }
            if ctx.input(|i| i.key_pressed(egui::Key::E))
                && let Some(p) = projects.get(self.selected_project)
                && let Ok(rules) = db::rules_for_project(conn, p.id)
                && let Some(r) = rules.get(self.selected_rule)
            {
                self.rule_modal = Some(RuleForm {
                    id: Some(r.id),
                    project_id: p.id,
                    app_id: r.app_id.clone().unwrap_or_default(),
                    name_regex: r.name_regex.clone(),
                    precedence: r.precedence.to_string(),
                });
            }
            if ctx.input(|i| i.key_pressed(egui::Key::D))
                && let Some(p) = projects.get(self.selected_project)
                && let Ok(rules) = db::rules_for_project(conn, p.id)
                && let Some(r) = rules.get(self.selected_rule)
            {
                self.confirm_modal = Some(ConfirmAction::DeleteRule(r.id));
            }

            let down =
                ctx.input(|i| i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::J));
            let up =
                ctx.input(|i| i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::K));
            if let Some(p) = projects.get(self.selected_project) {
                let rules = db::rules_for_project(conn, p.id).unwrap_or_default();
                if down && !rules.is_empty() {
                    self.selected_rule = (self.selected_rule + 1).min(rules.len() - 1);
                }
                if up {
                    self.selected_rule = self.selected_rule.saturating_sub(1);
                }
            }
            return;
        }

        let down =
            ctx.input(|i| i.key_pressed(egui::Key::ArrowDown) || i.key_pressed(egui::Key::J));
        let up = ctx.input(|i| i.key_pressed(egui::Key::ArrowUp) || i.key_pressed(egui::Key::K));
        if down && !projects.is_empty() {
            self.selected_project = (self.selected_project + 1).min(projects.len() - 1);
            self.selected_rule = 0;
        }
        if up {
            self.selected_project = self.selected_project.saturating_sub(1);
            self.selected_rule = 0;
        }

        if ctx.input(|i| i.key_pressed(egui::Key::A)) {
            self.project_modal = Some(ProjectForm::default());
        }
        if ctx.input(|i| i.key_pressed(egui::Key::E))
            && let Some(p) = projects.get(self.selected_project)
        {
            self.project_modal = Some(ProjectForm {
                id: Some(p.id),
                name: p.name.clone(),
                sap: p.sap_number.clone().unwrap_or_default(),
            });
        }
        if ctx.input(|i| i.key_pressed(egui::Key::D))
            && let Some(p) = projects.get(self.selected_project)
        {
            self.confirm_modal = Some(ConfirmAction::DeleteProject(p.id));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::R)) {
            self.rules_modal_open = true;
            self.selected_rule = 0;
        }
    }
}

use std::collections::BTreeMap;
use std::fs;

use eframe::egui;
use egui_phosphor_icons::icons;
use regex::Regex;

use crate::config::{Config, TimeRange};
use crate::db;
use crate::gui::style;
use crate::secrets;

#[derive(Default)]
pub struct OnboardingView {
    step: usize,
    morning_start: String,
    morning_end: String,
    afternoon_start: String,
    afternoon_end: String,
    first_project_name: String,
    regex_lines: String,
    jira_url: String,
    jira_email: String,
    jira_token: String,
    jira_token_masked: bool,
    jira_project: String,
    jira_sap_field: String,
}

impl OnboardingView {
    pub fn new(config: &Config) -> Self {
        let mut view = Self {
            step: 0,
            morning_start: "08:00".to_string(),
            morning_end: "12:00".to_string(),
            afternoon_start: "13:00".to_string(),
            afternoon_end: "18:00".to_string(),
            first_project_name: "Default".to_string(),
            regex_lines: String::new(),
            jira_url: config.jira_url.clone().unwrap_or_default(),
            jira_email: config.jira_email.clone().unwrap_or_default(),
            jira_token: String::new(),
            jira_token_masked: true,
            jira_project: config.jira_project.clone().unwrap_or_default(),
            jira_sap_field: config.jira_sap_field.clone(),
        };

        if let Ok(conn) = db::open(config.db_path()) {
            let _ = db::migrate(&conn);
            if let Ok(projects) = db::projects(&conn) {
                let preferred = if projects.iter().any(|p| p.name == config.default_project) {
                    Some(config.default_project.as_str())
                } else if projects.iter().any(|p| p.name == "Default") {
                    Some("Default")
                } else {
                    None
                };

                if let Some(name) = preferred {
                    view.first_project_name = name.to_string();
                    if let Some(project_id) = projects.iter().find(|p| p.name == name).map(|p| p.id)
                        && let Ok(rules) = db::rules_for_project(&conn, project_id)
                    {
                        let mut lines = Vec::new();
                        for rule in rules {
                            if rule.app_id.is_none() {
                                lines.push(rule.name_regex);
                            }
                        }
                        view.regex_lines = lines.join("\n");
                    }
                }
            }
        }

        view
    }

    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        config: &mut Config,
        config_path: Option<&str>,
    ) -> Option<String> {
        let mut message = None;

        style::field_block(ui, |ui| {
            ui.heading("Welcome to LazyTime");
            ui.label(format!("Step {}/6", self.step + 1));
            ui.separator();

            match self.step {
                0 => self.step_one(ui),
                1 => self.step_two(ui),
                2 => self.step_three(ui),
                3 => self.step_four(ui),
                4 => self.step_five(ui),
                _ => self.step_six(ui),
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let can_back = self.step > 0;
                    if ui
                        .add_enabled(
                            can_back,
                            egui::Button::new(style::icon_label(ui, icons::ARROW_LEFT, "Back")),
                        )
                        .clicked()
                    {
                        self.step = self.step.saturating_sub(1);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.step == 5 {
                            if ui
                                .button(style::icon_label(ui, icons::CHECK, "Finish"))
                                .clicked()
                            {
                                message = Some(match self.finish(config, config_path) {
                                    Ok(msg) => msg,
                                    Err(err) => format!("error: {err}"),
                                });
                            }
                            return;
                        }
                        if ui
                            .button(style::icon_label(ui, icons::ARROW_RIGHT, "Next"))
                            .clicked()
                        {
                            match self.validate_current_step() {
                                Ok(()) => self.step += 1,
                                Err(err) => message = Some(format!("error: {err}")),
                            }
                        }
                        if self.step == 3
                            && ui
                                .button(style::icon_label(ui, icons::ARROW_RIGHT, "Skip Jira"))
                                .clicked()
                        {
                            self.step += 1;
                            return;
                        }
                    });
                });
                ui.separator();
            });
        });

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            message.get_or_insert("finish onboarding to continue".to_string());
        }
        message
    }

    fn step_one(&self, ui: &mut egui::Ui) {
        ui.label("LazyTime can track automatically based on window titles and your configured working hours.");
        ui.label("Manual tracking is always possible if you want to start/stop or adjust entries yourself.");
    }

    fn step_two(&mut self, ui: &mut egui::Ui) {
        ui.label("Set your regular working hours. Automatic tracking runs only in these ranges on weekdays.");
        ui.label("For advanced setup, go to Settings later.");
        ui.label(egui::RichText::new("Applies Monday-Friday only.").weak());
        style::setting_row(ui, "Morning", "", 180.0, |ui| {
            ui.horizontal(|ui| {
                style::padded_text_edit_sized_validated(ui, &mut self.morning_start, 90.0, None);
                ui.label("-");
                style::padded_text_edit_sized_validated(ui, &mut self.morning_end, 90.0, None);
            });
        });
        style::setting_row(ui, "Afternoon", "", 180.0, |ui| {
            ui.horizontal(|ui| {
                style::padded_text_edit_sized_validated(ui, &mut self.afternoon_start, 90.0, None);
                ui.label("-");
                style::padded_text_edit_sized_validated(ui, &mut self.afternoon_end, 90.0, None);
            });
        });
    }

    fn step_three(&mut self, ui: &mut egui::Ui) {
        ui.label("Add your first project. You can add more projects later.");
        ui.label("Regex rules are used to identify window titles that belong to this project.");
        ui.label(
            egui::RichText::new("Examples: .*Visual Studio Code.* | ^Jira - .* | .*Slack.*").weak(),
        );
        style::setting_text_row(ui, "Project name", "", 180.0, &mut self.first_project_name);
        style::setting_row(ui, "Regex list", "One pattern per line.", 180.0, |ui| {
            ui.add_sized(
                [ui.available_width(), 140.0],
                egui::TextEdit::multiline(&mut self.regex_lines)
                    .margin(egui::Margin::same(style::TEXT_PAD_X))
                    .hint_text("Example: .*Code.*"),
            );
        });
    }

    fn step_four(&mut self, ui: &mut egui::Ui) {
        ui.label("Jira sync is optional.");
        ui.label("If your Jira has SAP integration, LazyTime will include SAP number automatically during sync.");
        style::setting_text_row(ui, "Jira URL", "Base URL", 180.0, &mut self.jira_url);
        style::setting_text_row(
            ui,
            "Username",
            "Used for API auth and as assignee.",
            180.0,
            &mut self.jira_email,
        );
        style::setting_row(ui, "Token", "Stored securely in OS keyring.", 180.0, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.jira_token)
                    .password(self.jira_token_masked)
                    .margin(egui::Margin::symmetric(style::TEXT_PAD_X, style::TEXT_PAD_Y)),
            );
            let icon = if self.jira_token_masked {
                icons::EYE
            } else {
                icons::EYE_SLASH
            };
            if ui.button(style::icon_label(ui, icon, "")).clicked() {
                self.jira_token_masked = !self.jira_token_masked;
            }
        });
        style::setting_text_row(
            ui,
            "Project key",
            "Optional default project key",
            180.0,
            &mut self.jira_project,
        );
        style::setting_text_row(
            ui,
            "SAP field",
            "Custom field name for SAP",
            180.0,
            &mut self.jira_sap_field,
        );
    }

    fn step_five(&self, ui: &mut egui::Ui) {
        ui.label("LazyTime can show reminder dialogs when tracking needs attention.");
        ui.label("After screen lock/unlock you may also get dialogs guiding what to track next.");
    }

    fn step_six(&self, ui: &mut egui::Ui) {
        ui.heading("Ready - have fun.");
        ui.label("When you click Finish, LazyTime will save your setup and open the app normally.");
    }

    fn validate_current_step(&self) -> Result<(), String> {
        match self.step {
            1 => {
                validate_time_range(&self.morning_start, &self.morning_end)?;
                validate_time_range(&self.afternoon_start, &self.afternoon_end)?;
            }
            2 => {
                if self.first_project_name.trim().is_empty() {
                    return Err("project name must not be empty".to_string());
                }
                let rules = parse_regex_lines(&self.regex_lines)?;
                if rules.is_empty() {
                    return Err("add at least one regex".to_string());
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn finish(&self, config: &mut Config, config_path: Option<&str>) -> Result<String, String> {
        self.validate_current_step()?;

        let mut next = config.clone();
        next.onboarding_done = true;
        next.default_project = self.first_project_name.trim().to_string();
        next.working_hours = build_weekday_ranges(
            &self.morning_start,
            &self.morning_end,
            &self.afternoon_start,
            &self.afternoon_end,
        )?;
        next.jira_url = to_opt(&self.jira_url);
        next.jira_email = to_opt(&self.jira_email);
        next.jira_assignee = to_opt(&self.jira_email);
        next.jira_project = to_opt(&self.jira_project);
        if !self.jira_sap_field.trim().is_empty() {
            next.jira_sap_field = self.jira_sap_field.trim().to_string();
        }
        next.jira_token = None;
        next.validate().map_err(|e| e.to_string())?;

        if !self.jira_token.trim().is_empty() {
            secrets::store_jira_token(self.jira_token.trim()).map_err(|e| e.to_string())?;
        }

        let conn = db::open(next.db_path()).map_err(|e| e.to_string())?;
        db::migrate(&conn).map_err(|e| e.to_string())?;
        let first_name = self.first_project_name.trim();
        let projects = db::projects(&conn).map_err(|e| e.to_string())?;

        if !projects
            .iter()
            .any(|p| p.name == next.default_project.trim())
        {
            db::add_project(&conn, next.default_project.trim(), None).map_err(|e| e.to_string())?;
        }

        if !projects.iter().any(|p| p.name == first_name) {
            db::add_project(&conn, first_name, None).map_err(|e| e.to_string())?;
        }

        let projects = db::projects(&conn).map_err(|e| e.to_string())?;
        let project_id = projects
            .iter()
            .find(|p| p.name == first_name)
            .map(|p| p.id)
            .ok_or_else(|| "failed to find created project".to_string())?;

        let existing_rules = db::rules_for_project(&conn, project_id).map_err(|e| e.to_string())?;
        let mut seen = std::collections::HashSet::new();
        let mut max_precedence = -1_i64;
        for rule in &existing_rules {
            seen.insert(rule.name_regex.trim().to_string());
            if rule.precedence > max_precedence {
                max_precedence = rule.precedence;
            }
        }

        let mut next_precedence = max_precedence + 1;
        for regex in parse_regex_lines(&self.regex_lines)? {
            let key = regex.trim().to_string();
            if seen.contains(&key) {
                continue;
            }
            db::add_rule(&conn, project_id, None, None, &regex, next_precedence)
                .map_err(|e| e.to_string())?;
            seen.insert(key);
            next_precedence += 1;
        }

        let path = super::settings::resolve_config_path(config_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|_| format!("failed to create {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(&next).map_err(|e| e.to_string())?;
        fs::write(&path, json).map_err(|_| format!("failed to write {}", path.display()))?;

        *config = next;
        Ok("onboarding completed".to_string())
    }
}

fn validate_time_range(start: &str, end: &str) -> Result<(), String> {
    let (sh, sm) = crate::config::parse_hhmm(start).map_err(|e| e.to_string())?;
    let (eh, em) = crate::config::parse_hhmm(end).map_err(|e| e.to_string())?;
    let start_min = sh * 60 + sm;
    let end_min = eh * 60 + em;
    if end_min <= start_min {
        return Err("end must be greater than start".to_string());
    }
    Ok(())
}

fn build_weekday_ranges(
    morning_start: &str,
    morning_end: &str,
    afternoon_start: &str,
    afternoon_end: &str,
) -> Result<BTreeMap<u8, Vec<TimeRange>>, String> {
    validate_time_range(morning_start, morning_end)?;
    validate_time_range(afternoon_start, afternoon_end)?;
    let mut out = BTreeMap::new();
    for day in 0..5 {
        out.insert(
            day,
            vec![
                TimeRange {
                    start: morning_start.to_string(),
                    end: morning_end.to_string(),
                },
                TimeRange {
                    start: afternoon_start.to_string(),
                    end: afternoon_end.to_string(),
                },
            ],
        );
    }
    Ok(out)
}

fn parse_regex_lines(raw: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        Regex::new(trimmed).map_err(|e| e.to_string())?;
        out.push(trimmed.to_string());
    }
    Ok(out)
}

fn to_opt(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

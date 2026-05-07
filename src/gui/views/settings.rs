use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use eframe::egui;
use egui_phosphor_icons::icons;

use crate::config::{Config, ThemePreference, TimeRange};

use super::super::style;
#[path = "settings_working_hours.rs"]
mod working_hours;

const WEEKDAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const LABEL_WIDTH: f32 = 210.0;

#[derive(Clone)]
pub struct SettingsView {
    pub theme_preference: ThemePreference,
    pub sidebar_collapsed: bool,
    pref_changed: bool,
    edit: SettingsEdit,
    working_hours_modal: bool,
    working_hours_overlay: Option<(u8, usize)>,
    working_hours_overlay_pos: Option<egui::Pos2>,
    working_hours_last_edit_at: Option<Instant>,
    working_hours_last_error: Option<String>,
}

#[derive(Clone)]
struct SettingsEdit {
    default_project: String,
    tracking_stability_seconds: String,
    working_hours: BTreeMap<u8, Vec<TimeRange>>,
    track_reminder_seconds: String,
    track_reminder_snooze_seconds: String,
    summary_update_seconds: String,
    report_start: String,
    report_end: String,
    db_file: String,
    jira_url: String,
    jira_token: String,
    jira_token_masked: bool,
    jira_email: String,
    jira_project: String,
    jira_assignee: String,
    jira_issue_type: String,
    jira_sap_field: String,
    ipc_socket_path: String,
}

impl SettingsView {
    pub fn new(cfg: &Config) -> Self {
        Self {
            theme_preference: cfg.theme_preference.clone(),
            sidebar_collapsed: cfg.sidebar_collapsed,
            pref_changed: false,
            edit: SettingsEdit::from_config(cfg),
            working_hours_modal: false,
            working_hours_overlay: None,
            working_hours_overlay_pos: None,
            working_hours_last_edit_at: None,
            working_hours_last_error: None,
        }
    }

    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        config: &mut Config,
        config_path: Option<&str>,
    ) -> Option<String> {
        let mut message = None;

        if ctx.input(|i| i.key_pressed(egui::Key::R)) {
            self.edit = SettingsEdit::from_config(config);
            self.theme_preference = config.theme_preference.clone();
            self.sidebar_collapsed = config.sidebar_collapsed;
            self.pref_changed = true;
            message = Some("form reset".to_string());
        }

        ui.horizontal(|ui| {
            if ui
                .button(style::icon_label(ui, icons::FLOPPY_DISK, "Save"))
                .clicked()
                || ctx.input(|i| i.key_pressed(egui::Key::S))
            {
                match self.edit.to_config(
                    config,
                    self.theme_preference.clone(),
                    self.sidebar_collapsed,
                ) {
                    Ok(next) => {
                        let p = resolve_config_path(config_path);
                        if let Some(parent) = p.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        match serde_json::to_string_pretty(&next) {
                            Ok(json) => {
                                if fs::write(&p, json).is_ok() {
                                    *config = next;
                                    self.pref_changed = true;
                                    message = Some(format!("saved {}", p.display()));
                                } else {
                                    message =
                                        Some(format!("error: failed to write {}", p.display()));
                                }
                            }
                            Err(err) => message = Some(format!("error: {err}")),
                        }
                    }
                    Err(err) => message = Some(err),
                }
            }
            if ui
                .button(style::icon_label(ui, icons::X, "Reset"))
                .clicked()
            {
                self.edit = SettingsEdit::from_config(config);
                self.theme_preference = config.theme_preference.clone();
                self.sidebar_collapsed = config.sidebar_collapsed;
                self.pref_changed = true;
                message = Some("form reset".to_string());
            }
        });

        ui.set_min_width(ui.available_width());
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            style::field_block(ui, |ui| {
                ui.label(egui::RichText::new("Appearance").strong().size(20.0));
                ui.add_space(4.0);
                style::setting_row(
                    ui,
                    "Theme",
                    "Choose how the GUI theme is selected.",
                    LABEL_WIDTH,
                    |ui| {
                        egui::ComboBox::from_id_salt("settings_theme")
                            .selected_text(match self.theme_preference {
                                ThemePreference::Auto => "Auto",
                                ThemePreference::Light => "Light",
                                ThemePreference::Dark => "Dark",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.theme_preference,
                                    ThemePreference::Auto,
                                    "Auto",
                                );
                                ui.selectable_value(
                                    &mut self.theme_preference,
                                    ThemePreference::Light,
                                    "Light",
                                );
                                ui.selectable_value(
                                    &mut self.theme_preference,
                                    ThemePreference::Dark,
                                    "Dark",
                                );
                            });
                    },
                );
                style::setting_row(
                    ui,
                    "Start with collapsed sidebar",
                    "Enable to launch the GUI with the sidebar collapsed.",
                    LABEL_WIDTH,
                    |ui| {
                        ui.checkbox(&mut self.sidebar_collapsed, "Enabled");
                    },
                );
            });

            style::field_block(ui, |ui| {
                ui.label(egui::RichText::new("Core").strong().size(20.0));
                ui.add_space(4.0);
                style::setting_text_row(
                    ui,
                    "Default project",
                    "Project preselected when starting tracking.",
                    LABEL_WIDTH,
                    &mut self.edit.default_project,
                );
                style::setting_text_row(
                    ui,
                    "Tracking stability (seconds)",
                    "How long an app/title match must stay stable before tracking changes.",
                    LABEL_WIDTH,
                    &mut self.edit.tracking_stability_seconds,
                );
                style::setting_text_row(
                    ui,
                    "Reminder interval (seconds)",
                    "How often reminders are shown while tracking is running.",
                    LABEL_WIDTH,
                    &mut self.edit.track_reminder_seconds,
                );
                style::setting_text_row(
                    ui,
                    "Reminder snooze (seconds)",
                    "How long reminders stay snoozed after manual stop.",
                    LABEL_WIDTH,
                    &mut self.edit.track_reminder_snooze_seconds,
                );
                style::setting_text_row(
                    ui,
                    "Summary refresh (seconds)",
                    "How often summary data is refreshed.",
                    LABEL_WIDTH,
                    &mut self.edit.summary_update_seconds,
                );
                style::setting_text_row(
                    ui,
                    "Report start",
                    "Optional report start date/time filter.",
                    LABEL_WIDTH,
                    &mut self.edit.report_start,
                );
                style::setting_text_row(
                    ui,
                    "Report end",
                    "Optional report end date/time filter.",
                    LABEL_WIDTH,
                    &mut self.edit.report_end,
                );
                style::setting_text_row(
                    ui,
                    "Database file",
                    "Path to the LazyTime SQLite database file.",
                    LABEL_WIDTH,
                    &mut self.edit.db_file,
                );
                style::setting_text_row(
                    ui,
                    "IPC socket path",
                    "Socket endpoint used for daemon communication.",
                    LABEL_WIDTH,
                    &mut self.edit.ipc_socket_path,
                );
            });

            style::field_block(ui, |ui| {
                ui.label(egui::RichText::new("Jira").strong().size(20.0));
                ui.add_space(4.0);
                style::setting_text_row(
                    ui,
                    "Jira URL",
                    "Base URL of your Jira instance.",
                    LABEL_WIDTH,
                    &mut self.edit.jira_url,
                );
                style::setting_row(
                    ui,
                    "Jira API token",
                    "Token used for Jira authentication.",
                    LABEL_WIDTH,
                    |ui| {
                        if self.edit.jira_token_masked {
                            let mut masked = "*".repeat(self.edit.jira_token.chars().count());
                            style::padded_text_edit(ui, &mut masked);
                        } else {
                            style::padded_text_edit(ui, &mut self.edit.jira_token);
                        }
                        let icon = if self.edit.jira_token_masked {
                            icons::EYE
                        } else {
                            icons::EYE_SLASH
                        };
                        if ui.button(style::icon_label(ui, icon, "")).clicked() {
                            self.edit.jira_token_masked = !self.edit.jira_token_masked;
                        }
                    },
                );
                style::setting_text_row(
                    ui,
                    "Jira email",
                    "Account email used for Jira API access.",
                    LABEL_WIDTH,
                    &mut self.edit.jira_email,
                );
                style::setting_text_row(
                    ui,
                    "Jira project key",
                    "Default Jira project key for synchronization.",
                    LABEL_WIDTH,
                    &mut self.edit.jira_project,
                );
                style::setting_text_row(
                    ui,
                    "Jira assignee",
                    "Optional assignee filter (for example: me).",
                    LABEL_WIDTH,
                    &mut self.edit.jira_assignee,
                );
                style::setting_text_row(
                    ui,
                    "Jira issue type",
                    "Issue type created during sync (for example: Task).",
                    LABEL_WIDTH,
                    &mut self.edit.jira_issue_type,
                );
                style::setting_text_row(
                    ui,
                    "SAP field name",
                    "Jira field key used to store SAP references.",
                    LABEL_WIDTH,
                    &mut self.edit.jira_sap_field,
                );
            });

            style::field_block(ui, |ui| {
                ui.label(egui::RichText::new("Working Hours").strong().size(20.0));
                ui.add_space(4.0);
                style::setting_readonly_row(
                    ui,
                    "Weekly schedule",
                    "Overview of configured ranges per day.",
                    LABEL_WIDTH,
                    &format_working_hours_summary(&self.edit.working_hours),
                );
                style::setting_row(
                    ui,
                    "",
                    "",
                    LABEL_WIDTH,
                    |ui| {
                        if ui
                            .button(style::icon_label(
                                ui,
                                icons::CLOCK_COUNTDOWN,
                                "Edit Working Hours",
                            ))
                            .clicked()
                        {
                            self.working_hours_modal = true;
                        }
                    },
                );
            });
        });

        if self.working_hours_modal {
            if message.is_none() {
                message = self.render_working_hours_modal(ctx);
            } else {
                let _ = self.render_working_hours_modal(ctx);
            }
        }

        message
    }

    pub fn take_pref_changed(&mut self) -> bool {
        let v = self.pref_changed;
        self.pref_changed = false;
        v
    }
}

impl SettingsEdit {
    fn from_config(cfg: &Config) -> Self {
        Self {
            default_project: cfg.default_project.clone(),
            tracking_stability_seconds: cfg.tracking_stability_seconds.to_string(),
            working_hours: cfg.working_hours.clone(),
            track_reminder_seconds: cfg.track_reminder_seconds.to_string(),
            track_reminder_snooze_seconds: cfg.track_reminder_snooze_seconds.to_string(),
            summary_update_seconds: cfg.summary_update_seconds.to_string(),
            report_start: cfg.report_start.clone().unwrap_or_default(),
            report_end: cfg.report_end.clone().unwrap_or_default(),
            db_file: cfg.db_file.clone(),
            jira_url: cfg.jira_url.clone().unwrap_or_default(),
            jira_token: cfg.jira_token.clone().unwrap_or_default(),
            jira_token_masked: true,
            jira_email: cfg.jira_email.clone().unwrap_or_default(),
            jira_project: cfg.jira_project.clone().unwrap_or_default(),
            jira_assignee: cfg.jira_assignee.clone().unwrap_or_default(),
            jira_issue_type: cfg.jira_issue_type.clone(),
            jira_sap_field: cfg.jira_sap_field.clone(),
            ipc_socket_path: cfg.ipc_socket_path.clone().unwrap_or_default(),
        }
    }

    fn to_config(
        &self,
        cfg: &Config,
        theme_preference: ThemePreference,
        sidebar_collapsed: bool,
    ) -> Result<Config, String> {
        let next = Config {
            default_project: self.default_project.clone(),
            tracking_stability_seconds: parse_u64(
                "tracking_stability_seconds",
                &self.tracking_stability_seconds,
            )?,
            working_hours: self.working_hours.clone(),
            track_reminder_seconds: parse_u64(
                "track_reminder_seconds",
                &self.track_reminder_seconds,
            )?,
            track_reminder_snooze_seconds: parse_u64(
                "track_reminder_snooze_seconds",
                &self.track_reminder_snooze_seconds,
            )?,
            summary_update_seconds: parse_u64(
                "summary_update_seconds",
                &self.summary_update_seconds,
            )?,
            report_start: to_opt(&self.report_start),
            report_end: to_opt(&self.report_end),
            db_file: self.db_file.clone(),
            jira_url: to_opt(&self.jira_url),
            jira_token: to_opt(&self.jira_token),
            jira_email: to_opt(&self.jira_email),
            jira_project: to_opt(&self.jira_project),
            jira_assignee: to_opt(&self.jira_assignee),
            jira_issue_type: self.jira_issue_type.clone(),
            jira_sap_field: self.jira_sap_field.clone(),
            ipc_socket_path: to_opt(&self.ipc_socket_path),
            theme_preference,
            sidebar_collapsed,
        };
        next.validate().map_err(|e| e.to_string())?;
        let _ = cfg;
        Ok(next)
    }
}

fn parse_u64(name: &str, raw: &str) -> Result<u64, String> {
    raw.trim()
        .parse::<u64>()
        .map_err(|e| format!("{} invalid: {}", name, e))
}

fn to_opt(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn resolve_config_path(config_path: Option<&str>) -> PathBuf {
    if let Some(path) = config_path {
        return PathBuf::from(path);
    }
    if let Some(home) = dirs::home_dir() {
        return home.join(".config/lazytime/config.json");
    }
    PathBuf::from("./config.json")
}

fn format_working_hours_summary(map: &BTreeMap<u8, Vec<TimeRange>>) -> String {
    let mut lines = Vec::with_capacity(WEEKDAY_NAMES.len());
    for (day_idx, day_name) in WEEKDAY_NAMES.iter().enumerate() {
        let ranges = map.get(&(day_idx as u8)).cloned().unwrap_or_default();
        if ranges.is_empty() {
            lines.push(format!("{}:", day_name));
            continue;
        }

        let mut rendered_ranges = Vec::with_capacity(ranges.len());
        for range in ranges {
            rendered_ranges.push(format!("({}-{})", range.start, range.end));
        }
        lines.push(format!("{}: {}", day_name, rendered_ranges.join(" ")));
    }
    lines.join("\n")
}

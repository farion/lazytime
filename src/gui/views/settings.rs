use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use eframe::egui;
use egui_dock::{DockArea, DockState, NodeIndex, Style, SurfaceIndex, TabIndex};
use egui_phosphor_icons::icons;

use crate::config::{Config, ThemePreference, TimeRange};
use crate::secrets;

use super::super::style;
#[path = "settings_tabs.rs"]
mod settings_tabs;
#[path = "settings_working_hours.rs"]
mod working_hours;
use settings_tabs::SettingsTabViewer;

const WEEKDAY_NAMES: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const LABEL_WIDTH: f32 = 210.0;

#[derive(Clone, Debug, PartialEq, Eq)]
enum SettingsTab {
    General,
    Appearance,
    Jira,
    WorkingHours,
}

#[derive(Clone)]
pub struct SettingsView {
    pub theme_preference: ThemePreference,
    pub sidebar_collapsed: bool,
    pref_changed: bool,
    onboarding_requested: bool,
    onboarding_triggered: bool,
    edit: SettingsEdit,
    selected_tab: SettingsTab,
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
            onboarding_requested: false,
            onboarding_triggered: false,
            edit: SettingsEdit::from_config(cfg),
            selected_tab: SettingsTab::General,
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
        let has_changes = self.has_unsaved_changes(config);

        if has_changes && ctx.input(|i| i.key_pressed(egui::Key::R)) {
            self.edit = SettingsEdit::from_config(config);
            self.theme_preference = config.theme_preference.clone();
            self.sidebar_collapsed = config.sidebar_collapsed;
            self.pref_changed = true;
            message = Some("form reset".to_string());
        }

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Settings").size(18.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        has_changes,
                        egui::Button::new(style::icon_label(ui, icons::X, "")),
                    )
                    .on_hover_text("Reset")
                    .clicked()
                {
                    self.edit = SettingsEdit::from_config(config);
                    self.theme_preference = config.theme_preference.clone();
                    self.sidebar_collapsed = config.sidebar_collapsed;
                    self.pref_changed = true;
                    message = Some("form reset".to_string());
                }
                if ui
                    .add_enabled(
                        has_changes,
                        egui::Button::new(style::icon_label(ui, icons::FLOPPY_DISK, "")),
                    )
                    .on_hover_text("Save")
                    .clicked()
                    || (has_changes && ctx.input(|i| i.key_pressed(egui::Key::S)))
                {
                    match self.edit.to_config(
                        config,
                        self.theme_preference.clone(),
                        self.sidebar_collapsed,
                    ) {
                        Ok(next) => match save_config(next, &self.edit.jira_token, config_path) {
                            Ok((saved, path)) => {
                                *config = saved;
                                self.pref_changed = true;
                                message = Some(format!("saved {}", path.display()));
                            }
                            Err(err) => message = Some(format!("error: {err}")),
                        },
                        Err(err) => message = Some(err),
                    }
                }
            });
        });

        ui.set_min_width(ui.available_width());
        let mut dock_state = DockState::new(vec![
            SettingsTab::General,
            SettingsTab::Appearance,
            SettingsTab::Jira,
            SettingsTab::WorkingHours,
        ]);
        dock_state.set_active_tab((
            SurfaceIndex::main(),
            NodeIndex::root(),
            TabIndex(self.selected_tab.to_index()),
        ));

        let mut tab_viewer = SettingsTabViewer {
            view: self,
            active_tab: None,
        };
        let mut dock_style = Style::from_egui(ui.style().as_ref());
        dock_style.tab_bar.bg_fill = dock_style.tab.tab_body.bg_fill;
        dock_style.tab_bar.height = 30.0;
        dock_style.tab.minimum_width = Some(110.0);

        DockArea::new(&mut dock_state)
            .id(egui::Id::new("settings_dock_tabs"))
            .style(dock_style)
            .show_close_buttons(false)
            .show_leaf_close_all_buttons(false)
            .show_leaf_collapse_buttons(false)
            .draggable_tabs(false)
            .tab_context_menus(false)
            .show_inside(ui, &mut tab_viewer);
        if let Some(active_tab) = tab_viewer.active_tab {
            self.selected_tab = active_tab;
        }

        if self.onboarding_triggered {
            self.onboarding_triggered = false;
            message = Some(self.request_onboarding(config, config_path));
        }

        message
    }

    pub fn take_pref_changed(&mut self) -> bool {
        let v = self.pref_changed;
        self.pref_changed = false;
        v
    }

    pub fn take_onboarding_requested(&mut self) -> bool {
        let v = self.onboarding_requested;
        self.onboarding_requested = false;
        v
    }

    pub fn request_onboarding(&mut self, config: &mut Config, config_path: Option<&str>) -> String {
        let mut next = config.clone();
        next.onboarding_done = false;
        match save_config(next, &self.edit.jira_token, config_path) {
            Ok((saved, _)) => {
                *config = saved;
                self.edit = SettingsEdit::from_config(config);
                self.pref_changed = true;
                self.onboarding_requested = true;
                "onboarding started".to_string()
            }
            Err(err) => format!("error: {err}"),
        }
    }

    fn has_unsaved_changes(&self, config: &Config) -> bool {
        let Ok(next) = self.edit.to_config(
            config,
            self.theme_preference.clone(),
            self.sidebar_collapsed,
        ) else {
            return true;
        };
        let cfg_diff = match (serde_json::to_value(&next), serde_json::to_value(config)) {
            (Ok(a), Ok(b)) => a != b,
            _ => true,
        };
        if cfg_diff {
            return true;
        }
        let stored_token = secrets::load_jira_token()
            .ok()
            .flatten()
            .or_else(|| config.jira_token.clone())
            .unwrap_or_default();
        self.edit.jira_token.trim() != stored_token.trim()
    }
}

impl SettingsTab {
    fn to_index(&self) -> usize {
        match self {
            SettingsTab::General => 0,
            SettingsTab::Appearance => 1,
            SettingsTab::Jira => 2,
            SettingsTab::WorkingHours => 3,
        }
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
            jira_token: secrets::load_jira_token()
                .ok()
                .flatten()
                .or_else(|| cfg.jira_token.clone())
                .unwrap_or_default(),
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
            onboarding_done: cfg.onboarding_done,
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
            jira_token: None,
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

impl SettingsView {
    pub(crate) fn trigger_onboarding_again(&mut self) {
        self.onboarding_triggered = true;
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

fn save_config(
    next: Config,
    jira_token: &str,
    config_path: Option<&str>,
) -> Result<(Config, PathBuf), String> {
    if jira_token.trim().is_empty() {
        secrets::clear_jira_token().map_err(|e| e.to_string())?;
    } else {
        secrets::store_jira_token(jira_token).map_err(|e| e.to_string())?;
    }
    let p = resolve_config_path(config_path);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).map_err(|_| format!("failed to create {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(&next).map_err(|e| e.to_string())?;
    fs::write(&p, json).map_err(|_| format!("failed to write {}", p.display()))?;
    Ok((next, p))
}

pub(crate) fn resolve_config_path(config_path: Option<&str>) -> PathBuf {
    if let Some(path) = config_path {
        return PathBuf::from(path);
    }
    if let Some(home) = dirs::home_dir() {
        return home.join(".config/lazytime/config.json");
    }
    PathBuf::from("./config.json")
}

use eframe::egui;
use egui_dock::TabViewer;
use egui_phosphor_icons::icons;

use super::{LABEL_WIDTH, SettingsTab, SettingsView};
use crate::config::ThemePreference;
use crate::gui::style;

pub(super) struct SettingsTabViewer<'a> {
    pub(super) view: &'a mut SettingsView,
    pub(super) active_tab: Option<SettingsTab>,
}

impl TabViewer for SettingsTabViewer<'_> {
    type Tab = SettingsTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        match tab {
            SettingsTab::Appearance => "Appearance".into(),
            SettingsTab::General => "General".into(),
            SettingsTab::Jira => "Jira".into(),
            SettingsTab::WorkingHours => "Working Hours".into(),
        }
    }

    fn on_tab_button(&mut self, tab: &mut Self::Tab, response: &egui::Response) {
        if response.clicked() {
            self.active_tab = Some(tab.clone());
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            match tab {
                SettingsTab::Appearance => {
                    style::field_block(ui, |ui| {
                        style::setting_row(
                            ui,
                            "Theme",
                            "Choose how the GUI theme is selected.",
                            LABEL_WIDTH,
                            |ui| {
                                egui::ComboBox::from_id_salt("settings_theme")
                                    .selected_text(match self.view.theme_preference {
                                        ThemePreference::Auto => "Auto",
                                        ThemePreference::Light => "Light",
                                        ThemePreference::Dark => "Dark",
                                    })
                                    .show_ui(ui, |ui| {
                                        ui.selectable_value(
                                            &mut self.view.theme_preference,
                                            ThemePreference::Auto,
                                            "Auto",
                                        );
                                        ui.selectable_value(
                                            &mut self.view.theme_preference,
                                            ThemePreference::Light,
                                            "Light",
                                        );
                                        ui.selectable_value(
                                            &mut self.view.theme_preference,
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
                                ui.checkbox(&mut self.view.sidebar_collapsed, "Enabled");
                            },
                        );
                    });
                }

                SettingsTab::General => {
                    style::field_block(ui, |ui| {
                        style::setting_text_row(
                            ui,
                            "Default project",
                            "Project preselected when starting tracking.",
                            LABEL_WIDTH,
                            &mut self.view.edit.default_project,
                        );
                        style::setting_text_row(
                            ui,
                            "Tracking stability (seconds)",
                            "How long an app/title match must stay stable before tracking changes.",
                            LABEL_WIDTH,
                            &mut self.view.edit.tracking_stability_seconds,
                        );
                        style::setting_text_row(
                            ui,
                            "Reminder interval (seconds)",
                            "How often reminders are shown while tracking is running.",
                            LABEL_WIDTH,
                            &mut self.view.edit.track_reminder_seconds,
                        );
                        style::setting_text_row(
                            ui,
                            "Reminder snooze (seconds)",
                            "How long reminders stay snoozed after manual stop.",
                            LABEL_WIDTH,
                            &mut self.view.edit.track_reminder_snooze_seconds,
                        );
                        style::setting_text_row(
                            ui,
                            "Summary refresh (seconds)",
                            "How often summary data is refreshed.",
                            LABEL_WIDTH,
                            &mut self.view.edit.summary_update_seconds,
                        );
                        style::setting_text_row(
                            ui,
                            "Report start",
                            "Optional report start date/time filter.",
                            LABEL_WIDTH,
                            &mut self.view.edit.report_start,
                        );
                        style::setting_text_row(
                            ui,
                            "Report end",
                            "Optional report end date/time filter.",
                            LABEL_WIDTH,
                            &mut self.view.edit.report_end,
                        );
                        style::setting_text_row(
                            ui,
                            "Database file",
                            "Path to the LazyTime SQLite database file.",
                            LABEL_WIDTH,
                            &mut self.view.edit.db_file,
                        );
                        style::setting_text_row(
                            ui,
                            "IPC socket path",
                            "Socket endpoint used for daemon communication.",
                            LABEL_WIDTH,
                            &mut self.view.edit.ipc_socket_path,
                        );
                        style::setting_row(
                            ui,
                            "Onboarding",
                            "Show onboarding again.",
                            LABEL_WIDTH,
                            |ui| {
                                if ui.button("Show onboarding again").clicked() {
                                    self.view.trigger_onboarding_again();
                                }
                            },
                        );
                    });
                }
                SettingsTab::Jira => {
                    style::field_block(ui, |ui| {
                        style::setting_text_row(
                            ui,
                            "Jira URL",
                            "Base URL of your Jira instance.",
                            LABEL_WIDTH,
                            &mut self.view.edit.jira_url,
                        );
                        style::setting_row(
                            ui,
                            "Jira API token",
                            "Token used for Jira authentication.",
                            LABEL_WIDTH,
                            |ui| {
                                if self.view.edit.jira_token_masked {
                                    let mut masked =
                                        "*".repeat(self.view.edit.jira_token.chars().count());
                                    style::padded_text_edit(ui, &mut masked);
                                } else {
                                    style::padded_text_edit(ui, &mut self.view.edit.jira_token);
                                }
                                let icon = if self.view.edit.jira_token_masked {
                                    icons::EYE
                                } else {
                                    icons::EYE_SLASH
                                };
                                if ui.button(style::icon_label(ui, icon, "")).clicked() {
                                    self.view.edit.jira_token_masked =
                                        !self.view.edit.jira_token_masked;
                                }
                            },
                        );
                        style::setting_text_row(
                            ui,
                            "Jira email",
                            "Account email used for Jira API access.",
                            LABEL_WIDTH,
                            &mut self.view.edit.jira_email,
                        );
                        style::setting_text_row(
                            ui,
                            "Jira project key",
                            "Default Jira project key for synchronization.",
                            LABEL_WIDTH,
                            &mut self.view.edit.jira_project,
                        );
                        style::setting_text_row(
                            ui,
                            "Jira assignee",
                            "Optional assignee filter (for example: me).",
                            LABEL_WIDTH,
                            &mut self.view.edit.jira_assignee,
                        );
                        style::setting_text_row(
                            ui,
                            "Jira issue type",
                            "Issue type created during sync (for example: Task).",
                            LABEL_WIDTH,
                            &mut self.view.edit.jira_issue_type,
                        );
                        style::setting_text_row(
                            ui,
                            "SAP field name",
                            "Jira field key used to store SAP references.",
                            LABEL_WIDTH,
                            &mut self.view.edit.jira_sap_field,
                        );
                    });
                }
                SettingsTab::WorkingHours => {
                    style::field_block(ui, |ui| {
                        self.view.render_working_hours_inline(ui);
                    });
                }
            }
        });
    }
}

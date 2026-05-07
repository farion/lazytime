use anyhow::Result;
use eframe::egui;
use egui_phosphor_icons::icons;
use std::time::{Duration, Instant};

use crate::config::{Config, ThemePreference};
use crate::db;
use crate::platform;

use super::style;
use super::views;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Current,
    Trackings,
    Projects,
    Jira,
    Daemon,
    Settings,
}

pub fn run(config: &Config, config_path: Option<&str>) -> Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("LazyTime GUI")
            .with_inner_size([1280.0, 820.0]),
        ..Default::default()
    };

    let cfg = config.clone();
    let path = config_path.map(ToString::to_string);
    eframe::run_native(
        "LazyTime GUI",
        native_options,
        Box::new(move |cc| {
            let mut fonts = egui::FontDefinitions::default();
            egui_phosphor_icons::add_fonts(&mut fonts);
            let icon_family = egui::FontFamily::Name("phosphor-regular".into());
            let proportional_fallbacks = fonts
                .families
                .get(&egui::FontFamily::Proportional)
                .cloned()
                .unwrap_or_default();
            if let Some(icon_fonts) = fonts.families.get_mut(&icon_family) {
                for fallback in proportional_fallbacks {
                    if !icon_fonts.contains(&fallback) {
                        icon_fonts.push(fallback);
                    }
                }
            }
            cc.egui_ctx.set_fonts(fonts);
            style::apply_base_style(&cc.egui_ctx);
            Ok(Box::new(GuiApp::new(cfg, path)))
        }),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    Ok(())
}

struct GuiApp {
    config: Config,
    config_path: Option<String>,
    mode: ViewMode,
    backend_name: String,
    toast: Option<ToastMessage>,
    current: views::CurrentView,
    trackings: views::TrackingsView,
    projects: views::ProjectsView,
    jira: views::JiraSyncView,
    daemon: views::DaemonView,
    settings: views::SettingsView,
}

struct ToastMessage {
    text: String,
    created_at: Instant,
}

impl GuiApp {
    fn new(config: Config, config_path: Option<String>) -> Self {
        let settings = views::SettingsView::new(&config);
        let mut app = Self {
            config,
            config_path,
            mode: ViewMode::Current,
            backend_name: platform::detected_backend_name().to_string(),
            toast: None,
            current: views::CurrentView::default(),
            trackings: views::TrackingsView::default(),
            projects: views::ProjectsView::default(),
            jira: views::JiraSyncView::default(),
            daemon: views::DaemonView::default(),
            settings,
        };
        if let Some(msg) = app.daemon.auto_start_on_gui_launch(&app.config) {
            app.push_toast(msg);
        }
        app
    }

    fn push_toast(&mut self, text: String) {
        if text.trim().is_empty() {
            return;
        }
        self.toast = Some(ToastMessage {
            text,
            created_at: Instant::now(),
        });
    }

    fn draw_toast(&mut self, ctx: &egui::Context) {
        let Some(toast) = self.toast.as_ref() else {
            return;
        };
        if toast.created_at.elapsed() >= Duration::from_secs(5) {
            self.toast = None;
            return;
        }

        let mut dismiss = false;
        egui::Area::new(egui::Id::new("gui_toast"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 12.0))
            .interactable(true)
            .show(ctx, |ui| {
                let inner = egui::Frame::popup(ui.style())
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.set_max_width(420.0);
                        ui.label(&toast.text);
                    });
                let click_resp = ui.interact(
                    inner.response.rect,
                    ui.id().with("toast_click"),
                    egui::Sense::click(),
                );
                if click_resp.clicked() {
                    dismiss = true;
                }
            });
        if dismiss {
            self.toast = None;
        }
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        let pref = match self.config.theme_preference {
            ThemePreference::Auto => egui::ThemePreference::System,
            ThemePreference::Light => egui::ThemePreference::Light,
            ThemePreference::Dark => egui::ThemePreference::Dark,
        };
        ctx.set_theme(pref);
    }

    fn set_mode(&mut self, mode: ViewMode) {
        self.mode = mode;
    }

    fn handle_global_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.key_pressed(egui::Key::C)) {
            self.set_mode(ViewMode::Current);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::T)) {
            self.set_mode(ViewMode::Trackings);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::P)) {
            self.set_mode(ViewMode::Projects);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::J)) {
            self.set_mode(ViewMode::Jira);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::O)) {
            self.set_mode(ViewMode::Daemon);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::X)) {
            self.set_mode(ViewMode::Settings);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Q)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn sidebar_entry(
        ui: &mut egui::Ui,
        collapsed: bool,
        selected: bool,
        icon: egui_phosphor_icons::Icon,
        label: &str,
    ) -> egui::Response {
        let text = style::icon_label(ui, icon, if collapsed { "" } else { label });
        let button = if collapsed {
            egui::Button::new(text).selected(selected)
        } else {
            egui::Button::new(text).selected(selected).right_text("")
        };
        let response = ui.add_sized(
            [
                ui.available_width(),
                (style::BUTTON_PAD_Y as f32 * 2.0) + ui.spacing().interact_size.y,
            ],
            button,
        );
        if collapsed {
            response.on_hover_text(label)
        } else {
            response
        }
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.apply_theme(ctx);
        self.handle_global_shortcuts(ctx);
        self.daemon.poll(&self.config);

        egui::TopBottomPanel::top("gui_top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("LazyTime GUI").size(32.0).strong());
            });
        });

        egui::SidePanel::left("gui_sidebar")
            .resizable(false)
            .min_width(if self.config.sidebar_collapsed {
                style::SIDEBAR_COLLAPSED
            } else {
                style::SIDEBAR_EXPANDED
            })
            .max_width(if self.config.sidebar_collapsed {
                style::SIDEBAR_COLLAPSED
            } else {
                style::SIDEBAR_EXPANDED
            })
            .show(ctx, |ui| {
                ui.add_space(style::BUTTON_PAD_Y as f32);
                let collapse_button_width =
                    style::SIDEBAR_COLLAPSED - (style::BUTTON_PAD_X as f32 * 2.0);
                if ui
                    .add_sized(
                        [
                            collapse_button_width,
                            (style::BUTTON_PAD_Y as f32 * 2.0) + ui.spacing().interact_size.y,
                        ],
                        egui::Button::new(style::icon_label(
                            ui,
                            if self.config.sidebar_collapsed {
                                icons::CARET_RIGHT
                            } else {
                                icons::CARET_LEFT
                            },
                            "",
                        )),
                    )
                    .clicked()
                {
                    self.config.sidebar_collapsed = !self.config.sidebar_collapsed;
                }
                ui.separator();

                let collapsed = self.config.sidebar_collapsed;
                if Self::sidebar_entry(
                    ui,
                    collapsed,
                    self.mode == ViewMode::Current,
                    icons::CLOCK,
                    "Current",
                )
                .clicked()
                {
                    self.set_mode(ViewMode::Current);
                }
                if Self::sidebar_entry(
                    ui,
                    collapsed,
                    self.mode == ViewMode::Trackings,
                    icons::CALENDAR,
                    "Trackings",
                )
                .clicked()
                {
                    self.set_mode(ViewMode::Trackings);
                }
                if Self::sidebar_entry(
                    ui,
                    collapsed,
                    self.mode == ViewMode::Projects,
                    icons::PACKAGE,
                    "Projects",
                )
                .clicked()
                {
                    self.set_mode(ViewMode::Projects);
                }
                if Self::sidebar_entry(
                    ui,
                    collapsed,
                    self.mode == ViewMode::Jira,
                    icons::CLOUD_ARROW_UP,
                    "Jira",
                )
                .clicked()
                {
                    self.set_mode(ViewMode::Jira);
                }
                if Self::sidebar_entry(
                    ui,
                    collapsed,
                    self.mode == ViewMode::Daemon,
                    icons::HAMMER,
                    "Daemon",
                )
                .clicked()
                {
                    self.set_mode(ViewMode::Daemon);
                }
                if Self::sidebar_entry(
                    ui,
                    collapsed,
                    self.mode == ViewMode::Settings,
                    icons::GEAR,
                    "Settings",
                )
                .clicked()
                {
                    self.set_mode(ViewMode::Settings);
                }
            });

        egui::TopBottomPanel::bottom("gui_status").show(ctx, |ui| {
            let daemon_state = self.daemon.status_text(&self.config);
            let autotrack_status = self.current.autotrack_status_sentence(&self.config);
            let autotrack_snoozed = self.current.autotrack_is_snoozed(&self.config);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(autotrack_status).weak());
                if autotrack_snoozed
                    && ui.small_button("Unsnooze").clicked()
                    && let Some(msg) = self.current.unsnooze_autotracking(&self.config)
                {
                    self.push_toast(msg);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "backend: {} | daemon: {}",
                            self.backend_name, daemon_state
                        ))
                        .weak(),
                    );
                });
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            let msg = match self.mode {
                ViewMode::Current => self.current.ui(ctx, ui, &self.config),
                ViewMode::Trackings => self.trackings.ui(ctx, ui, &self.config),
                ViewMode::Projects => self.projects.ui(ctx, ui, &self.config),
                ViewMode::Jira => self.jira.ui(ui, &self.config),
                ViewMode::Daemon => self.daemon.ui(ui, &self.config),
                ViewMode::Settings => {
                    self.settings
                        .ui(ctx, ui, &mut self.config, self.config_path.as_deref())
                }
            };
            if let Some(msg) = msg {
                self.push_toast(msg);
            }
        });

        self.draw_toast(ctx);

        if self.settings.take_pref_changed() {
            self.config.theme_preference = self.settings.theme_preference.clone();
            self.config.sidebar_collapsed = self.settings.sidebar_collapsed;
        }

        if let Ok(conn) = db::open(self.config.db_path()) {
            let _ = db::migrate(&conn);
        }
    }
}

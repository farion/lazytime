use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver};

use chrono::Local;
use eframe::egui;
use egui_phosphor_icons::icons;

use crate::config::Config;
use crate::jira_sync::{self, JiraSyncEvent};

use super::super::style;

const MAX_LOG_LINES: usize = 2000;

#[derive(Default)]
pub struct JiraSyncView {
    logs: VecDeque<String>,
    running: bool,
    receiver: Option<Receiver<JiraSyncEvent>>,
}

impl JiraSyncView {
    pub fn ui(&mut self, ui: &mut egui::Ui, config: &Config) -> Option<String> {
        self.poll_events();

        let mut message = None;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Jira Sync").size(18.0).strong());
            let status = if self.running { "RUNNING" } else { "IDLE" };
            ui.label(status);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.running {
                    paint_rotating_notch(ui);
                    ui.ctx().request_repaint();
                }
                let btn = ui.add_enabled(
                    !self.running,
                    egui::Button::new(style::icon_label(ui, icons::ARROWS_CLOCKWISE, "")),
                );
                let btn = btn.on_hover_text("Start sync");
                if btn.clicked() {
                    self.start(config.clone());
                    message = Some("starting sync".to_string());
                }
            });
        });

        let log_bg = ui.visuals().extreme_bg_color.gamma_multiply(0.8);
        egui::Frame::new()
            .fill(log_bg)
            .inner_margin(egui::Margin::same(style::BUTTON_PAD_Y))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        for line in &self.logs {
                            ui.label(line);
                        }
                    });
            });

        message
    }

    fn start(&mut self, config: Config) {
        let (tx, rx) = mpsc::channel();
        self.receiver = Some(rx);
        self.running = true;
        self.push_log_line(format!(
            "--- Sync Start {} ---",
            Local::now().format("%Y-%m-%d %H:%M:%S")
        ));

        std::thread::spawn(move || {
            crate::jira::set_tracing_enabled(false);
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(err) => {
                    let _ = tx.send(JiraSyncEvent::Finished {
                        success: false,
                        message: format!("failed to start runtime: {}", err),
                    });
                    return;
                }
            };

            let result = rt.block_on(async {
                jira_sync::run_jira_sync(&config, false, Some(tx.clone())).await
            });
            crate::jira::set_tracing_enabled(true);
            if let Err(err) = result {
                let _ = tx.send(JiraSyncEvent::Finished {
                    success: false,
                    message: err.to_string(),
                });
            }
        });
    }

    fn poll_events(&mut self) {
        let mut finished = false;
        let mut pending = Vec::new();
        if let Some(receiver) = self.receiver.as_ref() {
            while let Ok(event) = receiver.try_recv() {
                pending.push(event);
            }
        }
        for event in pending {
            match event {
                JiraSyncEvent::Log(line) => {
                    self.push_log_line(line);
                }
                JiraSyncEvent::Progress { .. } => {}
                JiraSyncEvent::Finished {
                    success: _,
                    message,
                } => {
                    self.push_log_line(message);
                    self.push_log_line(format!(
                        "--- Sync End {} ---",
                        Local::now().format("%Y-%m-%d %H:%M:%S")
                    ));
                    self.running = false;
                    finished = true;
                }
            }
        }
        if finished {
            self.receiver = None;
        }
    }

    fn push_log_line(&mut self, line: String) {
        if self.logs.back().is_some_and(|last| last == &line) {
            return;
        }
        self.logs.push_back(line);
        while self.logs.len() > MAX_LOG_LINES {
            self.logs.pop_front();
        }
    }
}

fn paint_rotating_notch(ui: &mut egui::Ui) {
    let size = egui::vec2(16.0, 16.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let center = rect.center();
    let radius = 6.0;
    let time = ui.input(|i| i.time) as f32;
    let start = time * 4.5;
    let sweep = std::f32::consts::TAU * 0.75;
    let segments = 28;
    let mut points = Vec::with_capacity(segments + 1);
    for step in 0..=segments {
        let t = step as f32 / segments as f32;
        let a = start + (t * sweep);
        points.push(center + egui::vec2(a.cos() * radius, a.sin() * radius));
    }
    ui.painter().add(egui::Shape::line(
        points,
        egui::Stroke::new(2.0, ui.visuals().text_color()),
    ));
}

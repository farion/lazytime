use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Timelike, Utc};
use eframe::egui;
use egui_phosphor_icons::icons;

use crate::config::{Config, TimeRange, parse_hhmm};
use crate::daemon::DAEMON_RUNTIME_LOCK_KEY;
use crate::db;
use crate::tui::quotes::QuoteRotator;

use super::super::style;

const MANUAL_STOP_SNOOZE_UNTIL_KEY: &str = "autotracking_snooze_until";

pub struct CurrentView {
    start_modal_open: bool,
    projects: Vec<String>,
    selected_project: usize,
    quote_rotator: QuoteRotator,
}

impl Default for CurrentView {
    fn default() -> Self {
        Self {
            start_modal_open: false,
            projects: Vec::new(),
            selected_project: 0,
            quote_rotator: QuoteRotator::new(),
        }
    }
}

impl CurrentView {
    pub fn autotrack_status_sentence(&self, config: &Config) -> String {
        let Ok(conn) = db::open(config.db_path()) else {
            return "autotrack status unavailable".to_string();
        };
        tracking_strategy_sentence(&conn, config, Utc::now())
    }

    pub fn autotrack_is_snoozed(&self, config: &Config) -> bool {
        let Ok(conn) = db::open(config.db_path()) else {
            return false;
        };
        manual_stop_snooze_until(&conn).is_some_and(|until| Utc::now() < until)
    }

    pub fn unsnooze_autotracking(&self, config: &Config) -> Option<String> {
        let Ok(conn) = db::open(config.db_path()) else {
            return None;
        };
        if db::release_lock(&conn, MANUAL_STOP_SNOOZE_UNTIL_KEY).is_ok() {
            Some("autotracking unsnoozed".to_string())
        } else {
            None
        }
    }

    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        config: &Config,
    ) -> Option<String> {
        let conn = db::open(config.db_path()).ok()?;
        let all = db::list_today(&conn).ok()?;
        let active = db::get_active_tracking(&conn).ok().flatten();

        let now = Utc::now();
        let mut total_secs = 0i64;
        for row in all {
            let start = crate::time::parse_ts(&row.start_ts).ok();
            let end = row
                .end_ts
                .as_ref()
                .and_then(|e| crate::time::parse_ts(e).ok())
                .or_else(|| Some(now));
            if let (Some(s), Some(e)) = (start, end) {
                total_secs += e.signed_duration_since(s).num_seconds().max(0);
            }
        }

        let mut out = None;

        ui.vertical_centered(|ui| {
            let top_space = (ui.available_height() - 260.0).max(0.0) * 0.5;
            ui.add_space(top_space);
            let duration_text = format_duration(total_secs);
            let (big_lines, big_masks) = render_big_duration_lines(&duration_text);
            let bg_color = ui.visuals().window_fill;
            let font_id = egui::FontId::monospace(20.0);

            let mut job = egui::text::LayoutJob::default();
            job.halign = egui::Align::Center;
            for (row_idx, (line, mask)) in big_lines.iter().zip(big_masks.iter()).enumerate() {
                let row_fg = big_duration_row_color(ui.visuals().dark_mode, row_idx);
                if row_idx > 0 {
                    job.append("\n", 0.0, egui::TextFormat {
                        font_id: font_id.clone(),
                        color: row_fg,
                        ..Default::default()
                    });
                }
                // Build runs of same color
                let line_chars: Vec<char> = line.chars().collect();
                let mask_chars: Vec<char> = mask.chars().collect();
                let mut i = 0;
                while i < line_chars.len() {
                    let is_on = mask_chars.get(i).copied() == Some('1');
                    let color = if is_on { row_fg } else { bg_color };
                    let mut j = i + 1;
                    while j < line_chars.len() && (mask_chars.get(j).copied() == Some('1')) == is_on {
                        j += 1;
                    }
                    let segment: String = line_chars[i..j].iter().collect();
                    job.append(&segment, 0.0, egui::TextFormat {
                        font_id: font_id.clone(),
                        color,
                        ..Default::default()
                    });
                    i = j;
                }
            }
            let big_galley = ui.fonts_mut(|fonts| fonts.layout_job(job.clone()));
            if big_galley.size().x <= ui.available_width() {
                ui.label(job);
            } else {
                ui.label(
                    egui::RichText::new(duration_text)
                        .font(egui::FontId::proportional(48.0))
                        .strong(),
                );
            }
            ui.add_space(8.0);
            if let Some(a) = &active {
                let cur = crate::time::parse_ts(&a.start_ts)
                    .ok()
                    .map(|s| now.signed_duration_since(s).num_seconds().max(0))
                    .unwrap_or(0);
                ui.label(format!("{} | {}", a.project_name, format_duration(cur)));
            } else {
                ui.label("(none)");
            }
            ui.add_space(8.0);
            if active.is_some() {
                if ui
                    .button(style::icon_label(ui, icons::STOP_CIRCLE, "Stop Tracking"))
                    .clicked()
                {
                    let now = Utc::now();
                    let ts = crate::time::format_ts(&now);
                    let changed = conn
                        .execute(
                            "UPDATE trackings SET end_ts = ?1, updated_at = ?2 WHERE end_ts IS NULL",
                            rusqlite::params![ts, ts],
                        )
                        .unwrap_or(0);
                    if changed > 0 {
                        let snooze_until =
                            now + chrono::Duration::seconds(config.track_reminder_snooze_seconds as i64);
                        let _ = db::upsert_config_key(
                            &conn,
                            MANUAL_STOP_SNOOZE_UNTIL_KEY,
                            &crate::time::format_ts(&snooze_until),
                        );
                        out = Some("tracking stopped".to_string());
                    } else {
                        out = Some("no active tracking".to_string());
                    }
                }
            } else {
                let start_clicked = ui
                    .button(style::icon_label(ui, icons::PLAY_CIRCLE, "Start Tracking"))
                    .clicked();
                if start_clicked {
                    self.projects = db::projects(&conn)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|p| p.name)
                        .collect();
                    self.selected_project = 0;
                    self.start_modal_open = true;
                }
            }
            ui.add_space(16.0);
            self.quote_rotator.refresh_if_due();
            ui.label(egui::RichText::new(self.quote_rotator.current_quote()).italics());
        });

        if self.start_modal_open {
            let esc_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            style::draw_modal_backdrop(ctx);
            egui::Window::new("Start tracking")
                .order(egui::Order::Foreground)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(style::DIALOG_MARGIN))
                        .show(ui, |ui| {
                            if self.projects.is_empty() {
                                ui.label("no projects defined; add a project first");
                            } else {
                                egui::ComboBox::from_label("Project")
                                    .selected_text(
                                        self.projects
                                            .get(self.selected_project)
                                            .cloned()
                                            .unwrap_or_default(),
                                    )
                                    .show_ui(ui, |ui| {
                                        for (idx, p) in self.projects.iter().enumerate() {
                                            ui.selectable_value(&mut self.selected_project, idx, p);
                                        }
                                    });
                            }
                            ui.separator();
                            ui.horizontal(|ui| {
                                if ui
                                    .button(style::icon_label(ui, icons::CHECK, "OK"))
                                    .clicked()
                                {
                                    if let Some(project) = self.projects.get(self.selected_project) {
                                        let now = Utc::now();
                                        let ts = crate::time::format_ts(&now);
                                        let _ = conn.execute(
                                            "UPDATE trackings SET end_ts = ?1, updated_at = ?2 WHERE end_ts IS NULL",
                                            rusqlite::params![ts, ts],
                                        );
                                        let _ = conn.execute(
                                            "INSERT INTO trackings (project_name, start_ts, created_by, notes) VALUES (?1, ?2, 'tui', ?3)",
                                            rusqlite::params![project, ts, Option::<&str>::None],
                                        );
                                        out = Some(format!("tracking started: {}", project));
                                    }
                                    self.start_modal_open = false;
                                }
                                if ui
                                    .button(style::icon_label(ui, icons::X, "Cancel"))
                                    .clicked()
                                    || esc_pressed
                                {
                                    self.start_modal_open = false;
                                }
                            });
                        });
                });
        }

        out
    }
}

fn format_duration(secs: i64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    format!("{}:{:02}", h, m)
}

/// Returns (text, on_mask) where both are [String; 5].
/// All characters are '█'. on_mask has '1' for lit pixels and '0' for background.
fn render_big_duration_lines(value: &str) -> ([String; 5], [String; 5]) {
    let mut lines: [String; 5] = Default::default();
    let mut masks: [String; 5] = Default::default();
    for ch in value.chars() {
        let glyph = glyph_mask(ch);
        for row in 0..5 {
            if !lines[row].is_empty() {
                lines[row].push('█');
                masks[row].push('0');
            }
            for px in glyph[row].chars() {
                let on = px != ' ';
                lines[row].push_str("██");
                masks[row].push(if on { '1' } else { '0' });
                masks[row].push(if on { '1' } else { '0' });
            }
        }
    }
    (
        [
            lines[0].clone(),
            lines[1].clone(),
            lines[2].clone(),
            lines[3].clone(),
            lines[4].clone(),
        ],
        [
            masks[0].clone(),
            masks[1].clone(),
            masks[2].clone(),
            masks[3].clone(),
            masks[4].clone(),
        ],
    )
}

fn big_duration_row_color(dark_mode: bool, row_idx: usize) -> egui::Color32 {
    let shades_dark = [120, 145, 170, 200, 225];
    let shades_light = [110, 90, 75, 60, 45];
    let gray = if dark_mode {
        shades_dark[row_idx.min(4)]
    } else {
        shades_light[row_idx.min(4)]
    };
    egui::Color32::from_gray(gray)
}

fn glyph_mask(ch: char) -> [&'static str; 5] {
    match ch {
        '0' => ["#####", "#   #", "#   #", "#   #", "#####"],
        '1' => ["  ## ", "   # ", "   # ", "   # ", "  ###"],
        '2' => ["#####", "    #", "#####", "#    ", "#####"],
        '3' => ["#####", "    #", " ####", "    #", "#####"],
        '4' => ["#   #", "#   #", "#####", "    #", "    #"],
        '5' => ["#####", "#    ", "#####", "    #", "#####"],
        '6' => ["#####", "#    ", "#####", "#   #", "#####"],
        '7' => ["#####", "    #", "    #", "    #", "    #"],
        '8' => ["#####", "#   #", "#####", "#   #", "#####"],
        '9' => ["#####", "#   #", "#####", "    #", "#####"],
        ':' => ["     ", "  #  ", "     ", "  #  ", "     "],
        _ => ["     ", "     ", "     ", "     ", "     "],
    }
}

fn tracking_strategy_sentence(
    conn: &rusqlite::Connection,
    config: &Config,
    now: DateTime<Utc>,
) -> String {
    let daemon_running = db::get_config_key(conn, DAEMON_RUNTIME_LOCK_KEY)
        .ok()
        .flatten()
        .is_some();
    if !daemon_running {
        return "autotracking unavailable".to_string();
    }

    if let Some(until) = manual_stop_snooze_until(conn)
        && now < until
    {
        return format!(
            "autotracking snoozed until {}",
            human_datetime(until.with_timezone(&Local))
        );
    }

    let (inside_hours, next_start) = working_hours_state(config, now.with_timezone(&Local));
    if !inside_hours {
        return if let Some(next) = next_start {
            format!(
                "autotracking paused (outside hours) until {}",
                human_datetime(next)
            )
        } else {
            "autotracking paused (outside hours)".to_string()
        };
    }

    "autotracking active".to_string()
}

fn human_datetime(dt: DateTime<Local>) -> String {
    dt.format("%Y-%m-%d %H:%M").to_string()
}

fn manual_stop_snooze_until(conn: &rusqlite::Connection) -> Option<DateTime<Utc>> {
    db::get_config_key(conn, MANUAL_STOP_SNOOZE_UNTIL_KEY)
        .ok()
        .flatten()
        .and_then(|raw| crate::time::parse_ts(&raw).ok())
}

fn working_hours_state(config: &Config, now: DateTime<Local>) -> (bool, Option<DateTime<Local>>) {
    let now_weekday = now.weekday().num_days_from_monday() as u8;
    let now_minutes = now.hour() * 60 + now.minute();

    if let Some(ranges) = config.working_hours.get(&now_weekday)
        && is_inside_any_range(ranges, now_minutes)
    {
        return (true, None);
    }

    for day_offset in 0..7 {
        let weekday = ((now_weekday as i64 + day_offset) % 7) as u8;
        let Some(ranges) = config.working_hours.get(&weekday) else {
            continue;
        };
        let Some((start_h, start_m)) = earliest_valid_start(ranges, day_offset == 0, now_minutes)
        else {
            continue;
        };
        let date = now.date_naive() + Duration::days(day_offset);
        if let Some(next) = Local
            .with_ymd_and_hms(date.year(), date.month(), date.day(), start_h, start_m, 0)
            .single()
        {
            return (false, Some(next));
        }
    }

    (false, None)
}

fn is_inside_any_range(ranges: &[TimeRange], now_minutes: u32) -> bool {
    for range in ranges {
        let Ok((start_h, start_m)) = parse_hhmm(&range.start) else {
            continue;
        };
        let Ok((end_h, end_m)) = parse_hhmm(&range.end) else {
            continue;
        };
        let start = start_h * 60 + start_m;
        let end = end_h * 60 + end_m;
        if now_minutes >= start && now_minutes <= end {
            return true;
        }
    }
    false
}

fn earliest_valid_start(
    ranges: &[TimeRange],
    only_after_now: bool,
    now_minutes: u32,
) -> Option<(u32, u32)> {
    let mut best: Option<(u32, u32)> = None;
    for range in ranges {
        let Ok((start_h, start_m)) = parse_hhmm(&range.start) else {
            continue;
        };
        let start_minutes = start_h * 60 + start_m;
        if only_after_now && start_minutes <= now_minutes {
            continue;
        }
        best = match best {
            Some((best_h, best_m)) => {
                let best_minutes = best_h * 60 + best_m;
                if start_minutes < best_minutes {
                    Some((start_h, start_m))
                } else {
                    Some((best_h, best_m))
                }
            }
            None => Some((start_h, start_m)),
        };
    }
    best
}

use std::time::Duration;

use chrono::{NaiveTime, Timelike};
use eframe::egui;
use egui_phosphor_icons::icons;
use egui_timepicker::TimePickerButton;

use crate::config::TimeRange;

use super::{SettingsView, WEEKDAY_NAMES};
use crate::gui::style;

impl SettingsView {
    pub(super) fn render_working_hours_modal(&mut self, ctx: &egui::Context) -> Option<String> {
        let mut toast = None;
        let mut opened_overlay_this_frame = false;
        let max_ranges_in_day = self
            .edit
            .working_hours
            .values()
            .map(std::vec::Vec::len)
            .max()
            .unwrap_or(0)
            .min(4) as f32;
        let dialog_width = (max_ranges_in_day * 110.0) + 210.0;
        let esc_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        style::draw_modal_backdrop(ctx);
        egui::Window::new("Working hours")
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .default_width(dialog_width)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::same(style::DIALOG_MARGIN))
                    .show(ui, |ui| {
                        ui.set_min_width(dialog_width - 20.0);
                        ui.label("Click a range to edit. Changes are only saved with the global Save button.");
                        ui.separator();

                        egui::Grid::new("working_hours_grid")
                            .num_columns(2)
                            .spacing([12.0, 8.0])
                            .show(ui, |ui| {
                                let button_height =
                                    ui.spacing().interact_size.y + (style::BUTTON_PAD_Y as f32 * 2.0);
                                for (day_idx, day_name) in WEEKDAY_NAMES.iter().enumerate() {
                                    let day = day_idx as u8;
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(40.0, button_height),
                                        egui::Layout::left_to_right(egui::Align::Center),
                                        |ui| {
                                            ui.label(*day_name);
                                        },
                                    );
                                    ui.horizontal(|ui| {
                                        let ranges = self
                                            .edit
                                            .working_hours
                                            .get(&day)
                                            .cloned()
                                            .unwrap_or_default();

                                        for (range_idx, range) in ranges.iter().enumerate() {
                                            let response = ui.add_sized(
                                                [0.0, button_height],
                                                egui::Button::new(format!(
                                                    "{}-{}",
                                                    range.start, range.end
                                                )),
                                            );
                                            if response.clicked() {
                                                egui::Popup::close_all(ctx);
                                                self.working_hours_overlay = Some((day, range_idx));
                                                self.working_hours_overlay_pos =
                                                    Some(response.rect.left_bottom() + egui::vec2(0.0, 4.0));
                                                self.working_hours_last_edit_at = None;
                                                opened_overlay_this_frame = true;
                                            }
                                        }

                                        if ranges.len() < 4 {
                                            let plus_button = ui
                                                .add_sized(
                                                    [button_height, button_height],
                                                    egui::Button::new(style::icon_label(ui, icons::PLUS, "")),
                                                )
                                                .on_hover_text("Add range");
                                            if plus_button.clicked() {
                                                match self.add_working_hours_range(day) {
                                                    Ok(_) => {
                                                        self.working_hours_last_error = None;
                                                    }
                                                    Err(err) => {
                                                        toast = Some(err);
                                                    }
                                                }
                                            }
                                        }
                                    });
                                    ui.end_row();
                                }
                            });

                        ui.separator();

                        ui.horizontal(|ui| {
                            if ui
                                .button(style::icon_label(ui, icons::X, "Close"))
                                .clicked()
                                || esc_pressed
                            {
                                egui::Popup::close_all(ctx);
                                self.working_hours_modal = false;
                                self.working_hours_overlay = None;
                                self.working_hours_overlay_pos = None;
                                self.working_hours_last_edit_at = None;
                                self.working_hours_last_error = None;
                            }
                        });
                    });
            });

        self.render_working_hours_overlay(ctx, opened_overlay_this_frame);

        if let Some(last_edit) = self.working_hours_last_edit_at
            && last_edit.elapsed() >= Duration::from_millis(300)
        {
            self.working_hours_last_edit_at = None;
            if let Some((day, _)) = self.working_hours_overlay {
                match validate_day_ranges(self, day) {
                    Ok(()) => {
                        self.working_hours_last_error = None;
                    }
                    Err(err) => {
                        if self.working_hours_last_error.as_deref() != Some(err.as_str()) {
                            self.working_hours_last_error = Some(err.clone());
                            if toast.is_none() {
                                toast = Some(err);
                            }
                        }
                    }
                }
            }
        }

        toast
    }

    fn add_working_hours_range(&mut self, day: u8) -> Result<usize, String> {
        let ranges = self.edit.working_hours.entry(day).or_default();
        if ranges.len() >= 4 {
            return Err("a workday can have at most 4 ranges".to_string());
        }
        let new_range = if let Some(last) = ranges.last() {
            let last_end = hhmm_to_minutes(&last.end)?;
            let start = ((last_end / 60) + 1) * 60;
            let end = start + 60;
            if end > 1439 {
                return Err("cannot add range beyond 23:59".to_string());
            }
            TimeRange {
                start: minutes_to_hhmm(start),
                end: minutes_to_hhmm(end),
            }
        } else {
            TimeRange {
                start: "09:00".to_string(),
                end: "10:00".to_string(),
            }
        };
        ranges.push(new_range);
        Ok(ranges.len() - 1)
    }

    fn render_working_hours_overlay(&mut self, ctx: &egui::Context, opened_this_frame: bool) {
        let Some((day, range_idx)) = self.working_hours_overlay else {
            return;
        };

        let Some(ranges) = self.edit.working_hours.get_mut(&day) else {
            self.working_hours_overlay = None;
            self.working_hours_overlay_pos = None;
            self.working_hours_last_edit_at = None;
            return;
        };
        if range_idx >= ranges.len() {
            self.working_hours_overlay = None;
            self.working_hours_overlay_pos = None;
            self.working_hours_last_edit_at = None;
            return;
        }

        let mut should_delete = false;
        let mut should_close = false;
        let mut changed = false;
        let mut start_picker_hit_rect: Option<egui::Rect> = None;
        let mut end_picker_hit_rect: Option<egui::Rect> = None;
        let pos = self
            .working_hours_overlay_pos
            .unwrap_or_else(|| egui::pos2(320.0, 220.0));

        let overlay_area = egui::Area::new(egui::Id::new("working_hours_range_overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(180.0);
                    ui.set_max_width(180.0);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label("Start");
                            let mut start_time = parse_hhmm_time(&ranges[range_idx].start)
                                .unwrap_or_else(|| {
                                    NaiveTime::from_hms_opt(9, 0, 0).expect("valid default")
                                });
                            let start_id = format!("working_hours_start_time_{}_{}", day, range_idx);
                            let response = ui.add(
                                TimePickerButton::new(&mut start_time)
                                    .id_salt(start_id.as_str())
                                    .show_icon(false)
                                    .show_seconds(false),
                            );
                            start_picker_hit_rect = Some(timepicker_popup_hit_rect(ui, response.rect));
                            if response.changed() {
                                ranges[range_idx].start = time_to_hhmm(start_time);
                                changed = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label("End");
                            let mut end_time = parse_hhmm_time(&ranges[range_idx].end)
                                .unwrap_or_else(|| {
                                    NaiveTime::from_hms_opt(10, 0, 0).expect("valid default")
                                });
                            let end_id = format!("working_hours_end_time_{}_{}", day, range_idx);
                            let response = ui.add(
                                TimePickerButton::new(&mut end_time)
                                    .id_salt(end_id.as_str())
                                    .show_icon(false)
                                    .show_seconds(false),
                            );
                            end_picker_hit_rect = Some(timepicker_popup_hit_rect(ui, response.rect));
                            if response.changed() {
                                ranges[range_idx].end = time_to_hhmm(end_time);
                                changed = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui
                                .button(style::icon_label(ui, icons::TRASH, "Delete"))
                                .clicked()
                            {
                                should_delete = true;
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("OK").clicked() {
                                        should_close = true;
                                    }
                                },
                            );
                        });
                    });
                });
            });

        if !opened_this_frame {
            let (pressed, click_pos) =
                ctx.input(|i| (i.pointer.any_pressed(), i.pointer.interact_pos()));
            if pressed
                && let Some(click_pos) = click_pos
                && !overlay_area.response.rect.contains(click_pos)
                && !start_picker_hit_rect.is_some_and(|rect| rect.contains(click_pos))
                && !end_picker_hit_rect.is_some_and(|rect| rect.contains(click_pos))
            {
                egui::Popup::close_all(ctx);
                self.working_hours_overlay = None;
                self.working_hours_overlay_pos = None;
                self.working_hours_last_edit_at = None;
                self.working_hours_last_error = None;
                return;
            }
        }

        if changed {
            self.working_hours_last_edit_at = Some(std::time::Instant::now());
            self.working_hours_last_error = None;
        }

        if should_delete {
            if let Some(ranges) = self.edit.working_hours.get_mut(&day)
                && range_idx < ranges.len()
            {
                ranges.remove(range_idx);
            }
            egui::Popup::close_all(ctx);
            self.working_hours_overlay = None;
            self.working_hours_overlay_pos = None;
            self.working_hours_last_edit_at = None;
            self.working_hours_last_error = None;
            return;
        }

        if should_close {
            egui::Popup::close_all(ctx);
            self.working_hours_overlay = None;
            self.working_hours_overlay_pos = None;
            self.working_hours_last_edit_at = None;
        }
    }
}

fn timepicker_popup_hit_rect(ui: &egui::Ui, button_rect: egui::Rect) -> egui::Rect {
    let popup_width = 250.0;
    let popup_height = 340.0;
    let width_with_padding = popup_width
        + ui.style().spacing.item_spacing.x
        + ui.style().spacing.window_margin.leftf()
        + ui.style().spacing.window_margin.rightf();

    let mut x = button_rect.left();
    if x + width_with_padding > ui.clip_rect().right() {
        x = button_rect.right() - width_with_padding;
    }
    x = x.max(ui.style().spacing.window_margin.leftf());

    egui::Rect::from_min_size(
        egui::pos2(x, button_rect.left_bottom().y),
        egui::vec2(width_with_padding, popup_height),
    )
}

fn hhmm_to_minutes(raw: &str) -> Result<i32, String> {
    let (h, m) = crate::config::parse_hhmm(raw).map_err(|err| err.to_string())?;
    Ok((h as i32 * 60) + m as i32)
}

fn parse_hhmm_time(raw: &str) -> Option<NaiveTime> {
    let (hour, minute) = crate::config::parse_hhmm(raw).ok()?;
    NaiveTime::from_hms_opt(hour, minute, 0)
}

fn time_to_hhmm(time: NaiveTime) -> String {
    format!("{:02}:{:02}", time.hour(), time.minute())
}

fn minutes_to_hhmm(total_minutes: i32) -> String {
    let hour = total_minutes / 60;
    let minute = total_minutes % 60;
    format!("{:02}:{:02}", hour, minute)
}

fn validate_day_ranges(settings: &SettingsView, day: u8) -> Result<(), String> {
    let ranges = settings
        .edit
        .working_hours
        .get(&day)
        .cloned()
        .unwrap_or_default();
    let mut previous_end: Option<i32> = None;
    for range in ranges {
        let start = hhmm_to_minutes(&range.start).map_err(|_| {
            format!(
                "{} has invalid start time {}",
                WEEKDAY_NAMES[day as usize], range.start
            )
        })?;
        let end = hhmm_to_minutes(&range.end).map_err(|_| {
            format!(
                "{} has invalid end time {}",
                WEEKDAY_NAMES[day as usize], range.end
            )
        })?;
        if end <= start {
            return Err(format!(
                "{} range invalid: end must be greater than start",
                WEEKDAY_NAMES[day as usize]
            ));
        }
        if let Some(prev_end) = previous_end
            && start <= prev_end
        {
            return Err(format!(
                "{} ranges must start after previous end",
                WEEKDAY_NAMES[day as usize]
            ));
        }
        previous_end = Some(end);
    }
    Ok(())
}

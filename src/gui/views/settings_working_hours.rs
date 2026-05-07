use eframe::egui;
use egui_phosphor_icons::icons;

use crate::config::TimeRange;
use crate::gui::style;

use super::{SettingsView, WEEKDAY_NAMES};

impl SettingsView {
    pub(super) fn render_working_hours_inline(&mut self, ui: &mut egui::Ui) {
        egui::Grid::new("working_hours_inline_grid")
            .num_columns(2)
            .spacing([12.0, 34.0])
            .show(ui, |ui| {
                for (day_idx, day_name) in WEEKDAY_NAMES.iter().enumerate() {
                    let day = day_idx as u8;
                    ui.label(*day_name);
                    self.render_day_inline(ui, day);
                    ui.end_row();
                }
            });
    }

    fn render_day_inline(&mut self, ui: &mut egui::Ui, day: u8) {
        let mut remove_idx: Option<usize> = None;
        let mut action_error: Option<String> = None;
        let len = self
            .edit
            .working_hours
            .get(&day)
            .map(std::vec::Vec::len)
            .unwrap_or(0);

        ui.vertical(|ui| {
            for idx in 0..len {
                let (start_value, end_value) = {
                    let range = &self.edit.working_hours.get(&day).expect("day exists")[idx];
                    (range.start.clone(), range.end.clone())
                };

                let start_error = validate_hhmm_input("start", &start_value);
                let end_error = validate_hhmm_input("end", &end_value);
                let range_error = validate_ordered_range(&start_value, &end_value);

                ui.horizontal(|ui| {
                    if let Some(range) = self
                        .edit
                        .working_hours
                        .get_mut(&day)
                        .and_then(|ranges| ranges.get_mut(idx))
                    {
                        style::padded_text_edit_sized_validated(
                            ui,
                            &mut range.start,
                            96.0,
                            start_error.as_deref(),
                        );
                    }

                    ui.label("-");
                    if let Some(range) = self
                        .edit
                        .working_hours
                        .get_mut(&day)
                        .and_then(|ranges| ranges.get_mut(idx))
                    {
                        style::padded_text_edit_sized_validated(
                            ui,
                            &mut range.end,
                            96.0,
                            end_error.as_deref().or(range_error.as_deref()),
                        );
                    }

                    if ui
                        .button(style::icon_label(ui, icons::TRASH, ""))
                        .on_hover_text("Delete range")
                        .clicked()
                    {
                        remove_idx = Some(idx);
                    }
                });

                if let Some(err) = start_error
                    .as_deref()
                    .or(end_error.as_deref())
                    .or(range_error.as_deref())
                {
                    let palette = style::validation_palette(ui);
                    ui.label(egui::RichText::new(err).size(13.0).color(palette.description));
                }
            }

            ui.horizontal(|ui| {
                if len < 4
                    && ui
                        .button(style::icon_label(ui, icons::PLUS, "Add range"))
                        .clicked()
                    && let Err(err) = self.add_working_hours_range(day)
                {
                    action_error = Some(err);
                }

                if day != 0 && len == 0 {
                    let monday_has_ranges = self
                        .edit
                        .working_hours
                        .get(&0)
                        .map(|ranges| !ranges.is_empty())
                        .unwrap_or(false);
                    if ui
                        .add_enabled(
                            monday_has_ranges,
                            egui::Button::new(style::icon_label(
                                ui,
                                icons::ARROW_LINE_DOWN,
                                "overtake from monday",
                            ))
                                .min_size(egui::vec2(0.0, ui.spacing().interact_size.y)),
                        )
                        .clicked()
                        && let Err(err) = self.overtake_ranges_from_monday(day)
                    {
                        action_error = Some(err);
                    }
                }
            });

            if let Some(idx) = remove_idx
                && let Some(ranges) = self.edit.working_hours.get_mut(&day)
                && idx < ranges.len()
            {
                ranges.remove(idx);
            }

            if let Some(err) = action_error {
                let palette = style::validation_palette(ui);
                ui.label(egui::RichText::new(err).size(13.0).color(palette.description));
            }

            if let Err(err) = validate_day_ranges(self, day) {
                let palette = style::validation_palette(ui);
                ui.label(egui::RichText::new(err).size(13.0).color(palette.description));
            }
        });
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

    fn overtake_ranges_from_monday(&mut self, day: u8) -> Result<(), String> {
        if day == 0 {
            return Err("monday cannot overtake from monday".to_string());
        }

        let monday_ranges = self
            .edit
            .working_hours
            .get(&0)
            .cloned()
            .unwrap_or_default();
        if monday_ranges.is_empty() {
            return Err("monday has no ranges to overtake".to_string());
        }

        if let Some(existing) = self.edit.working_hours.get(&day)
            && !existing.is_empty()
        {
            return Err("target day already has ranges".to_string());
        }

        self.edit
            .working_hours
            .insert(day, monday_ranges.into_iter().take(4).collect());
        Ok(())
    }
}

fn hhmm_to_minutes(raw: &str) -> Result<i32, String> {
    let (h, m) = crate::config::parse_hhmm(raw).map_err(|err| err.to_string())?;
    Ok((h as i32 * 60) + m as i32)
}

fn validate_hhmm_input(label: &str, raw: &str) -> Option<String> {
    if raw.trim().is_empty() {
        return Some(format!("{} time is required", label));
    }
    crate::config::parse_hhmm(raw)
        .err()
        .map(|_| format!("invalid {}; expected HH:mm", label))
}

fn validate_ordered_range(start: &str, end: &str) -> Option<String> {
    let start_m = hhmm_to_minutes(start).ok()?;
    let end_m = hhmm_to_minutes(end).ok()?;
    if end_m <= start_m {
        Some("end must be greater than start".to_string())
    } else {
        None
    }
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

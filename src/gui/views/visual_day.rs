use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime, Timelike};
use eframe::egui;
use egui_extras::DatePickerButton;
use egui_phosphor_icons::icons;
use std::collections::HashMap;

use crate::config::Config;
use crate::db;
use crate::tui::trackings_cleanup::cleanup_unsynced_trackings_in_range;
use crate::tui::trackings_storno::storno_tracking;

use super::super::style;

const DAY_SECONDS: i64 = 24 * 60 * 60;
const MIN_SECONDS: i64 = 60;
const LANE_H: f32 = 28.0;
const LANE_GAP: f32 = 16.0;
const HANDLE_W: f32 = 8.0;
const SNAP_STEP_SECONDS: i64 = 15 * 60;
const SNAP_THRESHOLD_PX: f32 = 10.0;

pub struct VisualDayView {
    selected_day: NaiveDate,
    day_modal: bool,
    drag: Option<DragState>,
    edit_modal: Option<TrackingForm>,
    confirm_delete_id: Option<i64>,
    zoom_x: f32,
    scroll_x: f32,
    last_timeline_viewport: Option<egui::Rect>,
    last_timeline_content_width: f32,
    zoom_initialized_for_day: Option<NaiveDate>,
}

#[derive(Clone)]
struct TrackingForm {
    id: i64,
    project_name: String,
    start_date: NaiveDate,
    start_time: String,
    end_date: NaiveDate,
    end_time: String,
    description: String,
    projects: Vec<String>,
    selected_project: usize,
}

#[derive(Clone, Copy)]
enum DragMode {
    Move,
    ResizeStart,
    ResizeEnd,
}

#[derive(Clone)]
struct DragState {
    tracking_id: i64,
    mode: DragMode,
    start_pointer_x: f32,
    orig_start_sec: i64,
    orig_end_sec: i64,
    candidate_start_sec: i64,
    candidate_end_sec: i64,
    candidate_project_name: String,
    notes: Option<String>,
    is_invalid: bool,
    snap_lines_sec: [Option<i64>; 2],
}

impl Default for VisualDayView {
    fn default() -> Self {
        Self {
            selected_day: Local::now().date_naive(),
            day_modal: false,
            drag: None,
            edit_modal: None,
            confirm_delete_id: None,
            zoom_x: 1.0,
            scroll_x: 0.0,
            last_timeline_viewport: None,
            last_timeline_content_width: 0.0,
            zoom_initialized_for_day: None,
        }
    }
}

impl VisualDayView {
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        config: &Config,
    ) -> Option<String> {
        let conn = db::open(config.db_path()).ok()?;
        let mut message = None;
        let mut selected_day = self.selected_day;
        let selected_day_label = selected_day.format("%Y-%m-%d").to_string();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Visual Day").size(18.0).strong());
            if ui
                .button(style::icon_label(ui, icons::ARROW_LEFT, ""))
                .on_hover_text("Previous day")
                .clicked()
            {
                selected_day = selected_day.pred_opt().unwrap_or(selected_day);
            }
            if ui
                .button(style::icon_label(ui, icons::SLIDERS, &selected_day_label))
                .on_hover_text("Select day")
                .clicked()
            {
                self.day_modal = true;
            }
            if ui
                .button(style::icon_label(ui, icons::ARROW_RIGHT, ""))
                .on_hover_text("Next day")
                .clicked()
            {
                selected_day = selected_day.succ_opt().unwrap_or(selected_day);
            }
            if ui
                .button(style::icon_label(ui, icons::CALENDAR_DOT, ""))
                .on_hover_text("Today")
                .clicked()
            {
                selected_day = Local::now().date_naive();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let is_today = selected_day == Local::now().date_naive();
                if ui
                    .add_enabled(
                        is_today,
                        egui::Button::new(style::icon_label(ui, icons::BROOM, "")),
                    )
                    .on_hover_text("Merge adjacent unsynced trackings for shown day")
                    .clicked()
                    && let Ok(stats) = cleanup_unsynced_trackings_in_range(
                        &conn,
                        &self.selected_day.format("%Y-%m-%d").to_string(),
                        &self.selected_day.format("%Y-%m-%d").to_string(),
                    )
                {
                    message = Some(if stats.removed_rows == 0 {
                        "cleanup: nothing to merge".to_string()
                    } else {
                        format!(
                            "cleanup: merged {} groups, removed {} rows",
                            stats.merged_groups, stats.removed_rows
                        )
                    });
                }
            });
        });

        self.selected_day = selected_day;
        let day_key = self.selected_day.format("%Y-%m-%d").to_string();
        let day_label = self.selected_day.format("%A, %Y-%m-%d").to_string();
        ui.add_space(6.0);
        ui.label(egui::RichText::new(day_label).weak());

        let mut trackings = db::list_trackings_for_date(&conn, &day_key).unwrap_or_default();
        let all_projects = db::projects(&conn).unwrap_or_default();
        let project_colors: HashMap<String, String> = all_projects
            .iter()
            .map(|p| {
                let color = p
                    .color
                    .clone()
                    .unwrap_or_else(|| crate::gui::color::generate_color_for_name(&p.name));
                (p.name.clone(), color)
            })
            .collect();
        let mut project_lanes: HashMap<String, usize> = HashMap::new();
        let mut lane_projects: Vec<String> = Vec::new();
        for p in &all_projects {
            let idx = lane_projects.len();
            project_lanes.insert(p.name.clone(), idx);
            lane_projects.push(p.name.clone());
        }
        for t in &trackings {
            if project_lanes.contains_key(&t.project_name) {
                continue;
            }
            let idx = lane_projects.len();
            project_lanes.insert(t.project_name.clone(), idx);
            lane_projects.push(t.project_name.clone());
        }
        if lane_projects.is_empty() {
            lane_projects.push("(no projects)".to_string());
            project_lanes.insert("(no projects)".to_string(), 0);
        }
        let zoom_delta = ctx.input(|i| {
            if i.modifiers.ctrl {
                i.zoom_delta()
            } else {
                1.0
            }
        });
        if zoom_delta.is_finite() && (zoom_delta - 1.0).abs() > f32::EPSILON {
            let prev_zoom = self.zoom_x;
            let next_zoom = (self.zoom_x * zoom_delta).clamp(1.0, 6.0);
            if prev_zoom > 0.0 {
                let scale = next_zoom / prev_zoom;
                let pointer = ctx.input(|i| i.pointer.hover_pos());
                if let (Some(pos), Some(viewport)) = (pointer, self.last_timeline_viewport)
                    && viewport.contains(pos)
                    && self.last_timeline_content_width > 0.0
                {
                    let anchor_x = (pos.x - viewport.left()).clamp(0.0, viewport.width());
                    let content_x = self.scroll_x + anchor_x;
                    self.scroll_x = (content_x * scale - anchor_x).max(0.0);
                } else {
                    self.scroll_x *= scale;
                }
            }
            self.zoom_x = next_zoom;
        }

        let mut timeline_hovered = false;
        let mut max_scroll_x = 0.0_f32;
        egui::ScrollArea::vertical().show(ui, |ui| {
            let lane_count = lane_projects.len().max(1);
            let chart_height = (lane_count as f32 * (LANE_H + LANE_GAP)) + 60.0;
            let left_label_width = 180.0;
            let viewport_width = ui.available_width().max(420.0);
            let timeline_base_width = (viewport_width - left_label_width - 20.0).max(320.0);

            if self.zoom_initialized_for_day != Some(self.selected_day) {
                let (focus_start, focus_end) =
                    default_working_hours_focus_window(config, self.selected_day);
                let focus_len = (focus_end - focus_start).max(60);
                let zoom = (DAY_SECONDS as f32 / focus_len as f32).clamp(1.0, 6.0);
                let timeline_width_for_zoom = timeline_base_width * zoom;
                let start_ratio = focus_start as f32 / DAY_SECONDS as f32;
                let max_scroll = (timeline_width_for_zoom - timeline_base_width).max(0.0);
                self.zoom_x = zoom;
                self.scroll_x = (timeline_width_for_zoom * start_ratio).clamp(0.0, max_scroll);
                self.zoom_initialized_for_day = Some(self.selected_day);
            }

            let timeline_width = timeline_base_width * self.zoom_x;
            let row_height = chart_height.max(260.0);

            ui.horizontal(|ui| {
                let (label_rect, _) = ui.allocate_exact_size(
                    egui::vec2(left_label_width, row_height),
                    egui::Sense::hover(),
                );
                let label_painter = ui.painter_at(label_rect);
                let visuals = ui.visuals();
                label_painter.rect_filled(label_rect, 6.0, visuals.faint_bg_color);
                let label_lane_rect = egui::Rect::from_min_max(
                    egui::pos2(label_rect.left() + 8.0, label_rect.top() + 28.0),
                    egui::pos2(label_rect.right() - 8.0, label_rect.bottom() - 12.0),
                );
                label_painter.line_segment(
                    [
                        egui::pos2(label_rect.right() - 2.0, label_lane_rect.top()),
                        egui::pos2(label_rect.right() - 2.0, label_lane_rect.bottom()),
                    ],
                    egui::Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color),
                );

                let out = egui::ScrollArea::horizontal()
                    .id_salt("visual_day_timeline_scroll")
                    .horizontal_scroll_offset(self.scroll_x)
                    .show(ui, |ui| {
                        let (timeline_rect, response) = ui.allocate_exact_size(
                            egui::vec2(timeline_width, row_height),
                            egui::Sense::hover(),
                        );
                        timeline_hovered |= response.hovered();

                        let painter = ui.painter_at(timeline_rect);
                        let visuals = ui.visuals();
                        painter.rect_filled(timeline_rect, 6.0, visuals.faint_bg_color);

                        let chart_rect = egui::Rect::from_min_max(
                            egui::pos2(timeline_rect.left() + 10.0, timeline_rect.top() + 28.0),
                            egui::pos2(timeline_rect.right() - 10.0, timeline_rect.bottom() - 12.0),
                        );

                        draw_working_hours_shading(&painter, chart_rect, config, self.selected_day);
                        draw_hour_grid(&painter, chart_rect, visuals);

                        for (idx, project_name) in lane_projects.iter().enumerate() {
                            let y = chart_rect.top() + idx as f32 * (LANE_H + LANE_GAP);
                            let label_y = y + (LANE_H * 0.5);
                            let color_hex = project_colors
                                .get(project_name)
                                .cloned()
                                .unwrap_or_else(|| {
                                    crate::gui::color::generate_color_for_name(project_name)
                                });
                            let color = crate::gui::color::color32_from_hex(&color_hex)
                                .unwrap_or(egui::Color32::LIGHT_BLUE)
                                .gamma_multiply(0.90);
                            let swatch_rect = egui::Rect::from_min_size(
                                egui::pos2(label_lane_rect.left(), label_y - 6.0),
                                egui::vec2(12.0, 12.0),
                            );
                            label_painter.rect_filled(swatch_rect, 2.0, color);
                            label_painter.rect_stroke(
                                swatch_rect,
                                2.0,
                                egui::Stroke::new(1.0, color.gamma_multiply(0.65)),
                                egui::StrokeKind::Outside,
                            );
                            label_painter.text(
                                egui::pos2(label_lane_rect.left() + 18.0, label_y),
                                egui::Align2::LEFT_CENTER,
                                project_name,
                                egui::FontId::proportional(13.0),
                                visuals.text_color(),
                            );
                            if idx + 1 < lane_projects.len() {
                                let sep_y = y + LANE_H + 8.0;
                                let sep_stroke = egui::Stroke::new(
                                    1.0,
                                    visuals.widgets.noninteractive.bg_stroke.color,
                                );
                                label_painter.line_segment(
                                    [
                                        egui::pos2(label_lane_rect.left(), sep_y),
                                        egui::pos2(label_lane_rect.right(), sep_y),
                                    ],
                                    sep_stroke,
                                );
                                painter.line_segment(
                                    [
                                        egui::pos2(chart_rect.left(), sep_y),
                                        egui::pos2(chart_rect.right(), sep_y),
                                    ],
                                    sep_stroke,
                                );
                            }
                        }

                        let day_start = compose_local_day_start(self.selected_day);
                        let snap_ranges: Vec<(i64, i64, i64)> = if let Some(day_start) = day_start {
                            trackings
                                .iter()
                                .filter_map(|t| {
                                    let start_utc = crate::time::parse_ts(&t.start_ts).ok()?;
                                    let end_utc = t
                                        .end_ts
                                        .as_deref()
                                        .and_then(|raw| crate::time::parse_ts(raw).ok())
                                        .unwrap_or_else(chrono::Utc::now);
                                    let start_sec = start_utc
                                        .signed_duration_since(day_start)
                                        .num_seconds()
                                        .clamp(0, DAY_SECONDS);
                                    let end_sec = end_utc
                                        .signed_duration_since(day_start)
                                        .num_seconds()
                                        .clamp(0, DAY_SECONDS);
                                    if end_sec <= start_sec {
                                        return None;
                                    }
                                    Some((t.id, start_sec, end_sec))
                                })
                                .collect()
                        } else {
                            Vec::new()
                        };
                        if let Some(drag) = self.drag.as_mut() {
                            update_drag_candidate(ctx, drag, chart_rect, &snap_ranges);
                            if matches!(drag.mode, DragMode::Move)
                                && let Some(pointer_y) =
                                    ctx.input(|i| i.pointer.latest_pos().map(|p| p.y))
                            {
                                let lane_span = LANE_H + LANE_GAP;
                                if lane_span > 0.0 && !lane_projects.is_empty() {
                                    let raw_lane = ((pointer_y - chart_rect.top()) / lane_span)
                                        .floor()
                                        as isize;
                                    let clamped_lane = raw_lane
                                        .clamp(0, lane_projects.len().saturating_sub(1) as isize)
                                        as usize;
                                    if let Some(project_name) = lane_projects.get(clamped_lane) {
                                        drag.candidate_project_name = project_name.clone();
                                    }
                                }
                            }
                            if let Some((cand_start, cand_end)) = sec_range_to_ts(
                                self.selected_day,
                                drag.candidate_start_sec,
                                drag.candidate_end_sec,
                            ) {
                                drag.is_invalid = db::has_overlap_for_day(
                                    &conn,
                                    &day_key,
                                    Some(drag.tracking_id),
                                    &cand_start,
                                    &cand_end,
                                )
                                .unwrap_or(true);
                            } else {
                                drag.is_invalid = true;
                            }
                        }

                        if self.drag.is_none()
                            && let Some(day_start) = day_start
                            && let Some(pointer_pos) = ctx.input(|i| i.pointer.hover_pos())
                            && chart_rect.contains(pointer_pos)
                        {
                            let lane_span = LANE_H + LANE_GAP;
                            let raw_lane =
                                ((pointer_pos.y - chart_rect.top()) / lane_span).floor() as isize;
                            let lane_idx = raw_lane
                                .clamp(0, lane_projects.len().saturating_sub(1) as isize)
                                as usize;
                            let lane_top = chart_rect.top() + lane_idx as f32 * lane_span;
                            if pointer_pos.y <= lane_top + LANE_H
                                && let Some(project_name) = lane_projects.get(lane_idx)
                                && project_name != "(no projects)"
                            {
                                let pointer_sec = (((pointer_pos.x - chart_rect.left())
                                    / chart_rect.width().max(1.0))
                                    * DAY_SECONDS as f32)
                                    .floor()
                                    as i64;
                                let hour_start =
                                    (pointer_sec.clamp(0, DAY_SECONDS - 1) / 3600) * 3600;
                                let hour_end = (hour_start + 3600).min(DAY_SECONDS);

                                let mut overlap_same_project = false;
                                let mut overlap_other_project = false;
                                for t in &trackings {
                                    let Some(start_utc) = crate::time::parse_ts(&t.start_ts).ok()
                                    else {
                                        continue;
                                    };
                                    let end_utc = t
                                        .end_ts
                                        .as_deref()
                                        .and_then(|raw| crate::time::parse_ts(raw).ok())
                                        .unwrap_or_else(chrono::Utc::now);
                                    let start_sec = start_utc
                                        .signed_duration_since(day_start)
                                        .num_seconds()
                                        .clamp(0, DAY_SECONDS);
                                    let end_sec = end_utc
                                        .signed_duration_since(day_start)
                                        .num_seconds()
                                        .clamp(0, DAY_SECONDS);
                                    if !(start_sec < hour_end && hour_start < end_sec) {
                                        continue;
                                    }
                                    if t.project_name == *project_name {
                                        overlap_same_project = true;
                                    } else {
                                        overlap_other_project = true;
                                    }
                                    if overlap_same_project && overlap_other_project {
                                        break;
                                    }
                                }

                                if !overlap_same_project {
                                    let hour_rect = egui::Rect::from_min_max(
                                        egui::pos2(sec_to_x(chart_rect, hour_start), lane_top),
                                        egui::pos2(
                                            sec_to_x(chart_rect, hour_end),
                                            lane_top + LANE_H,
                                        ),
                                    );
                                    let plus_size = 16.0;
                                    let plus_rect = egui::Rect::from_min_size(
                                        egui::pos2(
                                            hour_rect.left() + 14.0,
                                            lane_top + ((LANE_H - plus_size) * 0.5) - 3.0,
                                        ),
                                        egui::vec2(plus_size, plus_size),
                                    );
                                    let plus_label = egui::RichText::new(icons::PLUS.as_str())
                                        .size(13.0)
                                        .family(egui::FontFamily::Name("phosphor-regular".into()));
                                    let plus_resp = ui
                                        .add_enabled_ui(!overlap_other_project, |ui| {
                                            ui.put(
                                                plus_rect,
                                                egui::Button::new(plus_label)
                                                    .min_size(egui::vec2(plus_size, plus_size)),
                                            )
                                        })
                                        .inner;
                                    if overlap_other_project {
                                        plus_resp.clone().on_hover_text(
                                            "disabled: hour already used by another project",
                                        );
                                    }
                                    if plus_resp.clicked()
                                        && let Some((start_s, end_s)) =
                                            sec_range_to_ts(self.selected_day, hour_start, hour_end)
                                    {
                                        message = Some(
                                            match db::add_manual_tracking(
                                                &conn,
                                                project_name,
                                                &start_s,
                                                Some(&end_s),
                                                None,
                                            ) {
                                                Ok(_) => "tracking added".to_string(),
                                                Err(err) => format!("error: {err}"),
                                            },
                                        );
                                    }
                                }
                            }
                        }

                        for t in trackings.iter_mut() {
                            let Some(start_utc) = crate::time::parse_ts(&t.start_ts).ok() else {
                                continue;
                            };
                            let Some(day_start) = day_start else {
                                continue;
                            };
                            let end_utc = t
                                .end_ts
                                .as_deref()
                                .and_then(|raw| crate::time::parse_ts(raw).ok())
                                .unwrap_or_else(chrono::Utc::now);
                            let mut start_sec = start_utc
                                .signed_duration_since(day_start)
                                .num_seconds()
                                .clamp(0, DAY_SECONDS);
                            let mut end_sec = end_utc
                                .signed_duration_since(day_start)
                                .num_seconds()
                                .clamp(0, DAY_SECONDS);

                            if let Some(drag) = self.drag.as_ref()
                                && drag.tracking_id == t.id
                            {
                                start_sec = drag.candidate_start_sec;
                                end_sec = drag.candidate_end_sec;
                            }
                            if end_sec <= start_sec {
                                continue;
                            }

                            let lane_project = if let Some(drag) = self.drag.as_ref()
                                && drag.tracking_id == t.id
                            {
                                &drag.candidate_project_name
                            } else {
                                &t.project_name
                            };
                            let Some(&lane_idx) = project_lanes.get(lane_project) else {
                                continue;
                            };
                            let y = chart_rect.top() + lane_idx as f32 * (LANE_H + LANE_GAP);
                            let bar_rect = egui::Rect::from_min_max(
                                egui::pos2(sec_to_x(chart_rect, start_sec), y),
                                egui::pos2(
                                    sec_to_x(chart_rect, end_sec)
                                        .max(sec_to_x(chart_rect, start_sec) + 2.0),
                                    y + LANE_H,
                                ),
                            );
                            let color_hex = project_colors
                                .get(lane_project)
                                .cloned()
                                .unwrap_or_else(|| {
                                    crate::gui::color::generate_color_for_name(lane_project)
                                });
                            let color = crate::gui::color::color32_from_hex(&color_hex)
                                .unwrap_or(egui::Color32::LIGHT_BLUE);
                            let invalid = self
                                .drag
                                .as_ref()
                                .is_some_and(|d| d.tracking_id == t.id && d.is_invalid);

                            let body_rect = egui::Rect::from_min_max(
                                egui::pos2(
                                    (bar_rect.left() + HANDLE_W).min(bar_rect.right()),
                                    bar_rect.top(),
                                ),
                                egui::pos2(
                                    (bar_rect.right() - HANDLE_W).max(bar_rect.left()),
                                    bar_rect.bottom(),
                                ),
                            );
                            let left_handle = egui::Rect::from_min_max(
                                bar_rect.min,
                                egui::pos2(
                                    (bar_rect.left() + HANDLE_W).min(bar_rect.right()),
                                    bar_rect.bottom(),
                                ),
                            );
                            let right_handle = egui::Rect::from_min_max(
                                egui::pos2(
                                    (bar_rect.right() - HANDLE_W).max(bar_rect.left()),
                                    bar_rect.top(),
                                ),
                                bar_rect.max,
                            );

                            let body_resp = ui
                                .interact(
                                    body_rect,
                                    ui.id().with(("visual_day_bar", t.id)),
                                    egui::Sense::click_and_drag(),
                                )
                                .on_hover_cursor(egui::CursorIcon::Grab);
                            if body_resp.dragged() {
                                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                            }
                            let left_resp = ui
                                .interact(
                                    left_handle,
                                    ui.id().with(("visual_day_left", t.id)),
                                    egui::Sense::click_and_drag(),
                                )
                                .on_hover_and_drag_cursor(egui::CursorIcon::ResizeHorizontal);
                            let right_resp = ui
                                .interact(
                                    right_handle,
                                    ui.id().with(("visual_day_right", t.id)),
                                    egui::Sense::click_and_drag(),
                                )
                                .on_hover_and_drag_cursor(egui::CursorIcon::ResizeHorizontal);

                            let drag_ready = t.jira_synced == 0
                                && (body_resp.hovered()
                                    || left_resp.hovered()
                                    || right_resp.hovered()
                                    || body_resp.dragged()
                                    || left_resp.dragged()
                                    || right_resp.dragged());
                            let fill = if invalid {
                                egui::Color32::from_rgb(200, 60, 60)
                            } else if drag_ready {
                                color.gamma_multiply(1.15)
                            } else {
                                color.gamma_multiply(0.90)
                            };

                            painter.rect_filled(bar_rect, 4.0, fill);
                            painter.rect_stroke(
                                bar_rect,
                                4.0,
                                egui::Stroke::new(1.0, fill.gamma_multiply(0.65)),
                                egui::StrokeKind::Outside,
                            );
                            if let Some(description) =
                                t.notes.as_deref().map(str::trim).filter(|s| !s.is_empty())
                            {
                                let text_rect = body_rect.shrink2(egui::vec2(4.0, 1.0));
                                if text_rect.width() > 28.0 {
                                    let brightness = ((u32::from(fill.r()) * 299
                                        + u32::from(fill.g()) * 587
                                        + u32::from(fill.b()) * 114)
                                        / 1000)
                                        as u8;
                                    let text_color = if brightness < 128 {
                                        egui::Color32::from_white_alpha(235)
                                    } else {
                                        egui::Color32::from_black_alpha(220)
                                    };
                                    painter.with_clip_rect(text_rect).text(
                                        egui::pos2(text_rect.left(), text_rect.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        description,
                                        egui::FontId::proportional(12.0),
                                        text_color,
                                    );
                                }
                            }
                            if self.drag.is_none() {
                                let hover_snap_line_sec = if left_resp.hovered() {
                                    Some(start_sec)
                                } else if right_resp.hovered() {
                                    Some(end_sec)
                                } else {
                                    None
                                };
                                if let Some(snap_sec) = hover_snap_line_sec {
                                    let x = sec_to_x(chart_rect, snap_sec);
                                    painter.line_segment(
                                        [
                                            egui::pos2(x, chart_rect.top()),
                                            egui::pos2(x, chart_rect.bottom()),
                                        ],
                                        egui::Stroke::new(
                                            2.0,
                                            egui::Color32::from_rgb(255, 196, 64),
                                        ),
                                    );
                                }
                            }

                            let duration_secs = (end_sec - start_sec).max(0);
                            let start_label = sec_to_hhmm(start_sec);
                            let end_label = sec_to_hhmm(end_sec);
                            let notes = t.notes.as_deref().unwrap_or("-");
                            let hover_text = format!(
                                "{}\n{} - {} ({})\n{}",
                                t.project_name,
                                start_label,
                                end_label,
                                format_duration_hm(duration_secs),
                                notes
                            );
                            body_resp.clone().on_hover_text(hover_text.clone());
                            left_resp.clone().on_hover_text(hover_text.clone());
                            right_resp.clone().on_hover_text(hover_text);

                            body_resp.context_menu(|ui| {
                                if ui
                                    .add_enabled(t.jira_synced == 0, egui::Button::new("Edit"))
                                    .clicked()
                                {
                                    if let Some(form) = form_from_tracking(t, &conn) {
                                        self.edit_modal = Some(form);
                                    }
                                    ui.close();
                                }
                                if ui
                                    .add_enabled(t.jira_synced == 0, egui::Button::new("Delete"))
                                    .clicked()
                                {
                                    self.confirm_delete_id = Some(t.id);
                                    ui.close();
                                }
                                if ui.button("Copy").clicked() {
                                    ctx.copy_text(format!(
                                        "{} | {}-{} | {}",
                                        t.project_name, start_label, end_label, notes
                                    ));
                                    message = Some("row copied".to_string());
                                    ui.close();
                                }
                                if ui.button("Storno").clicked() {
                                    message = Some(match storno_tracking(&conn, config, t) {
                                        Ok(msg) => msg,
                                        Err(err) => format!("error: {err}"),
                                    });
                                    ui.close();
                                }
                            });

                            if t.jira_synced == 0
                                && (body_resp.double_clicked()
                                    || left_resp.double_clicked()
                                    || right_resp.double_clicked())
                                && let Some(form) = form_from_tracking(t, &conn)
                            {
                                self.edit_modal = Some(form);
                            }

                            if self.drag.is_none()
                                && t.jira_synced == 0
                                && let Some(pos) = ctx.input(|i| i.pointer.press_origin())
                            {
                                if body_resp.drag_started() {
                                    self.drag = Some(make_drag_state(
                                        t,
                                        DragMode::Move,
                                        start_sec,
                                        end_sec,
                                        pos.x,
                                    ));
                                } else if left_resp.drag_started() {
                                    self.drag = Some(make_drag_state(
                                        t,
                                        DragMode::ResizeStart,
                                        start_sec,
                                        end_sec,
                                        pos.x,
                                    ));
                                } else if right_resp.drag_started() {
                                    self.drag = Some(make_drag_state(
                                        t,
                                        DragMode::ResizeEnd,
                                        start_sec,
                                        end_sec,
                                        pos.x,
                                    ));
                                }
                            }
                        }

                        if let Some(drag) = self.drag.as_ref() {
                            for snap_sec in drag.snap_lines_sec.iter().flatten() {
                                let x = sec_to_x(chart_rect, *snap_sec);
                                painter.line_segment(
                                    [
                                        egui::pos2(x, chart_rect.top()),
                                        egui::pos2(x, chart_rect.bottom()),
                                    ],
                                    egui::Stroke::new(2.0, egui::Color32::from_rgb(255, 196, 64)),
                                );
                            }
                            draw_active_drag_tooltip(ctx, &painter, chart_rect, drag);
                        }
                    });

                self.scroll_x = out.state.offset.x;
                self.last_timeline_viewport = Some(out.inner_rect);
                self.last_timeline_content_width = out.content_size.x;
                max_scroll_x = (out.content_size.x - out.inner_rect.width()).max(0.0);
                self.scroll_x = self.scroll_x.clamp(0.0, max_scroll_x);
            });
        });

        let (ctrl, wheel) = ctx.input(|i| (i.modifiers.ctrl, i.smooth_scroll_delta));
        if timeline_hovered && !ctrl {
            let dx = wheel.x + wheel.y;
            if dx.abs() > f32::EPSILON {
                self.scroll_x = (self.scroll_x - dx).clamp(0.0, max_scroll_x);
            }
        }

        if self.drag.is_some() && !ctx.input(|i| i.pointer.primary_down()) {
            if let Some(drag) = self.drag.take() {
                if drag.is_invalid {
                    message = Some("invalid move: overlap detected".to_string());
                } else if let Some((start_s, end_s)) = sec_range_to_ts(
                    self.selected_day,
                    drag.candidate_start_sec,
                    drag.candidate_end_sec,
                ) {
                    let res = db::update_tracking_times(
                        &conn,
                        drag.tracking_id,
                        &drag.candidate_project_name,
                        &start_s,
                        Some(&end_s),
                        drag.notes.as_deref(),
                    );
                    message = Some(match res {
                        Ok(_) => "tracking updated".to_string(),
                        Err(err) => format!("error: {err}"),
                    });
                }
            }
        }

        if self.day_modal {
            let esc_pressed = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            let today = Local::now().date_naive();
            let mut day = self.selected_day;
            style::draw_modal_backdrop(ctx);
            egui::Window::new("Filter")
                .order(egui::Order::Foreground)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(style::DIALOG_MARGIN))
                        .show(ui, |ui| {
                            style::setting_row(ui, "Day", "", 110.0, |ui| {
                                ui.horizontal(|ui| {
                                    ui.set_min_height(style::text_field_height(ui));
                                    ui.add_sized(
                                        [124.0, style::text_field_height(ui)],
                                        DatePickerButton::new(&mut day)
                                            .id_salt("visual_day_filter_day"),
                                    );
                                });
                            });

                            ui.separator();
                            ui.horizontal(|ui| {
                                if ui
                                    .button(style::icon_label(ui, icons::CHECK, "OK"))
                                    .clicked()
                                {
                                    self.selected_day = day;
                                    self.day_modal = false;
                                }
                                if ui
                                    .button(style::icon_label(ui, icons::CALENDAR_DOT, "Today"))
                                    .clicked()
                                {
                                    self.selected_day = today;
                                    self.day_modal = false;
                                }
                                if ui
                                    .button(style::icon_label(ui, icons::X, "Cancel"))
                                    .clicked()
                                    || esc_pressed
                                {
                                    self.day_modal = false;
                                }
                            });
                        });
                });
        }

        if let Some(mut form) = self.edit_modal.clone() {
            let mut keep_open = true;
            let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            style::draw_modal_backdrop(ctx);
            egui::Window::new("Edit tracking")
                .order(egui::Order::Foreground)
                .collapsible(false)
                .resizable(false)
                .min_width(480.0)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(style::DIALOG_MARGIN))
                        .show(ui, |ui| {
                            style::setting_row(ui, "Project", "", 110.0, |ui| {
                                egui::ComboBox::from_id_salt("visual_day_project")
                                    .width(ui.available_width())
                                    .selected_text(
                                        form.projects
                                            .get(form.selected_project)
                                            .cloned()
                                            .unwrap_or_default(),
                                    )
                                    .show_ui(ui, |ui| {
                                        for (idx, p) in form.projects.iter().enumerate() {
                                            ui.selectable_value(&mut form.selected_project, idx, p);
                                        }
                                    });
                                if let Some(name) = form.projects.get(form.selected_project) {
                                    form.project_name = name.clone();
                                }
                            });
                            style::setting_text_row(
                                ui,
                                "Start",
                                "YYYY-mm-dd HH:MM",
                                110.0,
                                &mut format!(
                                    "{} {}",
                                    form.start_date.format("%Y-%m-%d"),
                                    form.start_time
                                ),
                            );
                            style::setting_row(ui, "Start date/time", "", 110.0, |ui| {
                                ui.add_sized(
                                    [124.0, style::text_field_height(ui)],
                                    DatePickerButton::new(&mut form.start_date)
                                        .id_salt("visual_day_start_date"),
                                );
                                style::padded_text_edit_sized_validated(
                                    ui,
                                    &mut form.start_time,
                                    96.0,
                                    None,
                                );
                            });
                            style::setting_row(ui, "End date/time", "", 110.0, |ui| {
                                ui.add_sized(
                                    [124.0, style::text_field_height(ui)],
                                    DatePickerButton::new(&mut form.end_date)
                                        .id_salt("visual_day_end_date"),
                                );
                                style::padded_text_edit_sized_validated(
                                    ui,
                                    &mut form.end_time,
                                    96.0,
                                    None,
                                );
                            });
                            style::setting_row(ui, "Description", "", 110.0, |ui| {
                                style::padded_text_edit_fill(ui, &mut form.description);
                            });
                            ui.separator();
                            ui.horizontal(|ui| {
                                if ui
                                    .button(style::icon_label(ui, icons::CHECK, "OK"))
                                    .clicked()
                                {
                                    match save_form(&conn, self.selected_day, &form) {
                                        Ok(msg) => {
                                            message = Some(msg);
                                            keep_open = false;
                                        }
                                        Err(err) => message = Some(err),
                                    }
                                }
                                if ui
                                    .button(style::icon_label(ui, icons::X, "Cancel"))
                                    .clicked()
                                    || esc
                                {
                                    keep_open = false;
                                }
                            });
                        });
                });
            self.edit_modal = if keep_open { Some(form) } else { None };
        }

        if let Some(id) = self.confirm_delete_id {
            let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            style::draw_modal_backdrop(ctx);
            egui::Window::new("Confirm delete")
                .order(egui::Order::Foreground)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(style::DIALOG_MARGIN))
                        .show(ui, |ui| {
                            ui.label("Delete selected tracking?");
                            ui.horizontal(|ui| {
                                if ui
                                    .button(style::icon_label(ui, icons::CHECK, "OK"))
                                    .clicked()
                                {
                                    if db::delete_tracking(&conn, id).is_ok() {
                                        message = Some("tracking deleted".to_string());
                                    }
                                    self.confirm_delete_id = None;
                                }
                                if ui
                                    .button(style::icon_label(ui, icons::X, "Cancel"))
                                    .clicked()
                                    || esc
                                {
                                    self.confirm_delete_id = None;
                                }
                            });
                        });
                });
        }

        message
    }
}

fn draw_hour_grid(painter: &egui::Painter, rect: egui::Rect, visuals: &egui::Visuals) {
    for hour in 0..=24 {
        let x = sec_to_x(rect, i64::from(hour) * 3600);
        let stroke = if hour % 2 == 0 {
            egui::Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color)
        } else {
            egui::Stroke::new(1.0, visuals.faint_bg_color)
        };
        painter.line_segment(
            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
            stroke,
        );
        if hour <= 23 {
            painter.text(
                egui::pos2(x + 2.0, rect.top() - 18.0),
                egui::Align2::LEFT_TOP,
                format!("{:02}:00", hour),
                egui::FontId::proportional(11.0),
                visuals.weak_text_color(),
            );
        }
    }
}

fn draw_working_hours_shading(
    painter: &egui::Painter,
    rect: egui::Rect,
    config: &Config,
    day: NaiveDate,
) {
    let weekday = day.weekday().num_days_from_monday() as u8;
    let Some(ranges) = config.working_hours.get(&weekday) else {
        return;
    };
    painter.rect_filled(rect, 0.0, egui::Color32::from_black_alpha(36));
    for range in ranges {
        let Ok((sh, sm)) = crate::config::parse_hhmm(&range.start) else {
            continue;
        };
        let Ok((eh, em)) = crate::config::parse_hhmm(&range.end) else {
            continue;
        };
        let s = i64::from(sh * 60 + sm) * 60;
        let e = i64::from(eh * 60 + em) * 60;
        if e <= s {
            continue;
        }
        let shade = egui::Rect::from_min_max(
            egui::pos2(sec_to_x(rect, s), rect.top()),
            egui::pos2(sec_to_x(rect, e), rect.bottom()),
        );
        painter.rect_filled(shade, 0.0, egui::Color32::from_white_alpha(28));
    }
}

fn make_drag_state(
    t: &db::Tracking,
    mode: DragMode,
    start_sec: i64,
    end_sec: i64,
    pointer_x: f32,
) -> DragState {
    DragState {
        tracking_id: t.id,
        mode,
        start_pointer_x: pointer_x,
        orig_start_sec: start_sec,
        orig_end_sec: end_sec,
        candidate_start_sec: start_sec,
        candidate_end_sec: end_sec,
        candidate_project_name: t.project_name.clone(),
        notes: t.notes.clone(),
        is_invalid: false,
        snap_lines_sec: [None, None],
    }
}

fn update_drag_candidate(
    ctx: &egui::Context,
    drag: &mut DragState,
    chart_rect: egui::Rect,
    snap_ranges: &[(i64, i64, i64)],
) {
    let pointer_x = ctx
        .input(|i| i.pointer.latest_pos().map(|p| p.x))
        .unwrap_or(drag.start_pointer_x);
    let dx = pointer_x - drag.start_pointer_x;
    let delta_sec = ((dx / chart_rect.width().max(1.0)) * DAY_SECONDS as f32).round() as i64;
    let snap_enabled = !ctx.input(|i| i.modifiers.alt);
    let snap_threshold_sec =
        (((SNAP_THRESHOLD_PX / chart_rect.width().max(1.0)) * DAY_SECONDS as f32).round() as i64)
            .clamp(MIN_SECONDS, SNAP_STEP_SECONDS);
    let mut snap_targets: Vec<i64> = Vec::with_capacity(snap_ranges.len() * 2);
    for (id, start_sec, end_sec) in snap_ranges {
        if *id != drag.tracking_id {
            snap_targets.push(*start_sec);
            snap_targets.push(*end_sec);
        }
    }

    let nearest_target = |raw: i64, min: i64, max: i64| -> i64 {
        let mut snapped = ((raw + SNAP_STEP_SECONDS / 2) / SNAP_STEP_SECONDS) * SNAP_STEP_SECONDS;
        snapped = snapped.clamp(min, max);
        for target in &snap_targets {
            let cand = (*target).clamp(min, max);
            if (cand - raw).abs() <= snap_threshold_sec
                && (cand - raw).abs() < (snapped - raw).abs()
            {
                snapped = cand;
            }
        }
        snapped
    };

    match drag.mode {
        DragMode::Move => {
            let len = (drag.orig_end_sec - drag.orig_start_sec).max(MIN_SECONDS);
            let mut s = drag.orig_start_sec + delta_sec;
            let mut e = s + len;
            drag.snap_lines_sec = [None, None];
            if s < 0 {
                s = 0;
                e = len;
            }
            if e > DAY_SECONDS {
                e = DAY_SECONDS;
                s = e - len;
            }
            if snap_enabled {
                let max_start = (DAY_SECONDS - len).max(0);
                let mut best_start = nearest_target(s, 0, max_start);
                let mut best_dist = (best_start - s).abs();
                let mut best_line_sec = Some(best_start + len);
                if !snap_targets.is_empty() {
                    for target in &snap_targets {
                        if *target < len || *target > DAY_SECONDS {
                            continue;
                        }
                        let cand = (*target - len).clamp(0, max_start);
                        let dist = (cand - s).abs();
                        if (*target - e).abs() <= snap_threshold_sec && dist < best_dist {
                            best_start = cand;
                            best_dist = dist;
                            best_line_sec = Some(*target);
                        }
                    }
                }
                s = best_start;
                e = s + len;
                drag.snap_lines_sec = [Some(s), Some(e)];
                if let Some(line_sec) = best_line_sec {
                    let at_start = line_sec == s;
                    let at_end = line_sec == e;
                    if !(at_start || at_end) {
                        drag.snap_lines_sec[1] = Some(line_sec);
                    }
                }
            }
            drag.candidate_start_sec = s;
            drag.candidate_end_sec = e;
        }
        DragMode::ResizeStart => {
            let mut s = (drag.orig_start_sec + delta_sec).clamp(0, drag.orig_end_sec - MIN_SECONDS);
            if snap_enabled {
                s = nearest_target(s, 0, drag.orig_end_sec - MIN_SECONDS);
                drag.snap_lines_sec = [Some(s), None];
            } else {
                drag.snap_lines_sec = [None, None];
            }
            drag.candidate_start_sec = s;
            drag.candidate_end_sec = drag.orig_end_sec;
        }
        DragMode::ResizeEnd => {
            let mut e = (drag.orig_end_sec + delta_sec)
                .clamp(drag.orig_start_sec + MIN_SECONDS, DAY_SECONDS);
            if snap_enabled {
                e = nearest_target(e, drag.orig_start_sec + MIN_SECONDS, DAY_SECONDS);
                drag.snap_lines_sec = [Some(e), None];
            } else {
                drag.snap_lines_sec = [None, None];
            }
            drag.candidate_start_sec = drag.orig_start_sec;
            drag.candidate_end_sec = e;
        }
    }
}

fn form_from_tracking(t: &db::Tracking, conn: &rusqlite::Connection) -> Option<TrackingForm> {
    if t.jira_synced != 0 {
        return None;
    }
    let start = parse_local_parts(&t.start_ts)?;
    let end_raw = t.end_ts.as_ref()?;
    let end = parse_local_parts(end_raw)?;
    let projects: Vec<String> = db::projects(conn)
        .ok()?
        .into_iter()
        .map(|p| p.name)
        .collect();
    let selected = projects
        .iter()
        .position(|p| p == &t.project_name)
        .unwrap_or(0);
    Some(TrackingForm {
        id: t.id,
        project_name: t.project_name.clone(),
        start_date: start.0,
        start_time: format_hhmm(start.1),
        end_date: end.0,
        end_time: format_hhmm(end.1),
        description: t.notes.clone().unwrap_or_default(),
        projects,
        selected_project: selected,
    })
}

fn save_form(
    conn: &rusqlite::Connection,
    selected_day: NaiveDate,
    form: &TrackingForm,
) -> Result<String, String> {
    if form.project_name.trim().is_empty() {
        return Err("project must not be empty".to_string());
    }
    let st = parse_hhmm_text(&form.start_time, "start")?;
    let et = parse_hhmm_text(&form.end_time, "end")?;
    let start = compose_local_ts(form.start_date, st)
        .ok_or_else(|| "invalid start timestamp".to_string())?;
    let end =
        compose_local_ts(form.end_date, et).ok_or_else(|| "invalid end timestamp".to_string())?;
    if end <= start {
        return Err("end must be after start".to_string());
    }
    if form.start_date != selected_day || form.end_date != selected_day {
        return Err("tracking must stay within selected day".to_string());
    }
    let start_s = crate::time::format_ts(&start);
    let end_s = crate::time::format_ts(&end);
    let overlap = db::has_overlap_for_day(
        conn,
        &selected_day.format("%Y-%m-%d").to_string(),
        Some(form.id),
        &start_s,
        &end_s,
    )
    .map_err(|e| e.to_string())?;
    if overlap {
        return Err("tracking overlaps existing entry".to_string());
    }
    let notes = if form.description.trim().is_empty() {
        None
    } else {
        Some(form.description.trim())
    };
    db::update_tracking_times(
        conn,
        form.id,
        form.project_name.trim(),
        &start_s,
        Some(&end_s),
        notes,
    )
    .map_err(|e| e.to_string())?;
    Ok("tracking updated".to_string())
}

fn sec_range_to_ts(day: NaiveDate, start_sec: i64, end_sec: i64) -> Option<(String, String)> {
    let base = compose_local_day_start(day)?;
    let s = base + Duration::seconds(start_sec);
    let e = base + Duration::seconds(end_sec);
    Some((crate::time::format_ts(&s), crate::time::format_ts(&e)))
}

fn sec_to_x(rect: egui::Rect, sec: i64) -> f32 {
    let t = (sec as f32 / DAY_SECONDS as f32).clamp(0.0, 1.0);
    rect.left() + t * rect.width()
}

fn sec_to_hhmm(sec: i64) -> String {
    let sec = sec.clamp(0, DAY_SECONDS);
    format!("{:02}:{:02}", sec / 3600, (sec % 3600) / 60)
}

fn compose_local_day_start(day: NaiveDate) -> Option<chrono::DateTime<chrono::Utc>> {
    crate::time::parse_local_ts(&format!("{} 00:00:00", day.format("%Y-%m-%d"))).ok()
}

fn parse_local_parts(raw: &str) -> Option<(NaiveDate, NaiveTime)> {
    let dt = crate::time::parse_local_ts(raw).ok()?.with_timezone(&Local);
    Some((dt.date_naive(), dt.time()))
}

fn compose_local_ts(date: NaiveDate, time: NaiveTime) -> Option<chrono::DateTime<chrono::Utc>> {
    let raw = format!(
        "{} {:02}:{:02}:00",
        date.format("%Y-%m-%d"),
        time.hour(),
        time.minute()
    );
    crate::time::parse_local_ts(&raw).ok()
}

fn parse_hhmm_text(raw: &str, label: &str) -> Result<NaiveTime, String> {
    let (h, m) = crate::config::parse_hhmm(raw)
        .map_err(|_| format!("invalid {} time; expected HH:mm", label))?;
    NaiveTime::from_hms_opt(h, m, 0)
        .ok_or_else(|| format!("invalid {} time; expected HH:mm", label))
}

fn format_hhmm(time: NaiveTime) -> String {
    time.format("%H:%M").to_string()
}

fn format_duration_hm(secs: i64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    format!("{}:{:02}", h, m)
}

fn default_working_hours_focus_window(config: &Config, day: NaiveDate) -> (i64, i64) {
    let weekday = day.weekday().num_days_from_monday() as u8;
    let Some(ranges) = config.working_hours.get(&weekday) else {
        return (0, DAY_SECONDS);
    };
    let mut earliest: Option<i64> = None;
    let mut latest: Option<i64> = None;
    for range in ranges {
        let Ok((sh, sm)) = crate::config::parse_hhmm(&range.start) else {
            continue;
        };
        let Ok((eh, em)) = crate::config::parse_hhmm(&range.end) else {
            continue;
        };
        let start = i64::from(sh * 60 + sm) * 60;
        let end = i64::from(eh * 60 + em) * 60;
        if end <= start {
            continue;
        }
        earliest = Some(earliest.map_or(start, |v| v.min(start)));
        latest = Some(latest.map_or(end, |v| v.max(end)));
    }
    let (earliest, latest) = match (earliest, latest) {
        (Some(s), Some(e)) => (s, e),
        _ => return (0, DAY_SECONDS),
    };
    let start = (earliest - 3600).clamp(0, DAY_SECONDS - 60);
    let end = (latest + 3600).clamp(start + 60, DAY_SECONDS);
    (start, end)
}

fn draw_active_drag_tooltip(
    ctx: &egui::Context,
    painter: &egui::Painter,
    chart_rect: egui::Rect,
    drag: &DragState,
) {
    let start_label = sec_to_hhmm(drag.candidate_start_sec);
    let end_label = sec_to_hhmm(drag.candidate_end_sec);
    let duration_secs = (drag.candidate_end_sec - drag.candidate_start_sec).max(0);
    let text = format!(
        "{}\n{} - {} ({})",
        drag.candidate_project_name,
        start_label,
        end_label,
        format_duration_hm(duration_secs)
    );
    let pointer = ctx.input(|i| i.pointer.latest_pos()).unwrap_or(egui::pos2(
        sec_to_x(
            chart_rect,
            (drag.candidate_start_sec + drag.candidate_end_sec) / 2,
        ),
        chart_rect.top() + 8.0,
    ));
    let galley = painter.layout(
        text,
        egui::FontId::proportional(12.0),
        egui::Color32::WHITE,
        320.0,
    );
    let padding = egui::vec2(8.0, 6.0);
    let mut tip_rect = egui::Rect::from_min_size(
        pointer + egui::vec2(14.0, 14.0),
        galley.size() + padding * 2.0,
    );
    if tip_rect.right() > chart_rect.right() {
        tip_rect = tip_rect.translate(egui::vec2(chart_rect.right() - tip_rect.right() - 4.0, 0.0));
    }
    if tip_rect.bottom() > chart_rect.bottom() {
        tip_rect = tip_rect.translate(egui::vec2(
            0.0,
            chart_rect.bottom() - tip_rect.bottom() - 4.0,
        ));
    }
    if tip_rect.left() < chart_rect.left() {
        tip_rect = tip_rect.translate(egui::vec2(chart_rect.left() - tip_rect.left() + 4.0, 0.0));
    }
    if tip_rect.top() < chart_rect.top() {
        tip_rect = tip_rect.translate(egui::vec2(0.0, chart_rect.top() - tip_rect.top() + 4.0));
    }
    painter.rect_filled(tip_rect, 6.0, egui::Color32::from_black_alpha(220));
    painter.rect_stroke(
        tip_rect,
        6.0,
        egui::Stroke::new(1.0, egui::Color32::from_white_alpha(80)),
        egui::StrokeKind::Outside,
    );
    painter.galley(tip_rect.min + padding, galley, egui::Color32::WHITE);
}

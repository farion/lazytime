use anyhow::Result;
use chrono::Local;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Borders, Padding};

use crate::db;
use crate::tui::trackings::TrackingsState;
use crate::tui::trackings_modal::{
    DatePicker, FilterModal, ModalMode, TrackingModal, TrackingsModal, normalize_storage_ts,
};
use crate::tui::trackings_rows::extract_date;
use chrono::{Datelike, Duration, NaiveDate};

fn center_line(text: &str, width: u16) -> String {
    let w = width as usize;
    let len = text.chars().count();
    if w <= len {
        return text.to_string();
    }
    let left = (w - len) / 2;
    format!("{}{}", " ".repeat(left), text)
}

pub fn render_modal(frame: &mut Frame<'_>, area: Rect, modal: &TrackingsModal) {
    let mw = match modal {
        TrackingsModal::Filter(_) => area
            .width
            .saturating_mul(3)
            .saturating_div(4)
            .min(96)
            .max(56),
        _ => (area.width / 2).min(80).max(40),
    };
    let desired_mh = match modal {
        // Tracking modal now has an extra "Desc" line, so increase
        // desired height to accomodate the additional content line.
        // Increase height to fit Project, Start, End, Desc, Sync, blank, OK/CANCEL
        TrackingsModal::Tracking(_) => 10u16,
        TrackingsModal::Confirm(_) => 6u16,
        TrackingsModal::Filter(_) => 7u16,
    };
    let mh = desired_mh.min(area.height.saturating_sub(1).max(3));
    let modal_area = Rect {
        x: area.x + (area.width.saturating_sub(mw)) / 2,
        y: area.y + (area.height.saturating_sub(mh)) / 2,
        width: mw,
        height: mh,
    };

    let (title, text) = match modal {
        TrackingsModal::Tracking(m) => {
            let start_value = if m.field_idx == 1 {
                m.date_picker
                    .as_ref()
                    .map(DatePicker::format_with_focus)
                    .unwrap_or_else(|| m.start.clone())
            } else {
                m.start.clone()
            };
            let end_value = if m.field_idx == 2 {
                m.date_picker
                    .as_ref()
                    .map(DatePicker::format_with_focus)
                    .unwrap_or_else(|| m.end.clone())
            } else {
                m.end.clone()
            };
            let proj_line = if !m.projects.is_empty() {
                let name = m
                    .selected_project
                    .and_then(|i| m.projects.get(i))
                    .cloned()
                    .unwrap_or_default();
                format!(
                    "Project: {}{} (<-/->)",
                    if m.field_idx == 0 { "> " } else { "  " },
                    name
                )
            } else {
                format!(
                    "Project: {}(no projects in DB)",
                    if m.field_idx == 0 { "> " } else { "  " }
                )
            };
            let rows = vec![
                proj_line,
                format!(
                    "Start  : {}{}",
                    if m.field_idx == 1 { "> " } else { "  " },
                    start_value
                ),
                format!(
                    "End    : {}{}",
                    if m.field_idx == 2 { "> " } else { "  " },
                    end_value
                ),
                format!(
                    "Desc   : {}{}",
                    if m.field_idx == 3 { "> " } else { "  " },
                    m.description
                ),
                // Sync 0/1
                format!(
                    "Sync   : {}{}",
                    if m.field_idx == 4 { "> " } else { "  " },
                    if m.synced { "1" } else { "0" }
                ),
                "".to_string(),
                format!(
                    "{}OK   {}CANCEL",
                    if m.field_idx == 5 { "> " } else { "  " },
                    if m.field_idx == 6 { "> " } else { "  " }
                ),
            ];
            let title = if matches!(m.mode, ModalMode::Add) {
                "Add tracking"
            } else {
                "Edit tracking"
            };
            (title.to_string(), rows.join("\n"))
        }
        TrackingsModal::Confirm(m) => {
            let rows = vec![
                m.message.clone(),
                "".to_string(),
                format!(
                    "{}YES   {}NO",
                    if m.field_idx == 0 { "> " } else { "  " },
                    if m.field_idx == 1 { "> " } else { "  " }
                ),
            ];
            (m.title.clone(), rows.join("\n"))
        }
        TrackingsModal::Filter(m) => {
            let start_value = if m.field_idx == 0 {
                m.date_picker
                    .as_ref()
                    .map(DatePicker::format_with_focus)
                    .unwrap_or_else(|| m.start.clone())
            } else {
                m.start.clone()
            };
            let end_value = if m.field_idx == 1 {
                m.date_picker
                    .as_ref()
                    .map(DatePicker::format_with_focus)
                    .unwrap_or_else(|| m.end.clone())
            } else {
                m.end.clone()
            };
            let buttons = format!(
                "{}OK   {}CANCEL   {}TODAY",
                if m.field_idx == 2 { "> " } else { "  " },
                if m.field_idx == 3 { "> " } else { "  " },
                if m.field_idx == 4 { "> " } else { "  " }
            );
            let centered_buttons = center_line(&buttons, mw.saturating_sub(6));
            let rows = vec![
                format!(
                    "Start: {}{}",
                    if m.field_idx == 0 { "> " } else { "  " },
                    start_value
                ),
                format!(
                    "End  : {}{}",
                    if m.field_idx == 1 { "> " } else { "  " },
                    end_value
                ),
                "".to_string(),
                centered_buttons,
            ];
            ("Filter trackings".to_string(), rows.join("\n"))
        }
    };

    frame.render_widget(ratatui::widgets::Clear, modal_area);
    frame.render_widget(
        ratatui::widgets::Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} ", title))
                .padding(Padding::horizontal(1)),
        ),
        modal_area,
    );
}

pub fn handle_modal_key(
    state: &mut TrackingsState,
    key: KeyEvent,
    mut modal: TrackingsModal,
    conn: &rusqlite::Connection,
) -> Result<(Option<TrackingsModal>, bool)> {
    use KeyCode::*;
    match &mut modal {
        TrackingsModal::Confirm(m) => match key.code {
            Left | Right | Tab | Up | Down => {
                m.field_idx = 1usize.saturating_sub(m.field_idx);
                Ok((Some(modal), false))
            }
            Enter => {
                if m.field_idx == 0 {
                    db::delete_tracking(conn, m.tracking_id)?;
                    state.selected = state.selected.saturating_sub(1);
                    state.message = "tracking deleted".to_string();
                } else {
                    state.message = "cancelled".to_string();
                }
                Ok((None, true))
            }
            Esc => {
                state.message = "cancelled".to_string();
                Ok((None, true))
            }
            _ => Ok((Some(modal), false)),
        },
        TrackingsModal::Filter(m) => handle_filter_modal_key(state, key.code, m),
        TrackingsModal::Tracking(m) => handle_tracking_modal_key(state, key.code, m, conn),
    }
}

fn add_months_to_dp(dp: &mut DatePicker, delta: i64) {
    // convert to total months since year 0
    let total = (dp.year as i64) * 12 + (dp.month as i64 - 1) + delta;
    let new_year = (total / 12) as i32;
    let mut new_month = (total % 12) as i32;
    if new_month < 0 {
        new_month += 12;
    }
    let new_month_u = (new_month + 1) as u32;
    // clamp day to last day of new month if needed
    let mut new_day = dp.day;
    while NaiveDate::from_ymd_opt(new_year, new_month_u, new_day).is_none() {
        if new_day == 1 {
            break;
        }
        new_day = new_day.saturating_sub(1);
    }
    dp.year = new_year;
    dp.month = new_month_u;
    dp.day = new_day;
}

fn add_days_to_dp(dp: &mut DatePicker, delta_days: i64) {
    if let Some(d) = NaiveDate::from_ymd_opt(dp.year, dp.month, dp.day) {
        let nd = d + Duration::days(delta_days);
        dp.year = nd.year();
        dp.month = nd.month();
        dp.day = nd.day();
    }
}

fn big_inc_dp(dp: &mut DatePicker) {
    match dp.sel {
        0 => dp.year += 5,
        1 => add_months_to_dp(dp, 5),
        2 => add_days_to_dp(dp, 7),
        3 => dp.hour = (dp.hour + 5) % 24,
        4 => {
            // align to next 15-minute boundary or add 15 if already aligned
            let m = dp.minute as i32;
            if m % 15 == 0 {
                let mut nm = m + 15;
                if nm >= 60 {
                    nm -= 60;
                    dp.hour = (dp.hour + 1) % 24;
                }
                dp.minute = nm as u32;
            } else {
                let mut nm = ((m + 14) / 15) * 15; // ceil to next multiple
                if nm >= 60 {
                    nm -= 60;
                    dp.hour = (dp.hour + 1) % 24;
                }
                dp.minute = nm as u32;
            }
        }
        _ => {}
    }
}

fn big_dec_dp(dp: &mut DatePicker) {
    match dp.sel {
        0 => dp.year -= 5,
        1 => add_months_to_dp(dp, -5),
        2 => add_days_to_dp(dp, -7),
        3 => {
            dp.hour = if dp.hour < 5 {
                (24 + dp.hour).saturating_sub(5)
            } else {
                dp.hour - 5
            }
        }
        4 => {
            // align to previous 15-minute boundary or subtract 15 if already aligned
            let m = dp.minute as i32;
            if m % 15 == 0 {
                let mut nm = m - 15;
                if nm < 0 {
                    nm += 60;
                    dp.hour = if dp.hour == 0 { 23 } else { dp.hour - 1 };
                }
                dp.minute = nm as u32;
            } else {
                let mut nm = (m / 15) * 15; // floor to previous multiple
                if nm < 0 {
                    nm += 60;
                    dp.hour = if dp.hour == 0 { 23 } else { dp.hour - 1 };
                }
                dp.minute = nm as u32;
            }
        }
        _ => {}
    }
}

fn handle_filter_modal_key(
    state: &mut TrackingsState,
    code: KeyCode,
    m: &mut FilterModal,
) -> Result<(Option<TrackingsModal>, bool)> {
    use KeyCode::*;
    if m.date_picker.is_some() {
        return Ok((handle_date_picker_for_filter(code, m), true));
    }
    match code {
        Left => {
            // When a date field is selected but not in edit mode, Left moves the
            // selected date one day back.
            if (m.field_idx == 0 || m.field_idx == 1) && m.date_picker.is_none() {
                let src = if m.field_idx == 0 { &m.start } else { &m.end };
                let mut dp = DatePicker::from_str(src);
                add_days_to_dp(&mut dp, -1);
                if m.field_idx == 0 {
                    m.start = dp.format();
                } else {
                    m.end = dp.format();
                }
                return Ok((Some(TrackingsModal::Filter(m.clone())), true));
            }
            Ok((Some(TrackingsModal::Filter(m.clone())), false))
        }
        Right => {
            // When a date field is selected but not in edit mode, Right moves the
            // selected date one day forward.
            if (m.field_idx == 0 || m.field_idx == 1) && m.date_picker.is_none() {
                let src = if m.field_idx == 0 { &m.start } else { &m.end };
                let mut dp = DatePicker::from_str(src);
                add_days_to_dp(&mut dp, 1);
                if m.field_idx == 0 {
                    m.start = dp.format();
                } else {
                    m.end = dp.format();
                }
                return Ok((Some(TrackingsModal::Filter(m.clone())), true));
            }
            Ok((Some(TrackingsModal::Filter(m.clone())), false))
        }
        Char(_c) => {
            // manual typing disabled for date fields in filter modal; use Enter to edit
            Ok((Some(TrackingsModal::Filter(m.clone())), false))
        }
        Backspace => {
            if m.field_idx == 0 {
                m.start.clear();
            } else if m.field_idx == 1 {
                m.end.clear();
            }
            Ok((Some(TrackingsModal::Filter(m.clone())), true))
        }
        Tab | Down => {
            // cycle over Start, End, OK, CANCEL, TODAY
            m.field_idx = (m.field_idx + 1) % 5;
            Ok((Some(TrackingsModal::Filter(m.clone())), false))
        }
        Up => {
            m.field_idx = if m.field_idx == 0 { 4 } else { m.field_idx - 1 };
            Ok((Some(TrackingsModal::Filter(m.clone())), false))
        }
        Enter => {
            if m.field_idx <= 1 {
                let src = if m.field_idx == 0 { &m.start } else { &m.end };
                m.date_picker = Some(DatePicker::from_str(src));
                return Ok((Some(TrackingsModal::Filter(m.clone())), false));
            }
            if m.field_idx == 2 {
                // OK — apply filter from provided start/end fields
                let start = extract_date(&m.start)
                    .unwrap_or_else(|| Local::now().date_naive().format("%Y-%m-%d").to_string());
                let end = extract_date(&m.end).unwrap_or_else(|| start.clone());
                if start <= end {
                    state.filter_start = start;
                    state.filter_end = end;
                } else {
                    state.filter_start = end;
                    state.filter_end = start;
                }
                state.selected = 0;
                state.offset = 0;
                state.message = format!("filter set: {}..{}", state.filter_start, state.filter_end);
                return Ok((None, true));
            }
            if m.field_idx == 3 {
                // CANCEL
                state.message = "cancelled".to_string();
                return Ok((None, true));
            }
            // TODAY selected: set filter to today
            if m.field_idx == 5 {
                let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
                state.filter_start = today.clone();
                state.filter_end = today;
                state.selected = 0;
                state.offset = 0;
                state.message = format!("filter set: {}..{}", state.filter_start, state.filter_end);
                return Ok((None, true));
            }
            Ok((None, true))
        }
        Esc => {
            state.message = "cancelled".to_string();
            Ok((None, true))
        }
        _ => Ok((Some(TrackingsModal::Filter(m.clone())), false)),
    }
}

fn handle_date_picker_for_filter(code: KeyCode, m: &mut FilterModal) -> Option<TrackingsModal> {
    use KeyCode::*;
    match code {
        Left => {
            if let Some(dp) = &mut m.date_picker {
                dp.sel = dp.sel.saturating_sub(1);
            }
        }
        Right | Tab => {
            if let Some(dp) = &mut m.date_picker {
                dp.sel = (dp.sel + 1).min(4);
            }
        }
        Up => {
            if let Some(dp) = &mut m.date_picker {
                dp.inc();
            }
        }
        PageUp => {
            if let Some(dp) = &mut m.date_picker {
                big_inc_dp(dp);
            }
        }
        Down => {
            if let Some(dp) = &mut m.date_picker {
                dp.dec();
            }
        }
        PageDown => {
            if let Some(dp) = &mut m.date_picker {
                big_dec_dp(dp);
            }
        }
        Enter => {
            if let Some(dp) = m.date_picker.take() {
                if m.field_idx == 0 {
                    m.start = dp.format();
                } else {
                    m.end = dp.format();
                }
            }
        }
        Backspace => {
            if m.field_idx == 0 {
                m.start.clear();
            } else {
                m.end.clear();
            }
            m.date_picker = None;
        }
        Esc => {
            // cancel editing without applying changes
            m.date_picker = None;
        }
        _ => {}
    }
    Some(TrackingsModal::Filter(m.clone()))
}

fn handle_tracking_modal_key(
    state: &mut TrackingsState,
    code: KeyCode,
    m: &mut TrackingModal,
    conn: &rusqlite::Connection,
) -> Result<(Option<TrackingsModal>, bool)> {
    use KeyCode::*;
    if m.date_picker.is_some() {
        handle_date_picker_for_tracking(code, m);
        return Ok((Some(TrackingsModal::Tracking(m.clone())), true));
    }
    // allow toggling sync via Left/Right when on the sync field
    if m.field_idx == 4 {
        use KeyCode::*;
        match code {
            Left | Right => {
                m.synced = !m.synced;
                return Ok((Some(TrackingsModal::Tracking(m.clone())), true));
            }
            _ => {}
        }
    }
    match code {
        Char(c) => {
            // allow typing into project free text (field_idx == 0) and
            // description (field_idx == 3). Date fields are edited via date picker.
            if m.field_idx == 0 {
                m.project_free_text.push(c);
            } else if m.field_idx == 3 {
                m.description.push(c);
            } else if m.field_idx == 4 {
                // accept 0/1 for sync
                if c == '1' {
                    m.synced = true;
                } else if c == '0' {
                    m.synced = false;
                }
            }
        }
        Backspace => {
            if m.field_idx == 1 {
                m.start.clear();
            } else if m.field_idx == 2 {
                m.end.clear();
            } else if m.field_idx == 3 {
                m.description.pop();
            }
        }
        Tab | Down => m.field_idx = (m.field_idx + 1) % 7,
        Up => m.field_idx = if m.field_idx == 0 { 6 } else { m.field_idx - 1 },
        Left => {
            // If a date field is selected and not in edit mode, move it one day back.
            if (m.field_idx == 1 || m.field_idx == 2) && m.date_picker.is_none() {
                let src = if m.field_idx == 1 { &m.start } else { &m.end };
                let mut dp = DatePicker::from_str(src);
                add_days_to_dp(&mut dp, -1);
                if m.field_idx == 1 {
                    m.start = dp.format();
                } else {
                    m.end = dp.format();
                }
                return Ok((Some(TrackingsModal::Tracking(m.clone())), true));
            }
            // Otherwise, treat as project-selection left when on project field
            if m.field_idx == 0 && !m.projects.is_empty() {
                m.selected_project = Some(match m.selected_project {
                    Some(0) | None => m.projects.len() - 1,
                    Some(sel) => sel.saturating_sub(1),
                });
            }
        }
        Right => {
            // If a date field is selected and not in edit mode, move it one day forward.
            if (m.field_idx == 1 || m.field_idx == 2) && m.date_picker.is_none() {
                let src = if m.field_idx == 1 { &m.start } else { &m.end };
                let mut dp = DatePicker::from_str(src);
                add_days_to_dp(&mut dp, 1);
                if m.field_idx == 1 {
                    m.start = dp.format();
                } else {
                    m.end = dp.format();
                }
                return Ok((Some(TrackingsModal::Tracking(m.clone())), true));
            }
            // Otherwise, treat as project-selection right when on project field
            if m.field_idx == 0 && !m.projects.is_empty() {
                m.selected_project = Some(match m.selected_project {
                    Some(sel) => (sel + 1) % m.projects.len(),
                    None => 0,
                });
            }
        }
        Enter => {
            if m.field_idx == 1 || m.field_idx == 2 {
                let src = if m.field_idx == 1 { &m.start } else { &m.end };
                m.date_picker = Some(DatePicker::from_str(src));
                return Ok((Some(TrackingsModal::Tracking(m.clone())), false));
            }
            if m.field_idx == 5 {
                // OK selected
                let project_name = m.selected_project.and_then(|i| m.projects.get(i)).cloned();
                let Some(project_name) = project_name else {
                    state.message = "no projects defined; add a project first".to_string();
                    return Ok((None, true));
                };
                let start_storage = normalize_storage_ts(&m.start);
                let end_storage = if m.end.trim().is_empty() {
                    None
                } else {
                    Some(normalize_storage_ts(&m.end))
                };
                if matches!(m.mode, ModalMode::Add) {
                    db::add_manual_tracking(
                        conn,
                        &project_name,
                        &start_storage,
                        end_storage.as_deref(),
                        Some(&m.description),
                    )?;
                    state.message = "tracking added".to_string();
                } else if let Some(id) = m.editing_id {
                    db::update_tracking_times(
                        conn,
                        id,
                        &project_name,
                        &start_storage,
                        end_storage.as_deref(),
                        Some(&m.description),
                    )?;
                    // update jira_synced flag to match modal
                    db::set_tracking_synced(conn, id, if m.synced { 1 } else { 0 })?;
                    state.message = "tracking updated".to_string();
                }
                return Ok((None, true));
            }
            if m.field_idx == 6 {
                // CANCEL selected
                state.message = "cancelled".to_string();
                return Ok((None, true));
            }
            if m.field_idx == 3 {
                // editing description; Enter should not close modal
                return Ok((Some(TrackingsModal::Tracking(m.clone())), false));
            }
        }
        Esc => {
            state.message = "cancelled".to_string();
            return Ok((None, true));
        }
        _ => {}
    }
    Ok((Some(TrackingsModal::Tracking(m.clone())), false))
}

fn handle_date_picker_for_tracking(code: KeyCode, m: &mut TrackingModal) {
    use KeyCode::*;
    match code {
        Left => {
            if let Some(dp) = &mut m.date_picker {
                dp.sel = dp.sel.saturating_sub(1);
            }
        }
        Right | Tab => {
            if let Some(dp) = &mut m.date_picker {
                dp.sel = (dp.sel + 1).min(4);
            }
        }
        Up => {
            if let Some(dp) = &mut m.date_picker {
                dp.inc();
            }
            // live preview while editing; do not commit until Enter
        }
        PageUp => {
            if let Some(dp) = &mut m.date_picker {
                big_inc_dp(dp);
            }
        }
        Down => {
            if let Some(dp) = &mut m.date_picker {
                dp.dec();
            }
            // live preview while editing; do not commit until Enter
        }
        PageDown => {
            if let Some(dp) = &mut m.date_picker {
                big_dec_dp(dp);
            }
        }
        Enter => {
            // Commit changes and exit edit mode for this field
            if let Some(dp) = m.date_picker.take() {
                if m.field_idx == 1 {
                    m.start = dp.format();
                } else if m.field_idx == 2 {
                    m.end = dp.format();
                }
            }
        }
        Backspace => {
            // clear the field and exit edit mode
            if m.field_idx == 1 {
                m.start.clear();
            } else if m.field_idx == 2 {
                m.end.clear();
            }
            m.date_picker = None;
        }
        Esc => {
            // cancel editing without applying changes
            m.date_picker = None;
        }
        _ => {}
    }
}

use anyhow::Result;
use chrono::{Duration, Local, NaiveDate, Utc};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Padding, Row, Table};

use crate::config::Config;
use crate::db;
use crate::tui::trackings_cleanup::cleanup_today_unsynced_trackings;
use crate::tui::trackings_modal::{
    ConfirmModal, FilterModal, TrackingModal, TrackingsModal, format_storage_ts_for_tui,
};
use crate::tui::trackings_modal_actions::{handle_modal_key, render_modal};
use crate::tui::trackings_rows::{DisplayRow, display_rows};
use crate::tui::trackings_storno::storno_tracking;

fn format_duration_from_ts(start_raw: &str, end_raw_opt: Option<&String>) -> String {
    // If end is missing, compute duration from start until now (open tracking)
    let start_dt = match crate::time::parse_local_ts(start_raw) {
        Ok(dt) => dt,
        Err(_) => return String::new(),
    };
    let secs = if let Some(end_raw) = end_raw_opt {
        match crate::time::parse_local_ts(end_raw) {
            Ok(end_dt) => end_dt.signed_duration_since(start_dt).num_seconds(),
            Err(_) => return String::new(),
        }
    } else {
        // open tracking -> duration until now
        let now = Utc::now();
        now.signed_duration_since(start_dt).num_seconds()
    };
    if secs <= 0 {
        return "0:00".to_string();
    }
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    format!("{}:{:02}", hours, mins)
}

fn format_duration_between(start_raw: &str, end_raw: &str) -> String {
    let start_dt = match crate::time::parse_local_ts(start_raw) {
        Ok(dt) => dt,
        Err(_) => return String::new(),
    };
    let end_dt = match crate::time::parse_local_ts(end_raw) {
        Ok(dt) => dt,
        Err(_) => return String::new(),
    };
    let secs = end_dt.signed_duration_since(start_dt).num_seconds();
    if secs <= 0 {
        return "0:00".to_string();
    }
    let hours = secs / 3600;
    let mins = (secs % 3600) / 60;
    format!("{}:{:02}", hours, mins)
}

#[derive(Debug, Clone)]
pub struct TrackingsState {
    pub selected: usize,
    pub message: String,
    pub modal: Option<TrackingsModal>,
    pub offset: usize,
    pub show_gaps: bool,
    pub filter_start: String,
    pub filter_end: String,
    pub visible_rows: usize,
}

impl Default for TrackingsState {
    fn default() -> Self {
        let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
        Self {
            selected: 0,
            message: String::new(),
            modal: None,
            offset: 0,
            show_gaps: true,
            filter_start: today.clone(),
            filter_end: today,
            visible_rows: 1,
        }
    }
}

impl TrackingsState {
    pub fn render(
        &mut self,
        frame: &mut Frame<'_>,
        area: Rect,
        conn: &rusqlite::Connection,
        config: &Config,
    ) {
        let rows_data = display_rows(
            conn,
            config,
            &self.filter_start,
            &self.filter_end,
            self.show_gaps,
        )
        .unwrap_or_default();
        // Determine whether the filter is a single day (YYYY-MM-DD == YYYY-MM-DD).
        let single_day_filter = match (
            chrono::NaiveDate::parse_from_str(self.filter_start.trim(), "%Y-%m-%d"),
            chrono::NaiveDate::parse_from_str(self.filter_end.trim(), "%Y-%m-%d"),
        ) {
            (Ok(s), Ok(e)) => s == e,
            _ => false,
        };
        let selected = self.selected.min(rows_data.len().saturating_sub(1));
        let rows_all: Vec<_> = rows_data
            .iter()
            .enumerate()
            .map(|(idx, row)| {
                // Selected row keeps the selection color. Non-selected gap rows
                // should be rendered darker than normal rows.
                let style = if idx == selected {
                    Style::default().fg(Color::Yellow)
                } else {
                    match row {
                        DisplayRow::Gap(_) => Style::default().fg(Color::DarkGray),
                        DisplayRow::Tracking(t) if t.jira_synced != 0 => {
                            Style::default().fg(Color::Gray)
                        }
                        _ => Style::default(),
                    }
                };
                match row {
                    DisplayRow::Tracking(t) => {
                        // When the filter covers exactly one day, show times only.
                        let start_formatted = if single_day_filter {
                            match crate::time::parse_ts(&t.start_ts) {
                                Ok(dt) => dt.with_timezone(&Local).format("%H:%M").to_string(),
                                Err(_) => format_storage_ts_for_tui(&t.start_ts),
                            }
                        } else {
                            format_storage_ts_for_tui(&t.start_ts)
                        };

                        let end_formatted = if let Some(end_raw) = t.end_ts.as_ref() {
                            if single_day_filter {
                                match crate::time::parse_ts(end_raw) {
                                    Ok(dt) => dt.with_timezone(&Local).format("%H:%M").to_string(),
                                    Err(_) => format_storage_ts_for_tui(end_raw),
                                }
                            } else {
                                format_storage_ts_for_tui(end_raw)
                            }
                        } else {
                            "(open)".to_string()
                        };
                        let duration = format_duration_from_ts(&t.start_ts, t.end_ts.as_ref());
                        // right-align duration into 8 chars column
                        let duration_cell = format!("{:>8}", duration);
                        let desc = t.notes.clone().unwrap_or_default();
                        let desc_col = if desc.is_empty() {
                            String::new()
                        } else if desc.chars().count() > 15 {
                            format!("{}…", desc.chars().take(15).collect::<String>())
                        } else {
                            desc.clone()
                        };
                        Row::new(vec![
                            Cell::from(t.project_name.clone()),
                            Cell::from(start_formatted),
                            Cell::from(end_formatted),
                            Cell::from(duration_cell),
                            Cell::from(desc_col),
                            // Show 0 or 1 for jira_synced
                            Cell::from(format!("{}", if t.jira_synced != 0 { 1 } else { 0 })),
                            Cell::from(t.created_by.clone()),
                        ])
                        .style(style)
                    }
                    DisplayRow::Gap(g) => {
                        let start = if single_day_filter {
                            match crate::time::parse_ts(&g.start_ts) {
                                Ok(dt) => dt.with_timezone(&Local).format("%H:%M").to_string(),
                                Err(_) => format_storage_ts_for_tui(&g.start_ts),
                            }
                        } else {
                            format_storage_ts_for_tui(&g.start_ts)
                        };
                        let end = if single_day_filter {
                            match crate::time::parse_ts(&g.end_ts) {
                                Ok(dt) => dt.with_timezone(&Local).format("%H:%M").to_string(),
                                Err(_) => format_storage_ts_for_tui(&g.end_ts),
                            }
                        } else {
                            format_storage_ts_for_tui(&g.end_ts)
                        };
                        let duration = format_duration_between(&g.start_ts, &g.end_ts);
                        let duration_cell = format!("{:>8}", duration);
                        Row::new(vec![
                            Cell::from(""),
                            Cell::from(start),
                            Cell::from(end),
                            Cell::from(duration_cell),
                            Cell::from(String::new()),
                            Cell::from("0"),
                            Cell::from(""),
                        ])
                        .style(style)
                    }
                    DisplayRow::Separator => Row::new(vec![
                        Cell::from(""),
                        Cell::from(""),
                        Cell::from(""),
                        Cell::from(""),
                        Cell::from(""),
                        Cell::from(""),
                        Cell::from(""),
                    ])
                    .style(style),
                }
            })
            .collect();

        let title_height: u16 = 3;
        // Reserve one extra line at the bottom for the status/message bar so it
        // doesn't overlap the table. Title sits at the top, table in the middle,
        // and the footer (status) occupies the final line.
        let content_area = Rect {
            x: area.x,
            y: area.y + title_height,
            width: area.width,
            height: area
                .height
                .saturating_sub(title_height) // remove title
                .saturating_sub(1), // reserve one line for footer
        };
        let visible = crate::tui::table_visible_rows(content_area.height);
        self.visible_rows = visible.max(1);
        let len = rows_all.len();
        let start = crate::tui::scroll_offset_for_selection(
            selected,
            self.offset,
            len,
            self.visible_rows,
            2,
        );
        let end = (start + visible).min(len);
        let rows = rows_all[start..end].iter().cloned();

        let table = Table::new(
            rows,
            [
                Constraint::Length(24),
                Constraint::Length(12),
                Constraint::Length(12),
                Constraint::Length(8),
                Constraint::Length(15),
                // small sync column
                Constraint::Length(6),
                Constraint::Length(10),
            ],
        )
        .header(Row::new(vec![
            Cell::from("Project ").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Start ").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("End ").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from(format!("{:>8}", "Duration"))
                .style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Description ").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("Sync ").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("By ").style(Style::default().add_modifier(Modifier::BOLD)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .padding(Padding::horizontal(1)),
        );

        let left = if single_day_filter {
            format!(" TRACKINGS [{}]", self.filter_start)
        } else {
            format!(" TRACKINGS [{}..{}]", self.filter_start, self.filter_end)
        };
        let hints = format!(
            "a=add | e=edit | d=delete | f=filter | g=gaps:{} | l=cleanup | s=storno ",
            if self.show_gaps { "on" } else { "off" }
        );
        let inner_width = area.width.saturating_sub(2) as usize;
        let left_len = left.chars().count();
        let hints_len = hints.chars().count();
        let gap = if inner_width > left_len + hints_len {
            inner_width - left_len - hints_len
        } else {
            1
        };
        let mut title_line = format!("{}{}{}", left, " ".repeat(gap), hints);
        title_line = title_line.trim_end_matches(' ').to_string();
        title_line.push(' ');

        let title_block = Block::default()
            .borders(Borders::ALL)
            .padding(Padding::horizontal(0));
        frame.render_widget(
            ratatui::widgets::Paragraph::new(title_line).block(title_block),
            Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: title_height,
            },
        );
        let tbl_block = Block::default()
            .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
            .padding(Padding::horizontal(1));
        frame.render_widget(table.block(tbl_block), content_area);

        if let Some(modal) = &self.modal {
            render_modal(frame, area, modal);
        }
    }

    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        conn: &rusqlite::Connection,
        config: &Config,
    ) -> Result<bool> {
        let rows = display_rows(
            conn,
            config,
            &self.filter_start,
            &self.filter_end,
            self.show_gaps,
        )?;
        if rows.is_empty() {
            self.selected = 0;
        } else {
            self.selected = self.selected.min(rows.len() - 1);
        }

        if self.modal.is_some() {
            let modal = self.modal.take().expect("modal present");
            let (next, changed) = handle_modal_key(self, key, modal, conn)?;
            self.modal = next;
            return Ok(changed);
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if !rows.is_empty() {
                    self.selected = (self.selected + 1).min(rows.len() - 1);
                    self.offset = crate::tui::scroll_offset_for_selection(
                        self.selected,
                        self.offset,
                        rows.len(),
                        self.visible_rows,
                        2,
                    );
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                self.offset = crate::tui::scroll_offset_for_selection(
                    self.selected,
                    self.offset,
                    rows.len(),
                    self.visible_rows,
                    2,
                );
            }
            KeyCode::Left => {
                // shift filter window left: if single day, move by 1 day;
                // if range, move to the previous block of the same length
                let parse_day = |s: &str| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok();
                let today = Local::now().date_naive();
                let sday = parse_day(&self.filter_start).unwrap_or(today);
                let eday = parse_day(&self.filter_end).unwrap_or(sday);
                let (from, to) = if sday <= eday {
                    (sday, eday)
                } else {
                    (eday, sday)
                };
                let len = (to - from).num_days() + 1;
                let new_from = from + Duration::days(-len);
                let new_to = from + Duration::days(-1);
                self.filter_start = new_from.format("%Y-%m-%d").to_string();
                self.filter_end = new_to.format("%Y-%m-%d").to_string();
                self.selected = 0;
                self.offset = 0;
                self.message = format!("filter set: {}..{}", self.filter_start, self.filter_end);
                return Ok(true);
            }
            KeyCode::Right => {
                // shift filter window right
                let parse_day = |s: &str| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok();
                let today = Local::now().date_naive();
                let sday = parse_day(&self.filter_start).unwrap_or(today);
                let eday = parse_day(&self.filter_end).unwrap_or(sday);
                let (from, to) = if sday <= eday {
                    (sday, eday)
                } else {
                    (eday, sday)
                };
                let len = (to - from).num_days() + 1;
                let new_from = to + Duration::days(1);
                let new_to = to + Duration::days(len);
                self.filter_start = new_from.format("%Y-%m-%d").to_string();
                self.filter_end = new_to.format("%Y-%m-%d").to_string();
                self.selected = 0;
                self.offset = 0;
                self.message = format!("filter set: {}..{}", self.filter_start, self.filter_end);
                return Ok(true);
            }
            KeyCode::Char('f') => {
                self.modal = Some(TrackingsModal::Filter(FilterModal::new(
                    self.filter_start.clone(),
                    self.filter_end.clone(),
                )));
                return Ok(true);
            }
            KeyCode::Char('g') => {
                self.show_gaps = !self.show_gaps;
                self.message = if self.show_gaps {
                    "gaps shown".to_string()
                } else {
                    "gaps hidden".to_string()
                };
                return Ok(true);
            }
            KeyCode::Char('l') => {
                let stats = cleanup_today_unsynced_trackings(conn, config)?;
                if stats.removed_rows == 0 {
                    self.message = "cleanup: nothing to merge".to_string();
                } else {
                    self.message = format!(
                        "cleanup: merged {} groups, removed {} rows",
                        stats.merged_groups, stats.removed_rows
                    );
                }
                self.selected = 0;
                self.offset = 0;
                return Ok(true);
            }
            KeyCode::Char('s') => {
                if let Some(DisplayRow::Tracking(t)) = rows.get(self.selected) {
                    self.message = match storno_tracking(conn, config, t) {
                        Ok(msg) => msg,
                        Err(err) => format!("error: {}", err),
                    };
                    return Ok(true);
                }
            }
            KeyCode::Char('a') => {
                if let Some(DisplayRow::Tracking(t)) = rows.get(self.selected)
                    && t.jira_synced != 0
                {
                    self.message = "readonly: synced tracking cannot be changed".to_string();
                    return Ok(true);
                }
                let projects: Vec<String> =
                    db::projects(conn)?.into_iter().map(|p| p.name).collect();
                let mut modal = TrackingModal::new_add_with_projects(projects.clone());
                if let Some(DisplayRow::Gap(g)) = rows.get(self.selected) {
                    modal.start = format_storage_ts_for_tui(&g.start_ts);
                    modal.end = format_storage_ts_for_tui(&g.end_ts);
                    modal.selected_project = projects.iter().position(|p| p == &g.previous_project);
                }
                self.modal = Some(TrackingsModal::Tracking(modal));
                return Ok(true);
            }
            KeyCode::Char('e') => {
                if let Some(DisplayRow::Tracking(t)) = rows.get(self.selected) {
                    if t.jira_synced != 0 {
                        self.message = "readonly: synced tracking cannot be changed".to_string();
                        return Ok(true);
                    }
                    let projects = db::projects(conn)?.into_iter().map(|p| p.name).collect();
                    let mut modal = TrackingModal::new_edit_with_projects(
                        t.id,
                        t.project_name.clone(),
                        t.start_ts.clone(),
                        t.end_ts.clone().unwrap_or_default(),
                        projects,
                    );
                    modal.description = t.notes.clone().unwrap_or_default();
                    modal.synced = t.jira_synced != 0;
                    self.modal = Some(TrackingsModal::Tracking(modal));
                    return Ok(true);
                }
            }
            KeyCode::Char('d') => {
                if let Some(DisplayRow::Tracking(t)) = rows.get(self.selected) {
                    if t.jira_synced != 0 {
                        self.message = "readonly: synced tracking cannot be changed".to_string();
                        return Ok(true);
                    }
                    self.modal = Some(TrackingsModal::Confirm(ConfirmModal::delete_tracking(t.id)));
                    return Ok(true);
                }
            }
            _ => {}
        }
        Ok(false)
    }
}

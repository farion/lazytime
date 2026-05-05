use chrono::{Local, Utc};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::Clear;
use ratatui::widgets::{Block, Borders, Padding, Paragraph};
use rusqlite::params;

use anyhow::Result;

use crate::config::Config;
use crate::db;
use crate::tui::daemon_control::DaemonViewStatus;

const MANUAL_STOP_SNOOZE_UNTIL_KEY: &str = "autotracking_snooze_until";

#[derive(Debug, Default, Clone)]
pub struct CurrentState {
    pub modal: Option<CurrentTrackModal>,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct CurrentTrackModal {
    pub projects: Vec<String>,
    pub selected_project: Option<usize>,
    // 0=project,1=ok,2=cancel
    pub field_idx: usize,
}

impl CurrentTrackModal {
    fn new(projects: Vec<String>) -> Self {
        let selected_project = if projects.is_empty() { None } else { Some(0) };
        Self {
            projects,
            selected_project,
            field_idx: 0,
        }
    }
}

impl CurrentState {
    pub fn render(
        &self,
        frame: &mut Frame<'_>,
        area: Rect,
        conn: &rusqlite::Connection,
        daemon_status: DaemonViewStatus,
    ) {
        let now = Utc::now();
        let all = db::list_all_trackings(conn).unwrap_or_default();
        let active = db::get_active_tracking(conn).ok().flatten();

        let today_local = now.with_timezone(&Local).date_naive();
        let total_today_secs: i64 = all
            .iter()
            .filter(|t| {
                parse_tracking_ts(&t.start_ts)
                    .map(|dt| dt.with_timezone(&Local).date_naive() == today_local)
                    .unwrap_or(false)
            })
            .map(|t| duration_secs(&t.start_ts, t.end_ts.as_deref(), now))
            .sum();

        let current_secs = active
            .as_ref()
            .map(|t| duration_secs(&t.start_ts, t.end_ts.as_deref(), now))
            .unwrap_or(0);

        let title_height: u16 = 3;
        let title_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: title_height,
        };
        // Reserve one extra line at the bottom for the status/message bar so it
        // doesn't overlap the content. Title sits at the top, main content in
        // the middle, and the footer occupies the final line.
        let content_area = Rect {
            x: area.x,
            y: area.y + title_height,
            width: area.width,
            height: area
                .height
                .saturating_sub(title_height)
                .saturating_sub(1),
        };

        let left = " CURRENT";
        let hints = "s=track | d=stop tracking ";
        let inner_width = title_area.width.saturating_sub(2) as usize;
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
        frame.render_widget(Paragraph::new(title_line).block(title_block), title_area);

        let total_big = render_big_duration_lines(
            &format_duration(total_today_secs),
            progress_shades(total_today_secs),
        );
        let mut lines: Vec<Line<'static>> = Vec::new();
        for row in 0..5 {
            lines.push(Line::raw(total_big[row].clone()));
        }
        lines.push(Line::raw(String::new()));
        if let Some(t) = active {
            lines.push(Line::raw(format!(
                "{} | {}",
                t.project_name,
                format_duration(current_secs)
            )));
        } else {
            // When no project is tracked, only show the placeholder "(none)"
            // (do not show the separator '|' or a 0:00 time)
            lines.push(Line::raw("(none)"));
        }
        let daemon_text = match daemon_status {
            DaemonViewStatus::Running => "Daemon running",
            DaemonViewStatus::Stopped => "Daemon stopped",
            DaemonViewStatus::Outside => "Daemon running outside TUI",
        };
        lines.push(Line::raw(String::new()));
        lines.push(Line::from(Span::styled(
            daemon_text,
            Style::default().fg(Color::DarkGray),
        )));

        let inner_h = content_area.height.saturating_sub(2) as usize;
        let top_pad = inner_h.saturating_sub(lines.len()) / 2;
        let mut centered: Vec<Line<'static>> = Vec::new();
        for _ in 0..top_pad {
            centered.push(Line::raw(String::new()));
        }
        centered.extend(lines);
        let content_text = Text::from(centered);

        let content_block = Block::default()
            .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
            .padding(Padding::horizontal(1));
        frame.render_widget(
            Paragraph::new(content_text)
                .alignment(Alignment::Center)
                .block(content_block),
            content_area,
        );

        if !self.message.is_empty() {
            let footer = Rect {
                x: area.x,
                y: area.y + area.height.saturating_sub(1),
                width: area.width,
                height: 1,
            };
            frame.render_widget(Paragraph::new(self.message.clone()), footer);
        }

        if let Some(modal) = &self.modal {
            let mw = (area.width / 2).min(80).max(40);
            let desired_mh = 6u16;
            let max_mh = area.height.saturating_sub(1).max(3);
            let mh = desired_mh.min(max_mh);
            let mx = area.x + (area.width.saturating_sub(mw)) / 2;
            let my = area.y + (area.height.saturating_sub(mh)) / 2;
            let modal_area = Rect {
                x: mx,
                y: my,
                width: mw,
                height: mh,
            };

            let proj_line = if !modal.projects.is_empty() {
                let project_name = modal
                    .selected_project
                    .and_then(|idx| modal.projects.get(idx))
                    .cloned()
                    .unwrap_or_default();
                format!(
                    "Project: {}{} (\u{2190}/\u{2192})",
                    if modal.field_idx == 0 { "> " } else { "  " },
                    project_name
                )
            } else {
                format!(
                    "Project: {}(no projects in DB)",
                    if modal.field_idx == 0 { "> " } else { "  " }
                )
            };

            let body = vec![
                proj_line,
                String::new(),
                format!(
                    "{}OK   {}CANCEL",
                    if modal.field_idx == 1 { "> " } else { "  " },
                    if modal.field_idx == 2 { "> " } else { "  " }
                ),
            ]
            .join("\n");

            frame.render_widget(Clear, modal_area);
            frame.render_widget(
                Paragraph::new(body).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Start tracking ")
                        .padding(Padding::horizontal(1)),
                ),
                modal_area,
            );
        }
    }

    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        conn: &mut rusqlite::Connection,
        config: &Config,
    ) -> Result<bool> {
        if self.modal.is_some() {
            let modal = self.modal.take().expect("modal present");
            let (next, changed) = self.handle_modal_key(key, modal, conn)?;
            self.modal = next;
            return Ok(changed);
        }

        if matches!(key.code, KeyCode::Char('s')) {
            let projects = db::projects(conn)
                .unwrap_or_default()
                .into_iter()
                .map(|p| p.name)
                .collect();
            self.modal = Some(CurrentTrackModal::new(projects));
            return Ok(false);
        }

        if matches!(key.code, KeyCode::Char('d')) {
            let now_dt = Utc::now();
            let now = crate::time::format_ts(&now_dt);
            let changed = conn.execute(
                "UPDATE trackings SET end_ts = ?1, updated_at = ?2 WHERE end_ts IS NULL",
                params![now, now],
            )?;
            self.message = if changed > 0 {
                let snooze_until = now_dt
                    + chrono::Duration::seconds(config.track_reminder_snooze_seconds as i64);
                db::upsert_config_key(
                    conn,
                    MANUAL_STOP_SNOOZE_UNTIL_KEY,
                    &crate::time::format_ts(&snooze_until),
                )?;
                "tracking stopped".to_string()
            } else {
                "no active tracking".to_string()
            };
            return Ok(changed > 0);
        }

        Ok(false)
    }

    fn handle_modal_key(
        &mut self,
        key: KeyEvent,
        mut modal: CurrentTrackModal,
        conn: &mut rusqlite::Connection,
    ) -> Result<(Option<CurrentTrackModal>, bool)> {
        match key.code {
            KeyCode::Left => {
                if modal.field_idx == 0 && !modal.projects.is_empty() {
                    let next = match modal.selected_project {
                        Some(0) | None => modal.projects.len() - 1,
                        Some(idx) => idx.saturating_sub(1),
                    };
                    modal.selected_project = Some(next);
                }
                Ok((Some(modal), false))
            }
            KeyCode::Right => {
                if modal.field_idx == 0 && !modal.projects.is_empty() {
                    let next = match modal.selected_project {
                        Some(idx) => (idx + 1) % modal.projects.len(),
                        None => 0,
                    };
                    modal.selected_project = Some(next);
                }
                Ok((Some(modal), false))
            }
            KeyCode::Tab | KeyCode::Down => {
                modal.field_idx = (modal.field_idx + 1) % 3;
                Ok((Some(modal), false))
            }
            KeyCode::Up => {
                modal.field_idx = if modal.field_idx == 0 {
                    2
                } else {
                    modal.field_idx - 1
                };
                Ok((Some(modal), false))
            }
            KeyCode::Esc => {
                self.message = "cancelled".to_string();
                Ok((None, false))
            }
            KeyCode::Enter => {
                if modal.field_idx == 2 {
                    self.message = "cancelled".to_string();
                    return Ok((None, false));
                }
                if modal.field_idx == 0 {
                    return Ok((Some(modal), false));
                }

                let project_name = modal
                    .selected_project
                    .and_then(|idx| modal.projects.get(idx))
                    .cloned();

                let Some(project_name) = project_name else {
                    self.message = "no projects defined; add a project first".to_string();
                    return Ok((None, false));
                };

                let now = Utc::now();
                let ts = crate::time::format_ts(&now);
                let tx = conn.transaction()?;
                tx.execute(
                    "UPDATE trackings SET end_ts = ?1, updated_at = ?2 WHERE end_ts IS NULL",
                    params![ts, ts],
                )?;
                tx.execute(
                    "INSERT INTO trackings (project_name, start_ts, created_by, notes) VALUES (?1, ?2, 'tui', ?3)",
                    params![project_name, crate::time::format_ts(&now), Option::<&str>::None],
                )?;
                tx.commit()?;
                self.message = format!("tracking started: {}", project_name);
                Ok((None, true))
            }
            _ => Ok((Some(modal), false)),
        }
    }
}

fn duration_secs(start_ts: &str, end_ts: Option<&str>, now: chrono::DateTime<Utc>) -> i64 {
    let start = match parse_tracking_ts(start_ts) {
        Ok(dt) => dt,
        Err(_) => return 0,
    };
    let end = match end_ts {
        Some(ts) => match parse_tracking_ts(ts) {
            Ok(dt) => dt,
            Err(_) => now,
        },
        None => now,
    };

    end.signed_duration_since(start).num_seconds().max(0)
}

fn parse_tracking_ts(raw: &str) -> Result<chrono::DateTime<Utc>, ()> {
    crate::time::parse_ts(raw).map_err(|_| ())
}

fn format_duration(secs: i64) -> String {
    let hrs = secs / 3600;
    let mins = (secs % 3600) / 60;
    format!("{}:{:02}", hrs, mins)
}

fn render_big_duration_lines(value: &str, row_shades: [char; 5]) -> [String; 5] {
    let mut lines = vec![
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    ];
    for ch in value.chars() {
        let glyph = glyph_mask(ch);
        for row in 0..5 {
            let on = row_shades[row];
            if !lines[row].is_empty() {
                lines[row].push(' ');
            }
            for px in glyph[row].chars() {
                if px == ' ' {
                    lines[row].push_str("  ");
                } else {
                    lines[row].push(on);
                    lines[row].push(on);
                }
            }
        }
    }
    [
        lines[0].clone(),
        lines[1].clone(),
        lines[2].clone(),
        lines[3].clone(),
        lines[4].clone(),
    ]
}

fn progress_shades(total_secs: i64) -> [char; 5] {
    let palette = ['░', '▒', '▓', '█'];
    let hour_progress = total_secs.rem_euclid(3600) as usize;
    let base = (hour_progress * 4) / 3600;
    let lo = base.saturating_sub(1);
    let hi = (base + 1).min(3);
    let top = palette[lo];
    let mid = palette[base.min(3)];
    let bot = palette[hi];
    [top, mid, mid, bot, bot]
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

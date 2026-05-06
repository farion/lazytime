use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::config::{Config, TimeRange};

const WEEKDAY_NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

#[derive(Debug, Clone)]
pub struct SettingsState {
    pub message: String,
    pub modal: Option<WorkingHoursModal>,
    pub editing: bool,
    pub selected: usize,
    pub offset: usize,
    pub default_project: String,
    pub tracking_stability_seconds: String,
    pub working_hours: BTreeMap<u8, Vec<TimeRange>>,
    pub track_reminder_seconds: String,
    pub track_reminder_snooze_seconds: String,
    pub summary_update_seconds: String,
    pub report_start: String,
    pub report_end: String,
    pub db_file: String,
    pub jira_url: String,
    pub jira_token: String,
    pub jira_token_masked: bool,
    pub jira_email: String,
    pub jira_project: String,
    pub jira_assignee: String,
    pub jira_issue_type: String,
    pub jira_sap_field: String,
    pub ipc_socket_path: String,
}

impl SettingsState {
    pub fn new_from_config(cfg: &Config) -> Self {
        Self {
            message: String::new(),
            modal: None,
            editing: false,
            selected: 0,
            offset: 0,
            default_project: cfg.default_project.clone(),
            tracking_stability_seconds: cfg.tracking_stability_seconds.to_string(),
            working_hours: cfg.working_hours.clone(),
            track_reminder_seconds: cfg.track_reminder_seconds.to_string(),
            track_reminder_snooze_seconds: cfg.track_reminder_snooze_seconds.to_string(),
            summary_update_seconds: cfg.summary_update_seconds.to_string(),
            report_start: cfg.report_start.clone().unwrap_or_default(),
            report_end: cfg.report_end.clone().unwrap_or_default(),
            db_file: cfg.db_file.clone(),
            jira_url: cfg.jira_url.clone().unwrap_or_default(),
            jira_token: cfg.jira_token.clone().unwrap_or_default(),
            jira_token_masked: true,
            jira_email: cfg.jira_email.clone().unwrap_or_default(),
            jira_project: cfg.jira_project.clone().unwrap_or_default(),
            jira_assignee: cfg.jira_assignee.clone().unwrap_or_default(),
            jira_issue_type: cfg.jira_issue_type.clone(),
            jira_sap_field: cfg.jira_sap_field.clone(),
            ipc_socket_path: cfg.ipc_socket_path.clone().unwrap_or_default(),
        }
    }

    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let left = " SETTINGS";
        let hints = if self.editing {
            "editing: Enter/Esc=done"
        } else {
            "x=open | s=save | r=reset | Enter=edit | m=mask token"
        };
        let inner_width = area.width.saturating_sub(2) as usize;
        let gap = inner_width
            .saturating_sub(left.chars().count())
            .saturating_sub(hints.chars().count())
            .max(1);
        let title_line = format!("{}{}{}", left, " ".repeat(gap), hints);

        let title = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 3,
        };
        frame.render_widget(
            Paragraph::new(title_line).block(Block::default().borders(Borders::ALL)),
            title,
        );

        let content = Rect {
            x: area.x,
            y: area.y + 3,
            width: area.width,
            height: area.height.saturating_sub(4),
        };

        let fields = self.field_rows();
        let total_lines = fields.len();
        if self.offset >= total_lines {
            self.offset = total_lines.saturating_sub(1);
        }
        let visible = content.height.saturating_sub(2) as usize;
        let selected_line = self.selected.saturating_mul(4);
        if selected_line < self.offset {
            self.offset = selected_line;
        }
        if visible > 0 && selected_line >= self.offset.saturating_add(visible) {
            self.offset = selected_line.saturating_sub(visible.saturating_sub(1));
        }

        let start = self.offset.min(total_lines);
        let end = (start + visible).min(total_lines);
        let view = fields[start..end].to_vec();

        frame.render_widget(
            Paragraph::new(view).block(Block::default().borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)),
            content,
        );

        if let Some(modal) = &self.modal {
            modal.render(frame, area);
        }
    }

    pub fn handle_key(
        &mut self,
        key: KeyEvent,
        _conn: &rusqlite::Connection,
        config_path: Option<&str>,
    ) -> Result<bool> {
        if let Some(modal) = &mut self.modal {
            match modal.handle_key(key) {
                ModalAction::Continue => return Ok(false),
                ModalAction::Cancel => {
                    self.modal = None;
                    self.message = "working_hours unchanged".to_string();
                    return Ok(false);
                }
                ModalAction::Save => {
                    self.working_hours = modal.to_map();
                    self.modal = None;
                    self.message = "working_hours updated".to_string();
                    return Ok(true);
                }
            }
        }

        if self.editing {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.editing = false;
                }
                KeyCode::Backspace => self.edit_backspace(),
                KeyCode::Char(c) => self.edit_push(c),
                _ => {}
            }
            return Ok(false);
        }

        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.field_count().saturating_sub(1));
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected = self.selected.saturating_sub(1);
            }
            KeyCode::Enter => {
                if self.selected == 2 {
                    self.modal = Some(WorkingHoursModal::from_map(&self.working_hours));
                } else {
                    self.editing = true;
                }
            }
            KeyCode::Char('m') if self.selected == 10 => {
                self.jira_token_masked = !self.jira_token_masked;
            }
            KeyCode::Char('r') => {
                let p = resolve_config_path(config_path);
                let cfg = Config::from_path(Some(&p.to_string_lossy()))?;
                *self = Self::new_from_config(&cfg);
                self.message = format!("form reset from {}", p.display());
                return Ok(true);
            }
            KeyCode::Char('s') => {
                let cfg = self.to_config().map_err(|e| anyhow::anyhow!(e))?;
                let p = resolve_config_path(config_path);
                if let Some(parent) = p.parent() {
                    fs::create_dir_all(parent)?;
                }
                let json = serde_json::to_string_pretty(&cfg)?;
                fs::write(&p, json)?;
                self.message = format!("saved {} (restart may be needed)", p.display());
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }

    fn field_count(&self) -> usize {
        17
    }

    fn field_rows(&self) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        self.push_field(
            &mut out,
            0,
            "default_project",
            &self.default_project,
            "Default project used when tracking from the TUI.",
            true,
        );
        self.push_field(
            &mut out,
            1,
            "tracking_stability_seconds",
            &self.tracking_stability_seconds,
            "Minimum stable window-event duration in seconds.",
            false,
        );
        self.push_field(
            &mut out,
            2,
            "working_hours",
            &format_working_hours_summary(&self.working_hours),
            "Per-day ranges. Press Enter to open structured weekday editor.",
            false,
        );
        self.push_field(&mut out, 3, "track_reminder_seconds", &self.track_reminder_seconds, "Reminder interval in seconds.", false);
        self.push_field(&mut out, 4, "track_reminder_snooze_seconds", &self.track_reminder_snooze_seconds, "Snooze time after manual stop, in seconds.", false);
        self.push_field(&mut out, 5, "summary_update_seconds", &self.summary_update_seconds, "Summary refresh interval in seconds.", false);
        self.push_field(&mut out, 6, "report_start", &self.report_start, "Default report start date (YYYY-MM-DD).", false);
        self.push_field(&mut out, 7, "report_end", &self.report_end, "Default report end date (YYYY-MM-DD).", false);
        self.push_field(&mut out, 8, "db_file", &self.db_file, "SQLite database file path.", false);
        self.push_field(&mut out, 9, "jira_url", &self.jira_url, "Jira base URL.", false);
        let token = if self.jira_token_masked {
            "*".repeat(self.jira_token.chars().count())
        } else {
            self.jira_token.clone()
        };
        self.push_field(&mut out, 10, "jira_token", &token, "Jira auth token (masked). Press m to toggle mask.", false);
        self.push_field(&mut out, 11, "jira_email", &self.jira_email, "Jira email for basic auth (optional).", false);
        self.push_field(&mut out, 12, "jira_project", &self.jira_project, "Jira project key for syncing.", false);
        self.push_field(&mut out, 13, "jira_assignee", &self.jira_assignee, "Jira assignee (optional override).", false);
        self.push_field(&mut out, 14, "jira_issue_type", &self.jira_issue_type, "Jira issue type to create.", false);
        self.push_field(&mut out, 15, "jira_sap_field", &self.jira_sap_field, "Custom Jira field name/id for SAP mapping.", false);
        self.push_field(&mut out, 16, "ipc_socket_path", &self.ipc_socket_path, "IPC endpoint for daemon communication.", false);
        out
    }

    fn push_field(&self, out: &mut Vec<Line<'static>>, idx: usize, name: &str, value: &str, desc: &str, required: bool) {
        let mut label = name.to_string();
        if required {
            label.push('*');
        }
        let style = if idx == self.selected {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        out.push(Line::from(Span::styled(label, style)));
        out.push(Line::from(Span::styled(value.to_string(), style)));
        out.push(Line::from(Span::styled(
            desc.to_string(),
            Style::default().fg(Color::DarkGray),
        )));
        out.push(Line::raw(String::new()));
    }

    fn edit_push(&mut self, c: char) {
        match self.selected {
            0 => self.default_project.push(c),
            1 => self.tracking_stability_seconds.push(c),
            3 => self.track_reminder_seconds.push(c),
            4 => self.track_reminder_snooze_seconds.push(c),
            5 => self.summary_update_seconds.push(c),
            6 => self.report_start.push(c),
            7 => self.report_end.push(c),
            8 => self.db_file.push(c),
            9 => self.jira_url.push(c),
            10 => self.jira_token.push(c),
            11 => self.jira_email.push(c),
            12 => self.jira_project.push(c),
            13 => self.jira_assignee.push(c),
            14 => self.jira_issue_type.push(c),
            15 => self.jira_sap_field.push(c),
            16 => self.ipc_socket_path.push(c),
            _ => {}
        }
    }

    fn edit_backspace(&mut self) {
        match self.selected {
            0 => {
                self.default_project.pop();
            }
            1 => {
                self.tracking_stability_seconds.pop();
            }
            3 => {
                self.track_reminder_seconds.pop();
            }
            4 => {
                self.track_reminder_snooze_seconds.pop();
            }
            5 => {
                self.summary_update_seconds.pop();
            }
            6 => {
                self.report_start.pop();
            }
            7 => {
                self.report_end.pop();
            }
            8 => {
                self.db_file.pop();
            }
            9 => {
                self.jira_url.pop();
            }
            10 => {
                self.jira_token.pop();
            }
            11 => {
                self.jira_email.pop();
            }
            12 => {
                self.jira_project.pop();
            }
            13 => {
                self.jira_assignee.pop();
            }
            14 => {
                self.jira_issue_type.pop();
            }
            15 => {
                self.jira_sap_field.pop();
            }
            16 => {
                self.ipc_socket_path.pop();
            }
            _ => {}
        }
    }

    fn to_config(&self) -> std::result::Result<Config, String> {
        let cfg = Config {
            default_project: self.default_project.clone(),
            tracking_stability_seconds: parse_u64("tracking_stability_seconds", &self.tracking_stability_seconds)?,
            working_hours: self.working_hours.clone(),
            track_reminder_seconds: parse_u64("track_reminder_seconds", &self.track_reminder_seconds)?,
            track_reminder_snooze_seconds: parse_u64(
                "track_reminder_snooze_seconds",
                &self.track_reminder_snooze_seconds,
            )?,
            summary_update_seconds: parse_u64("summary_update_seconds", &self.summary_update_seconds)?,
            report_start: to_opt(&self.report_start),
            report_end: to_opt(&self.report_end),
            db_file: self.db_file.clone(),
            jira_url: to_opt(&self.jira_url),
            jira_token: to_opt(&self.jira_token),
            jira_email: to_opt(&self.jira_email),
            jira_project: to_opt(&self.jira_project),
            jira_assignee: to_opt(&self.jira_assignee),
            jira_issue_type: self.jira_issue_type.clone(),
            jira_sap_field: self.jira_sap_field.clone(),
            ipc_socket_path: to_opt(&self.ipc_socket_path),
        };
        cfg.validate().map_err(|e| e.to_string())?;
        Ok(cfg)
    }
}

#[derive(Debug, Clone)]
pub struct WorkingHoursModal {
    days: Vec<Vec<TimeRange>>,
    day_idx: usize,
    range_idx: usize,
    edit_target: Option<EditTarget>,
    edit_buf: String,
}

#[derive(Debug, Clone, Copy)]
enum EditTarget {
    Start,
    End,
}

#[derive(Debug, Clone, Copy)]
pub enum ModalAction {
    Continue,
    Save,
    Cancel,
}

impl WorkingHoursModal {
    fn from_map(map: &BTreeMap<u8, Vec<TimeRange>>) -> Self {
        let mut days = vec![Vec::new(); 7];
        for (k, v) in map {
            if (*k as usize) < 7 {
                days[*k as usize] = v.clone();
            }
        }
        Self {
            days,
            day_idx: 0,
            range_idx: 0,
            edit_target: None,
            edit_buf: String::new(),
        }
    }

    fn to_map(&self) -> BTreeMap<u8, Vec<TimeRange>> {
        let mut out = BTreeMap::new();
        for (i, ranges) in self.days.iter().enumerate() {
            if !ranges.is_empty() {
                out.insert(i as u8, ranges.clone());
            }
        }
        out
    }

    fn render(&self, frame: &mut Frame<'_>, area: Rect) {
        let mw = area.width.saturating_sub(6).min(90);
        let mh = area.height.saturating_sub(6).min(20);
        let mx = area.x + (area.width.saturating_sub(mw)) / 2;
        let my = area.y + (area.height.saturating_sub(mh)) / 2;
        let rect = Rect { x: mx, y: my, width: mw, height: mh };

        let mut lines = Vec::new();
        lines.push(Line::raw("weekdays by name | ranges"));
        for (i, day) in WEEKDAY_NAMES.iter().enumerate() {
            let marker = if i == self.day_idx { ">" } else { " " };
            let right = self.days[i]
                .iter()
                .enumerate()
                .map(|(ri, r)| {
                    if i == self.day_idx && ri == self.range_idx {
                        format!("[{}-{}]", r.start, r.end)
                    } else {
                        format!("{}-{}", r.start, r.end)
                    }
                })
                .collect::<Vec<_>>()
                .join("  ");
            lines.push(Line::raw(format!("{} {:<3} | {}", marker, day, right)));
        }
        lines.push(Line::raw(String::new()));
        lines.push(Line::from(Span::styled(
            "Left/Right: weekday  Up/Down: range  a:add  d:delete  Enter:edit start/end  s:save  Esc:cancel",
            Style::default().fg(Color::DarkGray),
        )));
        if let Some(target) = self.edit_target {
            let which = match target {
                EditTarget::Start => "start",
                EditTarget::End => "end",
            };
            lines.push(Line::from(Span::styled(
                format!("editing {}: {}", which, self.edit_buf),
                Style::default().fg(Color::Yellow),
            )));
        }

        frame.render_widget(Clear, rect);
        frame.render_widget(
            Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title(" working_hours ")),
            rect,
        );
    }

    fn handle_key(&mut self, key: KeyEvent) -> ModalAction {
        if let Some(target) = self.edit_target {
            match key.code {
                KeyCode::Esc => {
                    self.edit_target = None;
                    self.edit_buf.clear();
                    return ModalAction::Continue;
                }
                KeyCode::Backspace => {
                    self.edit_buf.pop();
                    return ModalAction::Continue;
                }
                KeyCode::Char(c) => {
                    self.edit_buf.push(c);
                    return ModalAction::Continue;
                }
                KeyCode::Enter => {
                    if let Some(r) = self.days[self.day_idx].get_mut(self.range_idx) {
                        match target {
                            EditTarget::Start => {
                                r.start = self.edit_buf.clone();
                                self.edit_target = Some(EditTarget::End);
                                self.edit_buf = r.end.clone();
                            }
                            EditTarget::End => {
                                r.end = self.edit_buf.clone();
                                self.edit_target = None;
                                self.edit_buf.clear();
                            }
                        }
                    }
                    return ModalAction::Continue;
                }
                _ => return ModalAction::Continue,
            }
        }

        match key.code {
            KeyCode::Esc => ModalAction::Cancel,
            KeyCode::Char('s') => ModalAction::Save,
            KeyCode::Left => {
                self.day_idx = self.day_idx.saturating_sub(1);
                self.range_idx = 0;
                ModalAction::Continue
            }
            KeyCode::Right => {
                self.day_idx = (self.day_idx + 1).min(6);
                self.range_idx = 0;
                ModalAction::Continue
            }
            KeyCode::Up => {
                self.range_idx = self.range_idx.saturating_sub(1);
                ModalAction::Continue
            }
            KeyCode::Down => {
                let len = self.days[self.day_idx].len();
                if len > 0 {
                    self.range_idx = (self.range_idx + 1).min(len - 1);
                }
                ModalAction::Continue
            }
            KeyCode::Char('a') => {
                self.days[self.day_idx].push(TimeRange {
                    start: "09:00".to_string(),
                    end: "17:00".to_string(),
                });
                self.range_idx = self.days[self.day_idx].len().saturating_sub(1);
                ModalAction::Continue
            }
            KeyCode::Char('d') => {
                if !self.days[self.day_idx].is_empty() {
                    let idx = self.range_idx.min(self.days[self.day_idx].len() - 1);
                    self.days[self.day_idx].remove(idx);
                    self.range_idx = self.range_idx.saturating_sub(1);
                }
                ModalAction::Continue
            }
            KeyCode::Enter => {
                if let Some(r) = self.days[self.day_idx].get(self.range_idx) {
                    self.edit_target = Some(EditTarget::Start);
                    self.edit_buf = r.start.clone();
                }
                ModalAction::Continue
            }
            _ => ModalAction::Continue,
        }
    }
}

fn format_working_hours_summary(map: &BTreeMap<u8, Vec<TimeRange>>) -> String {
    let mut parts = Vec::new();
    for (day, ranges) in map {
        if ranges.is_empty() || (*day as usize) >= WEEKDAY_NAMES.len() {
            continue;
        }
        parts.push(format!("{}:{}", WEEKDAY_NAMES[*day as usize], ranges.len()));
    }
    if parts.is_empty() {
        "(none)".to_string()
    } else {
        parts.join(" ")
    }
}

fn parse_u64(name: &str, raw: &str) -> std::result::Result<u64, String> {
    raw.trim()
        .parse::<u64>()
        .map_err(|e| format!("{} invalid: {}", name, e))
}

fn to_opt(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn resolve_config_path(config_path: Option<&str>) -> PathBuf {
    if let Some(path) = config_path {
        return PathBuf::from(path);
    }
    if let Some(home) = dirs::home_dir() {
        return home.join(".config/lazytime/config.json");
    }
    PathBuf::from("./config.json")
}

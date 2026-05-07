use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Clear, Padding, Paragraph, Row, Table};
use std::cmp::min;
use std::collections::VecDeque;
use std::io::BufRead;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};

use crate::config::Config;
use crate::daemon::DAEMON_RUNTIME_LOCK_KEY;
use crate::db;

const MAX_LOG_LINES: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonViewStatus {
    Outside,
    Running,
    Stopped,
}

impl DaemonViewStatus {
    fn badge(self) -> &'static str {
        match self {
            Self::Outside => "outside",
            Self::Running => "running",
            Self::Stopped => "stopped",
        }
    }
}

enum DaemonEvent {
    Log(String),
}

#[derive(Debug, Clone)]
struct LockOwner {
    token: Option<String>,
    pid: Option<u32>,
}

pub struct DaemonControlState {
    pub logs: VecDeque<String>,
    pub selected: usize,
    pub offset: usize,
    pub visible_rows: usize,
    pub message: String,
    pub status: DaemonViewStatus,
    owner_id: String,
    child: Option<Child>,
    receiver: Option<Receiver<DaemonEvent>>,
}

impl Default for DaemonControlState {
    fn default() -> Self {
        Self {
            logs: VecDeque::new(),
            selected: 0,
            offset: 0,
            visible_rows: 1,
            message: String::new(),
            status: DaemonViewStatus::Stopped,
            owner_id: format!(
                "tui:{}:{}",
                std::process::id(),
                chrono::Utc::now().timestamp()
            ),
            child: None,
            receiver: None,
        }
    }
}

impl DaemonControlState {
    pub fn auto_start_on_tui_launch(&mut self, config: &Config) -> Result<()> {
        self.refresh_status_from_lock(config);
        if self.status == DaemonViewStatus::Stopped {
            self.start_daemon(config)?;
            self.push_log("daemon auto-started by TUI".to_string());
            self.message = "daemon auto-started".to_string();
        }
        Ok(())
    }

    pub fn poll(&mut self, config: &Config) {
        self.poll_events();
        self.poll_child_exit(config);
        self.refresh_status_from_lock(config);
    }

    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        frame.render_widget(Clear, area);

        let title_height: u16 = 3;
        let content_area = Rect {
            x: area.x,
            y: area.y + title_height,
            width: area.width,
            height: area.height.saturating_sub(title_height).saturating_sub(1),
        };

        let left = format!(" DAEMON [{}]", self.status.badge());
        let hints = match self.status {
            DaemonViewStatus::Outside => "d=stop outside daemon",
            DaemonViewStatus::Running => "d=stop daemon | up/down=scroll ",
            DaemonViewStatus::Stopped => "s=start daemon | d=stop daemon | up/down=scroll ",
        };
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

        frame.render_widget(
            Paragraph::new(title_line).block(
                Block::default()
                    .borders(Borders::ALL)
                    .padding(Padding::horizontal(0)),
            ),
            Rect {
                x: area.x,
                y: area.y,
                width: area.width,
                height: title_height,
            },
        );

        if self.status == DaemonViewStatus::Outside {
            let inner_width = table_inner_width(content_area.width);
            let text = fit_line("Daemon running outside TUI", inner_width);
            let table = Table::new([Row::new(vec![Cell::from(text)])], [Constraint::Min(10)])
                .header(Row::new(vec![
                    Cell::from("Log ").style(Style::default().add_modifier(Modifier::BOLD)),
                ]))
                .block(
                    Block::default()
                        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
                        .padding(Padding::horizontal(1)),
                );
            frame.render_widget(table, content_area);
        } else {
            let inner_width = table_inner_width(content_area.width);
            let rows_all: Vec<_> = self
                .logs
                .iter()
                .enumerate()
                .map(|(idx, line)| {
                    let style = if idx == self.selected {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };
                    let rendered = fit_line(line, inner_width);
                    Row::new(vec![Cell::from(rendered)]).style(style)
                })
                .collect();

            let visible = crate::tui::table_visible_rows(content_area.height);
            self.visible_rows = visible.max(1);
            let len = rows_all.len();
            let selected = self.selected.min(len.saturating_sub(1));
            let start = crate::tui::scroll_offset_for_selection(
                selected,
                self.offset,
                len,
                self.visible_rows,
                2,
            );
            let end = min(start + visible, len);

            let table = Table::new(rows_all[start..end].iter().cloned(), [Constraint::Min(10)])
                .header(Row::new(vec![
                    Cell::from("Log ").style(Style::default().add_modifier(Modifier::BOLD)),
                ]))
                .block(
                    Block::default()
                        .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
                        .padding(Padding::horizontal(1)),
                );
            frame.render_widget(table, content_area);
        }

        let footer = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };
        frame.render_widget(Clear, footer);
        let mut footer_text = self.message.clone();
        let max_width = footer.width as usize;
        if footer_text.chars().count() > max_width {
            footer_text = footer_text.chars().take(max_width).collect();
        }
        let fill = max_width.saturating_sub(footer_text.chars().count());
        footer_text.push_str(&" ".repeat(fill));
        frame.render_widget(Paragraph::new(footer_text), footer);
    }

    pub fn handle_key(&mut self, key: KeyEvent, config: &Config) -> Result<bool> {
        if self.status == DaemonViewStatus::Outside {
            return match key.code {
                KeyCode::Char('d') => {
                    self.stop_outside_daemon(config)?;
                    Ok(true)
                }
                _ => {
                    self.message = "Daemon running outside TUI".to_string();
                    Ok(true)
                }
            };
        }

        match key.code {
            KeyCode::Char('s') => {
                self.start_daemon(config)?;
                Ok(true)
            }
            KeyCode::Char('d') => {
                self.stop_daemon(config)?;
                Ok(true)
            }
            KeyCode::Down => {
                if !self.logs.is_empty() {
                    self.selected = (self.selected + 1).min(self.logs.len() - 1);
                    self.offset = crate::tui::scroll_offset_for_selection(
                        self.selected,
                        self.offset,
                        self.logs.len(),
                        self.visible_rows,
                        2,
                    );
                }
                Ok(true)
            }
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
                self.offset = crate::tui::scroll_offset_for_selection(
                    self.selected,
                    self.offset,
                    self.logs.len(),
                    self.visible_rows,
                    2,
                );
                Ok(true)
            }
            KeyCode::PageDown => {
                if !self.logs.is_empty() {
                    let step = self.visible_rows.max(1);
                    self.selected = (self.selected + step).min(self.logs.len() - 1);
                    self.offset = crate::tui::scroll_offset_for_selection(
                        self.selected,
                        self.offset,
                        self.logs.len(),
                        self.visible_rows,
                        2,
                    );
                }
                Ok(true)
            }
            KeyCode::PageUp => {
                let step = self.visible_rows.max(1);
                self.selected = self.selected.saturating_sub(step);
                self.offset = crate::tui::scroll_offset_for_selection(
                    self.selected,
                    self.offset,
                    self.logs.len(),
                    self.visible_rows,
                    2,
                );
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn stop_owned_on_exit(&mut self, config: &Config) {
        let _ = self.stop_daemon(config);

        self.cleanup_owned_lock(config, true);
    }

    fn start_daemon(&mut self, config: &Config) -> Result<()> {
        if self.status == DaemonViewStatus::Running {
            self.message = "daemon already running".to_string();
            return Ok(());
        }

        self.message = "starting daemon".to_string();
        let exe = std::env::current_exe()?;
        let mut cmd = Command::new(exe);
        cmd.arg("--daemon")
            .env("LAZYTIME_DAEMON_OWNER", self.owner_id.clone())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(err) => {
                self.message = format!("failed to start daemon: {err}");
                self.push_log(self.message.clone());
                return Ok(());
            }
        };

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let (tx, rx) = mpsc::channel::<DaemonEvent>();
        if let Some(stdout) = stdout {
            let tx_out = tx.clone();
            std::thread::spawn(move || {
                let reader = std::io::BufReader::new(stdout);
                for line in reader.lines().map_while(|line| line.ok()) {
                    let _ = tx_out.send(DaemonEvent::Log(line));
                }
            });
        }
        if let Some(stderr) = stderr {
            let tx_err = tx.clone();
            std::thread::spawn(move || {
                let reader = std::io::BufReader::new(stderr);
                for line in reader.lines().map_while(|line| line.ok()) {
                    let _ = tx_err.send(DaemonEvent::Log(line));
                }
            });
        }

        self.child = Some(child);
        self.receiver = Some(rx);
        self.status = DaemonViewStatus::Running;
        self.push_log("daemon process started".to_string());
        self.message = "daemon running".to_string();

        self.refresh_status_from_lock(config);
        if self.status == DaemonViewStatus::Outside {
            self.message = "Daemon running outside TUI".to_string();
        }
        Ok(())
    }

    fn stop_daemon(&mut self, config: &Config) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }

        self.cleanup_owned_lock(config, true);

        self.receiver = None;
        self.status = DaemonViewStatus::Stopped;
        self.push_log("daemon stop requested".to_string());
        self.message = "daemon stopped".to_string();
        Ok(())
    }

    fn refresh_status_from_lock(&mut self, config: &Config) {
        let lock_owner = db::open(config.db_path()).ok().and_then(|conn| {
            db::get_config_key(&conn, DAEMON_RUNTIME_LOCK_KEY)
                .ok()
                .flatten()
        });

        self.status = match lock_owner {
            Some(owner) if lock_owner_matches_tui(&owner, &self.owner_id) => {
                DaemonViewStatus::Running
            }
            Some(_) => DaemonViewStatus::Outside,
            None => DaemonViewStatus::Stopped,
        };
    }

    fn poll_events(&mut self) {
        loop {
            let event = match self.receiver.as_ref() {
                Some(rx) => match rx.try_recv() {
                    Ok(ev) => ev,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.receiver = None;
                        break;
                    }
                },
                None => break,
            };

            match event {
                DaemonEvent::Log(line) => self.push_log(line),
            }
        }
    }

    fn poll_child_exit(&mut self, config: &Config) {
        let Some(child) = self.child.as_mut() else {
            return;
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                self.push_log(format!("daemon exited: {status}"));
                if !status.success() {
                    self.push_log("daemon exited with failure; clearing sqlite lock".to_string());
                }
                self.cleanup_owned_lock(config, false);
                self.status = DaemonViewStatus::Stopped;
                self.child = None;
                self.message = "daemon stopped".to_string();
            }
            Ok(None) => {}
            Err(err) => {
                self.push_log(format!("failed to poll daemon process: {err}"));
                self.cleanup_owned_lock(config, false);
                self.status = DaemonViewStatus::Stopped;
                self.child = None;
                self.message = "daemon stopped".to_string();
            }
        }
    }

    fn cleanup_owned_lock(&mut self, config: &Config, stop_pid: bool) {
        let lock_owner = db::open(config.db_path()).ok().and_then(|conn| {
            db::get_config_key(&conn, DAEMON_RUNTIME_LOCK_KEY)
                .ok()
                .flatten()
        });
        if let Some(owner) = lock_owner {
            let parsed = parse_lock_owner(&owner);
            let owned_by_tui =
                parsed.token.as_deref() == Some(self.owner_id.as_str()) || owner == self.owner_id;
            if owned_by_tui {
                if stop_pid {
                    if let Some(pid) = parsed.pid {
                        let _ = stop_process_by_pid(pid);
                    }
                }
                let _ = release_owned_lock(config, &owner);
            }
        }
    }

    fn stop_outside_daemon(&mut self, config: &Config) -> Result<()> {
        let conn = db::open(config.db_path())?;
        let lock_owner = db::get_config_key(&conn, DAEMON_RUNTIME_LOCK_KEY)?;
        let Some(owner) = lock_owner else {
            self.message = "daemon is not running".to_string();
            self.status = DaemonViewStatus::Stopped;
            return Ok(());
        };

        if owner == self.owner_id {
            return self.stop_daemon(config);
        }

        let parsed = parse_lock_owner(&owner);
        let Some(pid) = parsed.pid else {
            self.message = format!("cannot stop outside daemon (owner={owner})");
            self.push_log(self.message.clone());
            return Ok(());
        };

        let stop_ok = stop_process_by_pid(pid);
        if !stop_ok {
            self.message = format!("failed to stop outside daemon pid={pid}");
            self.push_log(self.message.clone());
            return Ok(());
        }

        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let conn = db::open(config.db_path())?;
            let current = db::get_config_key(&conn, DAEMON_RUNTIME_LOCK_KEY)?;
            if current.is_none() {
                self.status = DaemonViewStatus::Stopped;
                self.message = "outside daemon stopped".to_string();
                self.push_log(format!("stopped outside daemon pid={pid}"));
                return Ok(());
            }
        }

        self.message = format!("stop sent to pid={pid}; waiting for shutdown");
        self.push_log(self.message.clone());
        Ok(())
    }

    fn push_log(&mut self, line: String) {
        self.logs.push_back(format_log_entry(&line));
        while self.logs.len() > MAX_LOG_LINES {
            self.logs.pop_front();
        }
        if !self.logs.is_empty() {
            self.selected = self.logs.len() - 1;
            self.offset = crate::tui::scroll_offset_for_selection(
                self.selected,
                self.offset,
                self.logs.len(),
                self.visible_rows,
                2,
            );
        }
    }
}

fn format_log_entry(raw: &str) -> String {
    let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let msg = extract_log_message(raw);
    format!("{ts} {msg}")
}

fn extract_log_message(raw: &str) -> String {
    let text = strip_ansi(raw);
    let text = text.trim();
    let Some((_, mut rest)) = split_after_level(text) else {
        return text.to_string();
    };

    // Remove common target/module prefixes repeatedly.
    for _ in 0..3 {
        let trimmed = rest.trim_start();
        if let Some(end) = trimmed.find("] ")
            && trimmed.starts_with('[')
            && end > 1
        {
            rest = &trimmed[end + 2..];
            continue;
        }

        if let Some(pos) = trimmed.find(" - ") {
            let prefix = &trimmed[..pos];
            if is_boilerplate_prefix(prefix) {
                rest = &trimmed[pos + 3..];
                continue;
            }
        }

        if let Some(pos) = trimmed.find(": ") {
            let prefix = &trimmed[..pos];
            if is_boilerplate_prefix(prefix) {
                rest = &trimmed[pos + 2..];
                continue;
            }
        }

        rest = trimmed;
        break;
    }

    rest.trim().to_string()
}

fn split_after_level(input: &str) -> Option<(&str, &str)> {
    for level in ["TRACE", "DEBUG", "INFO", "WARN", "ERROR"] {
        if let Some(idx) = input.find(level) {
            let start_ok = idx == 0
                || !input[..idx]
                    .chars()
                    .next_back()
                    .unwrap_or(' ')
                    .is_ascii_alphanumeric();
            let end_idx = idx + level.len();
            let end_ok = end_idx >= input.len()
                || !input[end_idx..]
                    .chars()
                    .next()
                    .unwrap_or(' ')
                    .is_ascii_alphanumeric();
            if start_ok && end_ok {
                return Some((&input[..idx], &input[end_idx..]));
            }
        }
    }
    None
}

fn is_boilerplate_prefix(prefix: &str) -> bool {
    let trimmed = prefix.trim();
    if trimmed.is_empty() || trimmed.len() > 80 {
        return false;
    }
    if trimmed.contains("::") {
        return true;
    }
    trimmed.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(c, '_' | '-' | '.' | '[' | ']' | '(' | ')' | '{' | '}' | '=')
    })
}

fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i += 1;
            if i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                while i < bytes.len() {
                    let b = bytes[i];
                    i += 1;
                    if b.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn table_inner_width(total_width: u16) -> usize {
    total_width.saturating_sub(4) as usize
}

fn fit_line(input: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let mut out: String = input.chars().take(width).collect();
    let fill = width.saturating_sub(out.chars().count());
    out.push_str(&" ".repeat(fill));
    out
}

fn lock_owner_matches_tui(owner: &str, owner_id: &str) -> bool {
    let parsed = parse_lock_owner(owner);
    match parsed.token {
        Some(token) => token == owner_id,
        None => owner == owner_id,
    }
}

fn release_owned_lock(config: &Config, lock_owner: &str) -> Result<()> {
    let conn = db::open(config.db_path())?;
    db::release_lock_if_value(&conn, DAEMON_RUNTIME_LOCK_KEY, lock_owner)?;
    Ok(())
}

fn parse_lock_owner(owner: &str) -> LockOwner {
    if let Some((token, tail)) = owner.split_once("|pid:") {
        let pid = tail.trim().parse::<u32>().ok();
        return LockOwner {
            token: Some(token.to_string()),
            pid,
        };
    }

    if let Some(pid_str) = owner.strip_prefix("pid:") {
        return LockOwner {
            token: None,
            pid: pid_str.trim().parse::<u32>().ok(),
        };
    }

    LockOwner {
        token: Some(owner.to_string()),
        pid: None,
    }
}

impl Drop for DaemonControlState {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(target_family = "unix")]
fn stop_process_by_pid(pid: u32) -> bool {
    Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::extract_log_message;

    #[test]
    fn strips_tracing_prefix_with_target() {
        let raw = "2026-05-05T10:11:12Z INFO lazytime::daemon: started tracking project";
        assert_eq!(extract_log_message(raw), "started tracking project");
    }

    #[test]
    fn strips_bracketed_boilerplate() {
        let raw = "2026-05-05 12:00:00 INFO [lazytime::platform] reconnecting sway";
        assert_eq!(extract_log_message(raw), "reconnecting sway");
    }

    #[test]
    fn keeps_plain_message() {
        let raw = "daemon process started";
        assert_eq!(extract_log_message(raw), "daemon process started");
    }
}

#[cfg(target_family = "windows")]
fn stop_process_by_pid(pid: u32) -> bool {
    Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

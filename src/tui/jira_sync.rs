use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Padding, Row, Table};
use std::cmp::min;
use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver};

use crate::config::Config;
use crate::jira_sync::{self, JiraSyncEvent};

const MAX_LOG_LINES: usize = 2000;

pub struct JiraSyncState {
    pub logs: VecDeque<String>,
    pub running: bool,
    pub message: String,
    pub selected: usize,
    pub offset: usize,
    pub visible_rows: usize,
    pub processed: usize,
    pub total: usize,
    receiver: Option<Receiver<JiraSyncEvent>>,
}

impl Default for JiraSyncState {
    fn default() -> Self {
        Self {
            logs: VecDeque::new(),
            running: false,
            message: String::new(),
            selected: 0,
            offset: 0,
            visible_rows: 1,
            processed: 0,
            total: 0,
            receiver: None,
        }
    }
}

impl JiraSyncState {
    pub fn poll_events(&mut self) {
        let mut finished = false;
        let mut pending = Vec::new();
        if let Some(receiver) = self.receiver.as_ref() {
            while let Ok(event) = receiver.try_recv() {
                pending.push(event);
            }
        }
        for event in pending {
            match event {
                JiraSyncEvent::Log(line) => self.push_log(line),
                JiraSyncEvent::Progress { processed, total } => {
                    self.processed = processed;
                    self.total = total;
                }
                JiraSyncEvent::Finished { success, message } => {
                    self.running = false;
                    self.message = if success {
                        format!("done: {}", message)
                    } else {
                        format!("failed: {}", message)
                    };
                    finished = true;
                }
            }
        }
        if finished {
            self.receiver = None;
        }
    }

    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect) {
        let title_height: u16 = 3;
        let content_area = Rect {
            x: area.x,
            y: area.y + title_height,
            width: area.width,
            height: area.height.saturating_sub(title_height).saturating_sub(1),
        };

        let left = " JIRA SYNC";
        let hints = if self.running {
            "s=start(disabled) | up/down=scroll "
        } else {
            "s=start sync | up/down=scroll "
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
            ratatui::widgets::Paragraph::new(title_line).block(
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
                Row::new(vec![Cell::from(line.clone())]).style(style)
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

        let status = if self.running { "RUNNING" } else { "IDLE" };
        let footer_text = format!(
            "progress: {}/{} | {} | {}",
            self.processed, self.total, status, self.message
        );
        let footer = Rect {
            x: area.x,
            y: area.y + area.height.saturating_sub(1),
            width: area.width,
            height: 1,
        };
        frame.render_widget(ratatui::widgets::Paragraph::new(footer_text), footer);
    }

    pub fn handle_key(&mut self, key: KeyEvent, config: &Config) -> Result<bool> {
        match key.code {
            KeyCode::Char('s') => {
                if self.running {
                    self.message = "sync already running".to_string();
                    return Ok(true);
                }

                let (tx, rx) = mpsc::channel();
                self.receiver = Some(rx);
                self.running = true;
                self.message = "starting sync".to_string();
                self.processed = 0;
                self.total = 0;

                let cfg = config.clone();
                std::thread::spawn(move || {
                    // When running under the TUI, disable jira module's tracing so
                    // it doesn't print below the TUI. It will be restored after run.
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
                        jira_sync::run_jira_sync(&cfg, false, Some(tx.clone())).await
                    });
                    // restore tracing for jira module so other parts of the app behave normally
                    crate::jira::set_tracing_enabled(true);
                    if let Err(err) = result {
                        let _ = tx.send(JiraSyncEvent::Finished {
                            success: false,
                            message: err.to_string(),
                        });
                    }
                });

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

    fn push_log(&mut self, line: String) {
        self.logs.push_back(line);
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

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Frame;
use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Cell, Padding, Row, Table};
use regex::Regex;

use crate::db;
use crate::tui::projects_modal::{
    ConfirmKind, ConfirmModal, ProjectModal, ProjectModalMode, ProjectsModal, RuleModal,
    RuleModalMode,
};

#[derive(Debug, Clone)]
pub enum ProjectsViewMode {
    ProjectsList,
    RulesOnly,
}

pub struct ProjectsState {
    pub selected_project: usize,
    pub selected_rule: usize,
    pub message: String,
    pub modal: Option<ProjectsModal>,
    pub view_mode: ProjectsViewMode,
    // scrolling offsets for visible window
    pub projects_offset: usize,
    pub rules_offset: usize,
    pub projects_visible_rows: usize,
    pub rules_visible_rows: usize,
}

impl Default for ProjectsState {
    fn default() -> Self {
        Self {
            selected_project: 0,
            selected_rule: 0,
            message: String::new(),
            modal: None,
            view_mode: ProjectsViewMode::ProjectsList,
            projects_offset: 0,
            rules_offset: 0,
            projects_visible_rows: 1,
            rules_visible_rows: 1,
        }
    }
}

impl ProjectsState {
    pub fn render(&mut self, frame: &mut Frame<'_>, area: Rect, conn: &rusqlite::Connection) {
        let projects = db::projects(conn).unwrap_or_default();
        let project_rows_all: Vec<_> = projects
            .iter()
            .enumerate()
            .map(|(idx, p)| {
                let style = if idx == self.selected_project {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
                Row::new(vec![
                    Cell::from(p.name.clone()),
                    Cell::from(p.sap_number.clone().unwrap_or_default()),
                ])
                .style(style)
            })
            .collect();
        // Render a small framed title box directly above the table. The title
        // box owns the horizontal seam (its bottom border), so the table box
        // omits the top border to avoid double lines. Title box height = 3
        // (top border, one content line, bottom border).
        // Reserve one extra line at the bottom for the status/message bar so it
        // doesn't overlap the table. Title sits at the top, table in the middle,
        // and the footer occupies the final line.
        let title_height: u16 = 3;
        let content_area = Rect {
            x: area.x,
            y: area.y + title_height,
            width: area.width,
            height: area
                .height
                .saturating_sub(title_height) // remove title
                .saturating_sub(1), // reserve one line for footer
        };
        // apply vertical windowing (scroll). Compute a local start that ensures
        // the selected project is visible without mutating self (render is &self).
        let visible_height = crate::tui::table_visible_rows(content_area.height);
        self.projects_visible_rows = visible_height.max(1);
        let projects_len = project_rows_all.len();
        let start = crate::tui::scroll_offset_for_selection(
            self.selected_project,
            self.projects_offset,
            projects_len,
            self.projects_visible_rows,
            2,
        );
        let end = (start + visible_height).min(projects_len);
        let project_rows = project_rows_all[start..end].iter().cloned();

        let projects_table = Table::new(
            project_rows,
            [Constraint::Length(40), Constraint::Length(24)],
        )
        .header(Row::new(vec![
            Cell::from("Project ").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("SAP Number ").style(Style::default().add_modifier(Modifier::BOLD)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .padding(Padding::horizontal(1)),
        );
        if matches!(self.view_mode, ProjectsViewMode::ProjectsList) {
            let left = " PROJECTS";
            let hints = "a=add | e=edit | d=delete ";
            // title_block uses Padding::horizontal(0) so content width = area.width - 2 (borders)
            let inner_width = area.width.saturating_sub(2) as usize;
            let left_len = left.chars().count();
            let hints_len = hints.chars().count();
            let gap = if inner_width > left_len + hints_len {
                inner_width - left_len - hints_len
            } else {
                1
            };
            let mut title_line = format!("{}{}{}", left, " ".repeat(gap), hints);
            // render title box (owns bottom border)
            // reduce left padding to avoid extra leading space
            let title_block = Block::default()
                .borders(Borders::ALL)
                .padding(Padding::horizontal(0));
            // Ensure exactly one trailing space remains
            title_line = title_line.trim_end_matches(' ').to_string();
            title_line.push(' ');
            frame.render_widget(
                ratatui::widgets::Paragraph::new(title_line).block(title_block),
                Rect {
                    x: area.x,
                    y: area.y,
                    width: area.width,
                    height: title_height,
                },
            );
            // render table with no top border so seam is drawn only once by title box
            let tbl_block = Block::default()
                .borders(Borders::LEFT | Borders::RIGHT | Borders::BOTTOM)
                .padding(Padding::horizontal(1));
            frame.render_widget(projects_table.block(tbl_block), content_area);
        }

        let selected_project_id = projects.get(self.selected_project).map(|p| p.id);
        let rules = selected_project_id
            .and_then(|id| db::rules_for_project(conn, id).ok())
            .unwrap_or_default();
        let rule_rows_all: Vec<_> = rules
            .iter()
            .enumerate()
            .map(|(idx, r)| {
                let style = if idx == self.selected_rule {
                    // use same selection color as projects
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default()
                };
                Row::new(vec![
                    Cell::from(r.app_id.clone().unwrap_or_default()),
                    Cell::from(r.name_regex.clone()),
                    Cell::from(r.precedence.to_string()),
                ])
                .style(style)
            })
            .collect();
        let rules_len = rule_rows_all.len();
        let visible_rules_height = crate::tui::table_visible_rows(content_area.height);
        self.rules_visible_rows = visible_rules_height.max(1);
        let start_r = crate::tui::scroll_offset_for_selection(
            self.selected_rule,
            self.rules_offset,
            rules_len,
            self.rules_visible_rows,
            2,
        );
        let end_r = (start_r + visible_rules_height).min(rules_len);
        let rule_rows = rule_rows_all[start_r..end_r].iter().cloned();
        let rules_table = Table::new(
            rule_rows,
            [
                Constraint::Length(16),
                Constraint::Length(60),
                Constraint::Length(10),
            ],
        )
        .header(Row::new(vec![
            Cell::from("app_id ").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("name_regex ").style(Style::default().add_modifier(Modifier::BOLD)),
            Cell::from("prec ").style(Style::default().add_modifier(Modifier::BOLD)),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .padding(Padding::horizontal(1)),
        );
        if matches!(self.view_mode, ProjectsViewMode::RulesOnly) {
            let left = " RULES";
            let hints = "a=add | e=edit | d=delete ";
            let inner_width = area.width.saturating_sub(2) as usize;
            let left_len = left.chars().count();
            let hints_len = hints.chars().count();
            let gap = if inner_width > left_len + hints_len {
                inner_width - left_len - hints_len
            } else {
                1
            };
            let mut title_line = format!("{}{}{}", left, " ".repeat(gap), hints);
            // Ensure exactly one trailing space remains
            title_line = title_line.trim_end_matches(' ').to_string();
            title_line.push(' ');
            let title_block = Block::default()
                .borders(Borders::ALL)
                .padding(Padding::horizontal(1));
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
            frame.render_widget(rules_table.block(tbl_block), content_area);
        }

        if let Some(modal) = &self.modal {
            let mw = (area.width / 2).min(90).max(44);
            let desired_mh = match modal {
                ProjectsModal::Project(_) => 6u16,
                ProjectsModal::Rule(_) => 7u16,
                ProjectsModal::Confirm(_) => 6u16,
            };
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

            let (title, text) = match modal {
                ProjectsModal::Project(m) => {
                    let rows = vec![
                        format!(
                            "Name : {}{}",
                            if m.field_idx == 0 { "> " } else { "  " },
                            m.name
                        ),
                        format!(
                            "SAP  : {}{}",
                            if m.field_idx == 1 { "> " } else { "  " },
                            m.sap
                        ),
                        "".to_string(),
                        format!(
                            "{}OK   {}CANCEL",
                            if m.field_idx == 2 { "> " } else { "  " },
                            if m.field_idx == 3 { "> " } else { "  " }
                        ),
                    ];
                    let title = match m.mode {
                        ProjectModalMode::Add => "Add project",
                        ProjectModalMode::Edit => "Edit project",
                    };
                    (title.to_string(), rows.join("\n"))
                }
                ProjectsModal::Rule(m) => {
                    let rows = vec![
                        format!(
                            "app_id: {}{}",
                            if m.field_idx == 0 { "> " } else { "  " },
                            m.app_id
                        ),
                        format!(
                            "regex : {}{}",
                            if m.field_idx == 1 { "> " } else { "  " },
                            m.name_regex
                        ),
                        format!(
                            "prec  : {}{}",
                            if m.field_idx == 2 { "> " } else { "  " },
                            m.precedence
                        ),
                        "".to_string(),
                        format!(
                            "{}OK   {}CANCEL",
                            if m.field_idx == 3 { "> " } else { "  " },
                            if m.field_idx == 4 { "> " } else { "  " }
                        ),
                    ];
                    let title = match m.mode {
                        RuleModalMode::Add => "Add rule",
                        RuleModalMode::Edit => "Edit rule",
                    };
                    (title.to_string(), rows.join("\n"))
                }
                ProjectsModal::Confirm(m) => {
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

    }

    pub fn handle_key(&mut self, key: KeyEvent, conn: &rusqlite::Connection) -> Result<bool> {
        if self.modal.is_some() {
            let modal = self.modal.take().expect("modal present");
            let (next_modal, changed) = self.handle_modal_key_consuming(key, modal, conn)?;
            self.modal = next_modal;
            return Ok(changed);
        }

        let mut projects = db::projects(conn)?;
        if projects.is_empty() {
            if matches!(key.code, KeyCode::Char('a')) {
                self.modal = Some(ProjectsModal::Project(ProjectModal::new_add()));
                return Ok(false);
            }
            return Ok(false);
        }

        self.selected_project = self.selected_project.min(projects.len().saturating_sub(1));
        let selected = projects[self.selected_project].clone();

        match key.code {
            KeyCode::Char('j') => {
                if matches!(self.view_mode, ProjectsViewMode::ProjectsList) {
                    self.selected_project = (self.selected_project + 1).min(projects.len() - 1);
                    self.selected_rule = 0;
                    self.projects_offset = crate::tui::scroll_offset_for_selection(
                        self.selected_project,
                        self.projects_offset,
                        projects.len(),
                        self.projects_visible_rows,
                        2,
                    );
                } else {
                    let rules = db::rules_for_project(conn, projects[self.selected_project].id)?;
                    if !rules.is_empty() {
                        self.selected_rule = (self.selected_rule + 1).min(rules.len() - 1);
                        self.rules_offset = crate::tui::scroll_offset_for_selection(
                            self.selected_rule,
                            self.rules_offset,
                            rules.len(),
                            self.rules_visible_rows,
                            2,
                        );
                    }
                }
            }
            KeyCode::Char('k') => {
                if matches!(self.view_mode, ProjectsViewMode::ProjectsList) {
                    self.selected_project = self.selected_project.saturating_sub(1);
                    self.selected_rule = 0;
                    self.projects_offset = crate::tui::scroll_offset_for_selection(
                        self.selected_project,
                        self.projects_offset,
                        projects.len(),
                        self.projects_visible_rows,
                        2,
                    );
                } else {
                    self.selected_rule = self.selected_rule.saturating_sub(1);
                    let rules = db::rules_for_project(conn, projects[self.selected_project].id)?;
                    self.rules_offset = crate::tui::scroll_offset_for_selection(
                        self.selected_rule,
                        self.rules_offset,
                        rules.len(),
                        self.rules_visible_rows,
                        2,
                    );
                }
            }
            KeyCode::Char('a') => {
                if matches!(self.view_mode, ProjectsViewMode::ProjectsList) {
                    self.modal = Some(ProjectsModal::Project(ProjectModal::new_add()));
                    return Ok(false);
                } else {
                    let selected = projects[self.selected_project].clone();
                    self.modal = Some(ProjectsModal::Rule(RuleModal::new_add(selected.id)));
                    return Ok(false);
                }
            }
            KeyCode::Char('e') => {
                if matches!(self.view_mode, ProjectsViewMode::ProjectsList) {
                    self.modal = Some(ProjectsModal::Project(ProjectModal::new_edit(
                        selected.id,
                        selected.name.clone(),
                        selected.sap_number.clone().unwrap_or_default(),
                    )));
                    return Ok(false);
                } else {
                    let rules = db::rules_for_project(conn, selected.id)?;
                    if let Some(rule) = rules.get(self.selected_rule) {
                        self.modal = Some(ProjectsModal::Rule(RuleModal::new_edit(
                            selected.id,
                            rule.id,
                            rule.app_id.clone().unwrap_or_default(),
                            rule.name_regex.clone(),
                            rule.precedence,
                        )));
                        return Ok(false);
                    }
                }
            }
            KeyCode::Char('d') => {
                if matches!(self.view_mode, ProjectsViewMode::ProjectsList) {
                    self.modal = Some(ProjectsModal::Confirm(ConfirmModal::delete_project(
                        selected.id,
                    )));
                    return Ok(false);
                } else {
                    let rules = db::rules_for_project(conn, selected.id)?;
                    if let Some(rule) = rules.get(self.selected_rule) {
                        self.modal =
                            Some(ProjectsModal::Confirm(ConfirmModal::delete_rule(rule.id)));
                        return Ok(false);
                    }
                }
            }
            // Note: 'a' handled above (add project or add rule depending on view)
            // removed: u/m/t shortcuts (no-op)
            KeyCode::Down => {
                if matches!(self.view_mode, ProjectsViewMode::ProjectsList) {
                    self.selected_project = (self.selected_project + 1).min(projects.len() - 1);
                    self.selected_rule = 0;
                    self.projects_offset = crate::tui::scroll_offset_for_selection(
                        self.selected_project,
                        self.projects_offset,
                        projects.len(),
                        self.projects_visible_rows,
                        2,
                    );
                } else {
                    let rules = db::rules_for_project(conn, selected.id)?;
                    if !rules.is_empty() {
                        self.selected_rule = (self.selected_rule + 1).min(rules.len() - 1);
                        self.rules_offset = crate::tui::scroll_offset_for_selection(
                            self.selected_rule,
                            self.rules_offset,
                            rules.len(),
                            self.rules_visible_rows,
                            2,
                        );
                    }
                }
            }
            KeyCode::Up => {
                if matches!(self.view_mode, ProjectsViewMode::ProjectsList) {
                    self.selected_project = self.selected_project.saturating_sub(1);
                    self.selected_rule = 0;
                    self.projects_offset = crate::tui::scroll_offset_for_selection(
                        self.selected_project,
                        self.projects_offset,
                        projects.len(),
                        self.projects_visible_rows,
                        2,
                    );
                } else {
                    self.selected_rule = self.selected_rule.saturating_sub(1);
                    let rules = db::rules_for_project(conn, selected.id)?;
                    self.rules_offset = crate::tui::scroll_offset_for_selection(
                        self.selected_rule,
                        self.rules_offset,
                        rules.len(),
                        self.rules_visible_rows,
                        2,
                    );
                }
            }
            KeyCode::Enter => {
                // show rules-only view for selected project
                self.view_mode = ProjectsViewMode::RulesOnly;
                self.selected_rule = 0;
                return Ok(false);
            }
            KeyCode::Esc => {
                if matches!(self.view_mode, ProjectsViewMode::RulesOnly) {
                    self.view_mode = ProjectsViewMode::ProjectsList;
                    return Ok(false);
                }
            }
            _ => {}
        }
        projects.shrink_to_fit();
        Ok(false)
    }

    fn handle_modal_key_consuming(
        &mut self,
        key: KeyEvent,
        mut modal: ProjectsModal,
        conn: &rusqlite::Connection,
    ) -> Result<(Option<ProjectsModal>, bool)> {
        match &mut modal {
            ProjectsModal::Project(m) => match key.code {
                KeyCode::Char(c) => {
                    if m.field_idx == 0 {
                        m.name.push(c);
                    } else if m.field_idx == 1 {
                        m.sap.push(c);
                    }
                    return Ok((Some(modal), false));
                }
                KeyCode::Backspace => {
                    if m.field_idx == 0 {
                        m.name.pop();
                    } else if m.field_idx == 1 {
                        m.sap.pop();
                    }
                    return Ok((Some(modal), false));
                }
                KeyCode::Tab | KeyCode::Down => {
                    m.field_idx = (m.field_idx + 1) % 4;
                    return Ok((Some(modal), false));
                }
                KeyCode::Up => {
                    m.field_idx = if m.field_idx == 0 { 3 } else { m.field_idx - 1 };
                    return Ok((Some(modal), false));
                }
                KeyCode::Enter => {
                    if m.field_idx == 2 {
                        if m.name.trim().is_empty() {
                            self.message = "project name must not be empty".to_string();
                            return Ok((Some(modal), true));
                        }
                        let sap = if m.sap.trim().is_empty() {
                            None
                        } else {
                            Some(m.sap.as_str())
                        };
                        match m.mode {
                            ProjectModalMode::Add => db::add_project(conn, &m.name, sap)?,
                            ProjectModalMode::Edit => {
                                if let Some(id) = m.editing_id {
                                    db::update_project(conn, id, &m.name, sap)?;
                                }
                            }
                        }
                        self.message = match m.mode {
                            ProjectModalMode::Add => "project added",
                            ProjectModalMode::Edit => "project updated",
                        }
                        .to_string();
                        return Ok((None, true));
                    }
                    if m.field_idx == 3 {
                        self.message = "cancelled".to_string();
                        return Ok((None, true));
                    }
                    return Ok((Some(modal), false));
                }
                KeyCode::Esc => {
                    self.message = "cancelled".to_string();
                    return Ok((None, true));
                }
                _ => return Ok((Some(modal), false)),
            },
            ProjectsModal::Rule(m) => match key.code {
                KeyCode::Char(c) => {
                    if m.field_idx == 0 {
                        m.app_id.push(c);
                    } else if m.field_idx == 1 {
                        m.name_regex.push(c);
                    } else if m.field_idx == 2 {
                        m.precedence.push(c);
                    }
                    return Ok((Some(modal), false));
                }
                KeyCode::Backspace => {
                    if m.field_idx == 0 {
                        m.app_id.pop();
                    } else if m.field_idx == 1 {
                        m.name_regex.pop();
                    } else if m.field_idx == 2 {
                        m.precedence.pop();
                    }
                    return Ok((Some(modal), false));
                }
                KeyCode::Tab | KeyCode::Down => {
                    m.field_idx = (m.field_idx + 1) % 5;
                    return Ok((Some(modal), false));
                }
                KeyCode::Up => {
                    m.field_idx = if m.field_idx == 0 { 4 } else { m.field_idx - 1 };
                    return Ok((Some(modal), false));
                }
                KeyCode::Enter => {
                    if m.field_idx == 3 {
                        Regex::new(&m.name_regex).map_err(|e| anyhow::anyhow!(e.to_string()))?;
                        let precedence = m
                            .precedence
                            .trim()
                            .parse::<i64>()
                            .map_err(|_| anyhow::anyhow!("precedence must be a number"))?;
                        let app_id = if m.app_id.trim().is_empty() {
                            None
                        } else {
                            Some(m.app_id.as_str())
                        };
                        match m.mode {
                            RuleModalMode::Add => {
                                db::add_rule(
                                    conn,
                                    m.project_id,
                                    app_id,
                                    None,
                                    &m.name_regex,
                                    precedence,
                                )?;
                                self.message = "rule added".to_string();
                            }
                            RuleModalMode::Edit => {
                                if let Some(id) = m.editing_id {
                                    db::update_rule(
                                        conn,
                                        id,
                                        app_id,
                                        None,
                                        &m.name_regex,
                                        precedence,
                                    )?;
                                    self.message = "rule updated".to_string();
                                }
                            }
                        }
                        return Ok((None, true));
                    }
                    if m.field_idx == 4 {
                        self.message = "cancelled".to_string();
                        return Ok((None, true));
                    }
                    return Ok((Some(modal), false));
                }
                KeyCode::Esc => {
                    self.message = "cancelled".to_string();
                    return Ok((None, true));
                }
                _ => return Ok((Some(modal), false)),
            },
            ProjectsModal::Confirm(m) => match key.code {
                KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::Up | KeyCode::Down => {
                    m.field_idx = 1usize.saturating_sub(m.field_idx);
                    Ok((Some(modal), false))
                }
                KeyCode::Enter => {
                    if m.field_idx == 0 {
                        match m.kind {
                            ConfirmKind::DeleteProject { project_id } => {
                                db::delete_project(conn, project_id)?;
                                self.selected_project = self.selected_project.saturating_sub(1);
                                self.selected_rule = 0;
                                self.message = "project deleted".to_string();
                            }
                            ConfirmKind::DeleteRule { rule_id } => {
                                db::delete_rule(conn, rule_id)?;
                                self.selected_rule = self.selected_rule.saturating_sub(1);
                                self.message = "rule deleted".to_string();
                            }
                        }
                        Ok((None, true))
                    } else {
                        self.message = "cancelled".to_string();
                        Ok((None, true))
                    }
                }
                KeyCode::Esc => {
                    self.message = "cancelled".to_string();
                    Ok((None, true))
                }
                _ => Ok((Some(modal), false)),
            },
        }
    }
}

// interactive prompt helper removed in favor of in-TUI modals

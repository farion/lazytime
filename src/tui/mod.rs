pub mod current;
pub mod daemon_control;
pub mod jira_sync;
pub mod projects;
pub mod projects_modal;
pub mod trackings;
pub mod trackings_cleanup;
pub mod trackings_modal;
pub mod trackings_modal_actions;
pub mod trackings_rows;
pub mod trackings_storno;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Paragraph;
use std::io;
use std::time::Duration;

use crate::config::Config;
use crate::db;
use crate::ipc::client;

pub(crate) fn table_visible_rows(content_height: u16) -> usize {
    content_height.saturating_sub(2) as usize
}

pub(crate) fn scroll_offset_for_selection(
    selected: usize,
    offset: usize,
    len: usize,
    visible_rows: usize,
    margin: usize,
) -> usize {
    if len == 0 {
        return 0;
    }

    let visible = visible_rows.max(1);
    if len <= visible {
        return 0;
    }

    let max_offset = len - visible;
    let selected = selected.min(len - 1);
    let mut next_offset = offset.min(max_offset);
    let margin = margin.min(visible.saturating_sub(1) / 2);

    let top_trigger = next_offset.saturating_add(margin);
    if selected < top_trigger {
        return selected.saturating_sub(margin).min(max_offset);
    }

    let bottom_trigger = next_offset
        .saturating_add(visible)
        .saturating_sub(1)
        .saturating_sub(margin);
    if selected > bottom_trigger {
        next_offset = selected
            .saturating_add(margin)
            .saturating_add(1)
            .saturating_sub(visible)
            .min(max_offset);
    }

    next_offset
}

pub fn run(config: &Config) -> Result<()> {
    let mut conn = db::open(config.db_path())?;
    db::migrate(&conn)?;

    enable_raw_mode()?;
    // Enter alternate screen to fully own the terminal and avoid showing prior content.
    use crossterm::execute;
    use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen};
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut mode = ViewMode::Current;
    let mut current_state = current::CurrentState::default();
    let mut tracking_state = trackings::TrackingsState::default();
    let mut projects_state = projects::ProjectsState::default();
    let mut jira_sync_state = jira_sync::JiraSyncState::default();
    let mut daemon_control_state = daemon_control::DaemonControlState::default();
    daemon_control_state.auto_start_on_tui_launch(config)?;

    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(1)])
                .split(f.area());

            // Render exactly three lines: one blank line, the title line, one blank line.
            // Title has 1-space padding left and right as requested.
            let left = " LazyTime TUI";
            let hints = "c=current | t=trackings | p=projects | j=jira | o=daemon | q=quit";
            let inner_width = chunks[0].width.saturating_sub(2) as usize; // allow for minimal padding
            let left_len = left.chars().count();
            let hints_len = hints.chars().count();
            let gap = if inner_width > left_len + hints_len {
                inner_width - left_len - hints_len
            } else {
                1
            };
            // Build full title line (non-bold) then overlay only the left label as bold
            let title = format!("{}{}{}", left, " ".repeat(gap), hints);
            // Render full title line (non-bold) centered in the 3-line header area
            let header = Paragraph::new(format!("\n{}\n", title));
            f.render_widget(header, chunks[0]);
            // Overlay only the left label as bold by rendering it into the middle row
            use ratatui::layout::Rect as RRect;
            let left_w = left_len as u16;
            let gap_u = gap as u16;
            let hints_w = hints_len as u16;
            // position the overlay on the middle line (y + 1) with exact widths
            let mid_y = chunks[0].y.saturating_add(1);
            let left_rect = RRect {
                x: chunks[0].x,
                y: mid_y,
                width: left_w.min(chunks[0].width),
                height: 1,
            };
            let left_par =
                Paragraph::new(left).style(Style::default().add_modifier(Modifier::BOLD));
            f.render_widget(left_par, left_rect);
            // Render hints as non-bold at computed x so they remain normal
            let hints_x = chunks[0].x.saturating_add(left_w).saturating_add(gap_u);
            let hints_rect = RRect {
                x: hints_x,
                y: mid_y,
                width: hints_w.min(chunks[0].width.saturating_sub(left_w).saturating_sub(gap_u)),
                height: 1,
            };
            let hints_par = Paragraph::new(hints);
            f.render_widget(hints_par, hints_rect);

            match mode {
                ViewMode::Current => {
                    daemon_control_state.poll(config);
                    current_state.render(f, chunks[1], &conn, daemon_control_state.status);
                }
                ViewMode::Trackings => {
                    tracking_state.render(f, chunks[1], &conn, config);
                }
                ViewMode::Projects => {
                    projects_state.render(f, chunks[1], &conn);
                }
                ViewMode::JiraSync => {
                    jira_sync_state.poll_events();
                    jira_sync_state.render(f, chunks[1]);
                }
                ViewMode::DaemonControl => {
                    daemon_control_state.poll(config);
                    daemon_control_state.render(f, chunks[1]);
                }
            }
        })?;

        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
        {
            let modal_open = match mode {
                ViewMode::Current => current_state.modal.is_some(),
                ViewMode::Trackings => tracking_state.modal.is_some(),
                ViewMode::Projects => projects_state.modal.is_some(),
                ViewMode::JiraSync => false,
                ViewMode::DaemonControl => false,
            };

            if modal_open {
                match mode {
                    ViewMode::Current => {
                        let _ = current_state.handle_key(key, &mut conn, config)?;
                    }
                    ViewMode::Trackings => {
                        let _ = tracking_state.handle_key(key, &conn, config)?;
                    }
                    ViewMode::Projects => {
                        let changed = projects_state.handle_key(key, &conn)?;
                        if changed {
                            let ts = crate::time::format_ts(&chrono::Utc::now());
                            let socket = config.ipc_socket_path();
                            client::notify_projects_updated_blocking(&socket, &ts).ok();
                        }
                    }
                    ViewMode::JiraSync => {
                        let _ = jira_sync_state.handle_key(key, config)?;
                    }
                    ViewMode::DaemonControl => {
                        let _ = daemon_control_state.handle_key(key, config)?;
                    }
                }
                continue;
            }

            match key.code {
                KeyCode::Char('q') => {
                    daemon_control_state.stop_owned_on_exit(config);
                    break;
                }
                KeyCode::Char('c') => mode = ViewMode::Current,
                KeyCode::Char('t') => mode = ViewMode::Trackings,
                KeyCode::Char('p') => mode = ViewMode::Projects,
                KeyCode::Char('j') => mode = ViewMode::JiraSync,
                KeyCode::Char('o') => mode = ViewMode::DaemonControl,
                KeyCode::Char('r') => {
                    let _ = db::migrate(&conn);
                }
                _ => match mode {
                    ViewMode::Current => {
                        let _ = current_state.handle_key(key, &mut conn, config)?;
                    }
                    ViewMode::Trackings => {
                        let _ = tracking_state.handle_key(key, &conn, config)?;
                    }
                    ViewMode::Projects => {
                        let changed = projects_state.handle_key(key, &conn)?;
                        if changed {
                            let ts = crate::time::format_ts(&chrono::Utc::now());
                            let socket = config.ipc_socket_path();
                            client::notify_projects_updated_blocking(&socket, &ts).ok();
                        }
                    }
                    ViewMode::JiraSync => {
                        let _ = jira_sync_state.handle_key(key, config)?;
                    }
                    ViewMode::DaemonControl => {
                        let _ = daemon_control_state.handle_key(key, config)?;
                    }
                },
            }
        }
    }

    daemon_control_state.stop_owned_on_exit(config);

    // Restore terminal state
    terminal.show_cursor()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    disable_raw_mode()?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum ViewMode {
    Current,
    Trackings,
    Projects,
    JiraSync,
    DaemonControl,
}

#[cfg(test)]
mod tests {
    use super::scroll_offset_for_selection;

    #[test]
    fn no_scroll_when_all_rows_fit() {
        assert_eq!(scroll_offset_for_selection(8, 0, 9, 9, 2), 0);
        assert_eq!(scroll_offset_for_selection(8, 4, 9, 9, 2), 0);
    }

    #[test]
    fn scrolls_near_bottom_with_margin() {
        assert_eq!(scroll_offset_for_selection(7, 0, 20, 10, 2), 0);
        assert_eq!(scroll_offset_for_selection(8, 0, 20, 10, 2), 1);
    }

    #[test]
    fn scrolls_near_top_with_margin() {
        assert_eq!(scroll_offset_for_selection(2, 5, 20, 10, 2), 0);
        assert_eq!(scroll_offset_for_selection(1, 5, 20, 10, 2), 0);
    }
}

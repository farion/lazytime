use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

pub fn render_global_statusbar(
    frame: &mut Frame<'_>,
    area: Rect,
    left_message: &str,
    backend_name: &str,
    daemon_state: &str,
) {
    let total_width = area.width as usize;
    if total_width == 0 {
        return;
    }
    if total_width == 1 {
        frame.render_widget(Paragraph::new(" "), area);
        return;
    }

    let inner_width = total_width.saturating_sub(2);
    let right = format!("backend: {} | daemon: {}", backend_name, daemon_state);
    let right_len = right.chars().count();
    let available_for_left = inner_width.saturating_sub(right_len + 1);
    let left = trim_to_width(left_message, available_for_left);
    let left_len = left.chars().count();
    let gap = inner_width.saturating_sub(left_len + right_len).max(1);

    let line = Line::from(vec![
        Span::raw(" "),
        Span::raw(left),
        Span::raw(" ".repeat(gap)),
        Span::styled(right, Style::default().fg(Color::DarkGray)),
        Span::raw(" "),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn trim_to_width(input: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    input.chars().take(width).collect()
}

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub fn render_incoming_prompt(f: &mut Frame, app: &AppState, area: Rect) {
    let popup_area = centered_rect(60, 45, area);
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        .title(" INCOMING FILE TRANSFER OFFER ");

    let (file_name, file_size_str, sender_str) = if let Some(ref info) = app.incoming_prompt {
        (
            info.file_name.as_str(),
            format!("{:.2} MB ({} bytes)", info.file_size as f64 / (1024.0 * 1024.0), info.file_size),
            info.sender_name.as_str(),
        )
    } else {
        ("sample_video.mkv", "1250.50 MB (1311234000 bytes)".to_string(), "OnePlus 13s (Android)")
    };

    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Sender Device: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(sender_str, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  Incoming File: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(file_name, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  File Size:     ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(file_size_str, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(""),
        Line::from(Span::styled("  Do you want to accept and download this file?", Style::default().fg(Color::White))),
        Line::from(""),
        Line::from(vec![
            Span::styled("   [Enter] ACCEPT & RECEIVE   ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled("   [Esc] REJECT OFFER   ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        ]),
    ];

    let para = Paragraph::new(text).block(block).alignment(Alignment::Center);
    f.render_widget(para, popup_area);
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

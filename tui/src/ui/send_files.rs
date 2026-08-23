use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub const SEND_OPTIONS: [(&str, &str, &str); 3] = [
    ("Browse Filesystem", "Open interactive directory explorer to select files", "B"),
    ("Enter File Path", "Type or paste an exact absolute path directly into console", "P"),
    ("Recent Files", "Select from recently sent files and historical transfers", "R"),
];

pub fn render_send_files(f: &mut Frame, app: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header status
            Constraint::Length(10), // Options list
            Constraint::Min(6),    // Selected File Summary
        ])
        .split(area);

    // Header summary
    let header_text = Line::from(vec![
        Span::styled(" SEND FILES ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled("— Select a source file to transmit to a peer device", Style::default().fg(Color::Gray)),
    ]);
    let header_para = Paragraph::new(header_text).alignment(Alignment::Center);
    f.render_widget(header_para, chunks[0]);

    // Options list
    let option_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(3); SEND_OPTIONS.len()])
        .split(chunks[1]);

    for (i, (title, desc, hotkey)) in SEND_OPTIONS.iter().enumerate() {
        let is_selected = i == app.selected_index;
        let border_style = if is_selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(if is_selected { BorderType::Double } else { BorderType::Plain })
            .border_style(border_style);

        let key_badge = Span::styled(format!(" [{}] ", hotkey), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        let title_span = Span::styled(
            *title,
            if is_selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            },
        );
        let desc_span = Span::styled(format!("  — {}", desc), Style::default().fg(Color::DarkGray));

        let para = Paragraph::new(Line::from(vec![key_badge, title_span, desc_span])).block(block);
        f.render_widget(para, option_chunks[i]);
    }

    // Selected File preview box
    let file_info_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Selected File ");

    let mut preview_lines = Vec::new();
    if let Some(ref path) = app.selected_file_path {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
        let size_str = if let Ok(meta) = std::fs::metadata(path) {
            format!("{:.2} MB ({} bytes)", meta.len() as f64 / (1024.0 * 1024.0), meta.len())
        } else {
            "Unknown size".to_string()
        };

        preview_lines.push(Line::from(vec![
            Span::styled("  File Name: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(file_name, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]));
        preview_lines.push(Line::from(vec![
            Span::styled("  File Path: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(path.display().to_string(), Style::default().fg(Color::Cyan)),
        ]));
        preview_lines.push(Line::from(vec![
            Span::styled("  File Size: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(size_str, Style::default().fg(Color::Yellow)),
        ]));
        preview_lines.push(Line::from(""));
        preview_lines.push(Line::from(Span::styled("  ► Press [Enter] to proceed to Device Selection", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))));
    } else {
        preview_lines.push(Line::from(""));
        preview_lines.push(Line::from(Span::styled("  No file selected yet. Choose an option above to select a file.", Style::default().fg(Color::DarkGray))));
    }

    let preview_para = Paragraph::new(preview_lines).block(file_info_block);
    f.render_widget(preview_para, chunks[2]);
}

use ratatui::layout::{Constraint, Direction, Layout, Rect};
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
            Constraint::Length(7), // Options list in one clean block
            Constraint::Min(6),    // Selected File Summary
        ])
        .split(area);

    // Options Block (Clean list, no nested border soup)
    let options_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Select Source Method ");

    let mut option_lines = Vec::new();
    option_lines.push(Line::from(""));

    for (i, (title, desc, hotkey)) in SEND_OPTIONS.iter().enumerate() {
        let is_selected = i == app.selected_index;

        let (cursor, key_style, title_style, desc_style, bg_style) = if is_selected {
            (
                " ❯ ",
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                Style::default().fg(Color::Gray),
                Style::default().bg(Color::Rgb(35, 38, 48)),
            )
        } else {
            (
                "   ",
                Style::default().fg(Color::DarkGray),
                Style::default().fg(Color::Gray),
                Style::default().fg(Color::DarkGray),
                Style::default(),
            )
        };

        let line = Line::from(vec![
            Span::styled(cursor, if is_selected { Style::default().fg(Color::White).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::DarkGray) }),
            Span::styled(format!("[{}] ", hotkey), key_style),
            Span::styled(format!("{:<24}", title), title_style),
            Span::styled(format!(" {}", desc), desc_style),
        ]).style(bg_style);

        option_lines.push(line);
    }

    let options_para = Paragraph::new(option_lines).block(options_block);
    f.render_widget(options_para, chunks[0]);

    // Selected File preview box
    let file_info_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Selected Payload ");

    let mut preview_lines = Vec::new();
    if let Some(ref path) = app.selected_file_path {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
        let size_str = if let Ok(meta) = std::fs::metadata(path) {
            if meta.is_dir() {
                "Directory".to_string()
            } else {
                format!("{:.2} MB ({} bytes)", meta.len() as f64 / (1024.0 * 1024.0), meta.len())
            }
        } else {
            "Unknown size".to_string()
        };

        preview_lines.push(Line::from(""));
        preview_lines.push(Line::from(vec![
            Span::styled("   Target Name:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(file_name, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]));
        preview_lines.push(Line::from(vec![
            Span::styled("   Full Path:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(path.display().to_string(), Style::default().fg(Color::White)),
        ]));
        preview_lines.push(Line::from(vec![
            Span::styled("   Payload Size: ", Style::default().fg(Color::DarkGray)),
            Span::styled(size_str, Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]));
        preview_lines.push(Line::from(""));
        preview_lines.push(Line::from(Span::styled("   ► Press [Enter] to proceed to Device Selection", Style::default().fg(Color::White).add_modifier(Modifier::BOLD))));
    } else {
        preview_lines.push(Line::from(""));
        preview_lines.push(Line::from(Span::styled("   No file selected yet. Choose an option above or press [B] to browse files.", Style::default().fg(Color::DarkGray))));
    }

    let preview_para = Paragraph::new(preview_lines).block(file_info_block);
    f.render_widget(preview_para, chunks[1]);
}

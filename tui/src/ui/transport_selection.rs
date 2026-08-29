use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub const TRANSPORTS: [(&str, &str, &str); 4] = [
    ("1. Automatic (Fastest Multipath)", "Dynamic scheduler balancing all available links", "~50+ MB/s"),
    ("2. Combined (USB + 5 GHz Wi-Fi)", "Aggregates USB ADB tunnel and 5 GHz P2P Wi-Fi Direct", "~45–54 MB/s"),
    ("3. USB Only (ADB Tunnel)", "Direct zero-config wired transfer over USB cable", "~8–11 MB/s"),
    ("4. Wi-Fi Direct Only (5 GHz P2P)", "Pure wireless transfer between phone and PC", "~35–38 MB/s"),
];

pub fn render_transport_selection(f: &mut Frame, app: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9), // Transport options list in one clean block
            Constraint::Min(4),    // Confirmation summary
        ])
        .split(area);

    // Transport Option Block
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Select Transport Layer ");

    let mut lines = Vec::new();
    lines.push(Line::from(""));

    for (i, (title, desc, speed)) in TRANSPORTS.iter().enumerate() {
        let is_selected = i == app.selected_transport_index;

        let (cursor, title_style, speed_style, desc_style, bg_style) = if is_selected {
            (
                " ❯ ",
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                Style::default().fg(Color::Gray),
                Style::default().bg(Color::Rgb(35, 38, 48)),
            )
        } else {
            (
                "   ",
                Style::default().fg(Color::Gray),
                Style::default().fg(Color::DarkGray),
                Style::default().fg(Color::DarkGray),
                Style::default(),
            )
        };

        let line = Line::from(vec![
            Span::styled(cursor, if is_selected { Style::default().fg(Color::White).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::DarkGray) }),
            Span::styled(format!("{:<36}", title), title_style),
            Span::styled(format!(" [{}] ", speed), speed_style),
            Span::styled(format!(" — {}", desc), desc_style),
        ]).style(bg_style);

        lines.push(line);
    }

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, chunks[0]);

    // Transfer initiation summary
    let target_dev = app
        .cached_devices
        .get(app.selected_device_index)
        .map(|d| d.device_name.as_str())
        .unwrap_or("Default Endpoint");

    let file_str = app
        .selected_file_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "No file selected".to_string());

    let summary_lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("   Payload: ", Style::default().fg(Color::DarkGray)),
            Span::styled(file_str, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("  │  Target: ", Style::default().fg(Color::DarkGray)),
            Span::styled(target_dev, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(Span::styled("   ► Press [Enter] to START TRANSFER", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))),
    ];
    let summary_para = Paragraph::new(summary_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Confirmation "),
    );
    f.render_widget(summary_para, chunks[1]);
}

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub const TRANSPORTS: [(&str, &str, &str); 4] = [
    ("1. Automatic (Fastest Multipath)", "Dynamic scheduler balancing all available physical connections", "~50+ MB/s"),
    ("2. Combined (USB + 5 GHz Wi-Fi)", "Aggregates USB ADB tunnel and 5 GHz P2P Wi-Fi Direct simultaneously", "~45–54 MB/s"),
    ("3. USB Only (ADB Tunnel)", "Direct zero-config wired transfer over USB cable", "~8–11 MB/s"),
    ("4. Wi-Fi Direct Only (5 GHz P2P)", "Pure wireless transfer directly connecting Android and PC", "~35–38 MB/s"),
];

pub fn render_transport_selection(f: &mut Frame, app: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Length(14), // Transport options list
            Constraint::Min(4),     // Summary
        ])
        .split(area);

    // Header
    let header_text = Line::from(vec![
        Span::styled(" SELECT TRANSPORT LAYER ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled("— Choose physical transmission path", Style::default().fg(Color::Gray)),
    ]);
    let header_para = Paragraph::new(header_text).alignment(Alignment::Center);
    f.render_widget(header_para, chunks[0]);

    // Transport Option Cards
    let card_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(3); TRANSPORTS.len()])
        .split(chunks[1]);

    for (i, (title, desc, speed)) in TRANSPORTS.iter().enumerate() {
        let is_selected = i == app.selected_transport_index;
        let border_style = if is_selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(if is_selected { BorderType::Double } else { BorderType::Plain })
            .border_style(border_style);

        let title_span = Span::styled(
            format!("{:<36}", title),
            if is_selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            },
        );
        let speed_badge = Span::styled(format!(" [Est: {}] ", speed), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));
        let desc_span = Span::styled(format!("  — {}", desc), Style::default().fg(Color::DarkGray));

        let line = Line::from(vec![title_span, speed_badge, desc_span]);
        let para = Paragraph::new(line).block(block);
        f.render_widget(para, card_chunks[i]);
    }

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
        Line::from(vec![
            Span::styled(" Ready to Transmit: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(file_str, Style::default().fg(Color::Cyan)),
            Span::styled("  ► Target: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(target_dev, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(""),
        Line::from(Span::styled(" ► Press [Enter] to START TRANSFER via Transfer API start_transfer()", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))),
    ];
    let summary_para = Paragraph::new(summary_lines).block(Block::default().borders(Borders::ALL).title(" Confirmation "));
    f.render_widget(summary_para, chunks[2]);
}

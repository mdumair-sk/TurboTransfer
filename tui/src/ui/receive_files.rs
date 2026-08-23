use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub fn render_receive_files(f: &mut Frame, app: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Receiver Status Box
        ])
        .split(area);

    // Header
    let header_text = Line::from(vec![
        Span::styled(" RECEIVE FILES MODE ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled("— Active listener on port 9876 (Transfer API enter_receive_mode())", Style::default().fg(Color::Gray)),
    ]);
    let header_para = Paragraph::new(header_text).alignment(Alignment::Center);
    f.render_widget(header_para, chunks[0]);

    // Status Body
    let body_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Receiver Service Status ");

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("   Status: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(" ● LISTENING FOR INCOMING TRANSFERS ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("   Target Destination: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(&app.settings.download_dir, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("   Listening Ports:    ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("0.0.0.0:9876 (Wi-Fi Direct) / 127.0.0.1:9876 (ADB Forward/Reverse)", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("   Protocol Framing:   ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Binary Length-Prefixed [4B Len][1B Type][Payload] (TRD §6.1)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(Span::styled("   Waiting for a sender device to initiate TransferOffer...", Style::default().fg(Color::Gray))),
        Line::from(Span::styled("   When an offer arrives, an accept prompt will appear automatically.", Style::default().fg(Color::DarkGray))),
    ];

    let body_para = Paragraph::new(lines).block(body_block);
    f.render_widget(body_para, chunks[1]);
}

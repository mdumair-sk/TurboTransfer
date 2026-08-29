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
        Span::styled(" RECEIVE FILES MODE ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("│ Active high-speed listener on port 9876", Style::default().fg(Color::DarkGray)),
    ]);
    let header_para = Paragraph::new(header_text)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    f.render_widget(header_para, chunks[0]);

    // Status Body
    let body_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Receiver Service Status ");

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("   Status:             ", Style::default().fg(Color::DarkGray)),
            Span::styled("● LISTENING FOR INCOMING TRANSFERS", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("   Download Folder:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(&app.settings.download_dir, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("   Listening Ports:    ", Style::default().fg(Color::DarkGray)),
            Span::styled("0.0.0.0:9876 (Wi-Fi Direct) │ 127.0.0.1:9876 (ADB Tunnel)", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("   Framing & Checksum: ", Style::default().fg(Color::DarkGray)),
            Span::styled("Binary Length-Prefixed [4B Len][1B Type][Payload] (xxHash64 / CRC32c)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(Span::styled("   Waiting for a remote peer to initiate a transfer offer...", Style::default().fg(Color::Gray))),
        Line::from(Span::styled("   When an incoming file arrives, an acceptance dialog will pop up automatically.", Style::default().fg(Color::DarkGray))),
    ];

    let body_para = Paragraph::new(lines).block(body_block);
    f.render_widget(body_para, chunks[1]);
}

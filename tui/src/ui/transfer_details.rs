use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub fn render_transfer_details(f: &mut Frame, app: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(12),   // Details grid
        ])
        .split(area);

    // Header
    let header_text = Line::from(vec![
        Span::styled(" TRANSFER DIAGNOSTICS & DETAILS ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("│ In-depth chunk engine & scheduler health", Style::default().fg(Color::DarkGray)),
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

    // Grid content
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Detailed State Metrics ");

    let (tot_chunks, comp_chunks, retries, usb_err, wifi_err) = if let Some(ref p) = app.active_progress {
        (p.total_chunks, p.completed_chunks, p.retry_count, p.usb_errors, p.wifi_errors)
    } else {
        (1, 0, 0, 0, 0)
    };

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("   Chunk Allocation:       ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} / {} chunks completed ({} remaining)", comp_chunks, tot_chunks, tot_chunks.saturating_sub(comp_chunks)),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("   Chunk Size:             ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} MiB ({} bytes per chunk)", app.settings.chunk_size_mib, (app.settings.chunk_size_mib as usize * 1024 * 1024)),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("   NACK / Retry Count:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} retried chunks", retries),
                Style::default().fg(if retries > 0 { Color::Yellow } else { Color::Green }),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("   USB Disconnects/Errors: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} errors", usb_err),
                Style::default().fg(if usb_err > 0 { Color::Red } else { Color::Green }),
            ),
            Span::styled("  (ADB auto-reconnect: 2.0s poll)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("   Wi-Fi Dropouts/Errors:  ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} errors", wifi_err),
                Style::default().fg(if wifi_err > 0 { Color::Red } else { Color::Green }),
            ),
            Span::styled("  (P2P heartbeat timeout: 15.0s)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("   Integrity Checksums:    ", Style::default().fg(Color::DarkGray)),
            Span::styled("xxHash64 (Per-chunk frame)  │  CRC32c (Pre-rename verify)", Style::default().fg(Color::White)),
        ]),
        Line::from(vec![
            Span::styled("   Metadata Persistence:   ", Style::default().fg(Color::DarkGray)),
            Span::styled("MetaActor async flush (250ms batch threshold)", Style::default().fg(Color::DarkGray)),
        ]),
    ];

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, chunks[1]);
}

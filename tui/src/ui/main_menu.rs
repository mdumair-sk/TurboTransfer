use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub const MAIN_MENU_ITEMS: [(&str, &str, &str); 6] = [
    ("1. Send Files", "Transmit files to nearby devices over high-speed USB/Wi-Fi Direct", "1"),
    ("2. Receive Files", "Listen for incoming transfers and save files to downloads folder", "2"),
    ("3. Devices", "Discover and pair with local Android phones and Windows PCs", "3"),
    ("4. Transfers", "Monitor active transfers, resume interrupted jobs, view history", "4"),
    ("5. Benchmark", "Measure raw transport throughput across ADB and 5 GHz Wi-Fi", "5"),
    ("6. Settings", "Configure chunk size, buffer pools, transport priority, and P2P band", "6"),
];

pub fn render_main_menu(f: &mut Frame, app: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(3),
        ])
        .split(area);

    // Title / Subtitle
    let title_para = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("TURBOTRANSFER ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled("v0.1.0-mvp", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(Span::styled("Multipath High-Speed Transfer Engine (USB 3.0 + 5GHz Wi-Fi Direct)", Style::default().fg(Color::Gray))),
    ])
    .alignment(Alignment::Center);
    f.render_widget(title_para, chunks[0]);

    // Menu Cards Layout
    let item_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(3); MAIN_MENU_ITEMS.len()])
        .split(chunks[1]);

    for (i, (title, desc, key)) in MAIN_MENU_ITEMS.iter().enumerate() {
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

        let key_badge = Span::styled(format!(" [{}] ", key), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
        let title_span = Span::styled(
            *title,
            if is_selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            },
        );
        let desc_span = Span::styled(format!("  — {}", desc), Style::default().fg(Color::DarkGray));

        let line = Line::from(vec![key_badge, title_span, desc_span]);
        let para = Paragraph::new(line).block(block);
        f.render_widget(para, item_chunks[i]);
    }
}

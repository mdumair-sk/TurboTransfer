use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub fn render_devices(f: &mut Frame, app: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Device table
            Constraint::Length(3), // Footer hints
        ])
        .split(area);

    // Header
    let header_text = Line::from(vec![
        Span::styled(" DISCOVERED DEVICES ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("│ Active USB ADB tunnels, 5 GHz Wi-Fi Direct peers & local endpoints", Style::default().fg(Color::DarkGray)),
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

    // Table Block
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Peer Devices List ");

    let mut lines = Vec::new();
    lines.push(Line::from(""));

    // Table Header
    lines.push(Line::from(vec![
        Span::styled("   ", Style::default()),
        Span::styled(format!("{:<28}", "DEVICE NAME"), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(format!("{:<20}", "TRANSPORT"), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(format!("{:<16}", "STATUS"), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("DEVICE ID / ADDRESS", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(Span::styled("   ───────────────────────────────────────────────────────────────────────────────────", Style::default().fg(Color::DarkGray))));

    if app.cached_devices.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("   No devices currently detected. Connect USB cable or turn on Wi-Fi hotspot.", Style::default().fg(Color::DarkGray))));
    } else {
        for (i, dev) in app.cached_devices.iter().enumerate() {
            let is_selected = i == app.selected_device_index;
            let prefix = if is_selected { " ► " } else { "   " };

            let name_style = if is_selected {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            let status_badge = if dev.is_connected {
                Span::styled(format!("{:<16}", "● CONNECTED"), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(format!("{:<16}", "○ DISCONNECTED"), Style::default().fg(Color::DarkGray))
            };

            lines.push(Line::from(vec![
                Span::styled(
                    prefix,
                    if is_selected {
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
                Span::styled(format!("{:<28}", dev.device_name), name_style),
                Span::styled(format!("{:<20}", dev.transport), Style::default().fg(Color::White)),
                status_badge,
                Span::styled(format!("{}", dev.device_id), Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, chunks[1]);

    // Footer
    let footer_text = vec![
        Span::styled(" [S / Enter] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("Send File  ", Style::default().fg(Color::Gray)),
        Span::styled(" [R] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("Refresh  ", Style::default().fg(Color::Gray)),
        Span::styled(" [↑/↓] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("Navigate  ", Style::default().fg(Color::Gray)),
        Span::styled(" [Esc] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("Dashboard", Style::default().fg(Color::Gray)),
    ];
    let footer_para = Paragraph::new(Line::from(footer_text))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        );
    f.render_widget(footer_para, chunks[2]);
}

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub fn render_device_selection(f: &mut Frame, app: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(8),    // Device List
            Constraint::Length(3), // Selected File Summary
        ])
        .split(area);

    // Header
    let header_text = Line::from(vec![
        Span::styled(" SELECT TARGET DEVICE ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled("— Choose the destination Android phone or Windows PC", Style::default().fg(Color::Gray)),
    ]);
    let header_para = Paragraph::new(header_text).alignment(Alignment::Center);
    f.render_widget(header_para, chunks[0]);

    // Device List Block
    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Discovered Devices (via Transfer API get_devices()) ");

    let mut lines = Vec::new();
    lines.push(Line::from(""));

    if app.cached_devices.is_empty() {
        lines.push(Line::from(Span::styled("   No active devices found. Ensure USB debugging or Wi-Fi Direct is active.", Style::default().fg(Color::DarkGray))));
    } else {
        for (i, dev) in app.cached_devices.iter().enumerate() {
            let is_selected = i == app.selected_device_index;
            let prefix = if is_selected { " ► " } else { "   " };

            let name_style = if is_selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let status_badge = if dev.is_connected {
                Span::styled(" [CONNECTED] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" [DISCONNECTED] ", Style::default().fg(Color::Red))
            };

            lines.push(Line::from(vec![
                Span::styled(prefix, if is_selected { Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::DarkGray) }),
                Span::styled(format!("{:<28}", dev.device_name), name_style),
                Span::styled(format!("({:<18}) ", dev.transport), Style::default().fg(Color::Cyan)),
                status_badge,
                Span::styled(format!(" ID: {}", dev.device_id), Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    let list_para = Paragraph::new(lines).block(list_block);
    f.render_widget(list_para, chunks[1]);

    // Selected File bar
    let file_str = app
        .selected_file_path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "No file selected".to_string());

    let file_line = Line::from(vec![
        Span::styled(" Transmitting File: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(file_str, Style::default().fg(Color::Cyan)),
    ]);
    let file_para = Paragraph::new(file_line).block(Block::default().borders(Borders::ALL));
    f.render_widget(file_para, chunks[2]);
}

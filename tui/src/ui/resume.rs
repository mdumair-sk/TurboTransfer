use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use turbotransfer_core::manifest::TransferStatus;

use crate::app::AppState;

pub fn render_resume(f: &mut Frame, app: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Resumable list
        ])
        .split(area);

    // Header
    let header_text = Line::from(vec![
        Span::styled(" COLD RESUME SELECTOR ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("│ Resume interrupted transfers from stored meta.json state", Style::default().fg(Color::DarkGray)),
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

    // Resumable List
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Resumable Incomplete Transfers ");

    let mut lines = Vec::new();
    lines.push(Line::from(""));

    let resumable: Vec<_> = app
        .cached_transfers
        .iter()
        .filter(|t| t.status == TransferStatus::Paused || t.status == TransferStatus::Failed)
        .collect();

    if resumable.is_empty() {
        lines.push(Line::from(Span::styled("   No interrupted transfers found. All transfers have completed or been finalized.", Style::default().fg(Color::DarkGray))));
    } else {
        for (i, t) in resumable.iter().enumerate() {
            let is_selected = i == app.selected_transfer_index;
            let prefix = if is_selected { " ► " } else { "   " };

            let name_style = if is_selected {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            let size_str = format!("{:.2} MB", t.file_size as f64 / (1024.0 * 1024.0));

            lines.push(Line::from(vec![
                Span::styled(
                    prefix,
                    if is_selected {
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
                Span::styled(format!("{:<32}", t.file_name), name_style),
                Span::styled(format!("{:<16}", size_str), Style::default().fg(Color::White)),
                Span::styled(" [RESUMABLE] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::styled(format!(" ID: {}", t.transfer_id), Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, chunks[1]);
}

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Tabs};
use ratatui::Frame;

use turbotransfer_core::manifest::TransferStatus;

use crate::app::{AppState, TransfersTab};

pub fn render_transfers(f: &mut Frame, app: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Sub-Tabs
            Constraint::Min(10),   // Transfer list table
        ])
        .split(area);

    // Sub-Tabs: Current / Resumable / Completed
    let titles: Vec<Line> = TransfersTab::ALL
        .iter()
        .map(|t| {
            let style = if *t == app.transfers_tab {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            Line::from(vec![Span::styled(format!("  {}  ", t.title()), style)])
        })
        .collect();

    let tabs = Tabs::new(titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(" Transfer Category "),
        )
        .select(app.transfers_tab as usize);
    f.render_widget(tabs, chunks[0]);

    // Table Header and items
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(format!(" {} Transfers ({}) ", app.transfers_tab.title(), app.cached_transfers.len()));

    let mut lines = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("   ", Style::default()),
        Span::styled(format!("{:<30}", "FILE NAME"), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(format!("{:<15}", "SIZE"), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(format!("{:<12}", "ROLE"), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(format!("{:<16}", "STATUS"), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("TRANSFER ID", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ]));
    lines.push(Line::from(Span::styled("   ───────────────────────────────────────────────────────────────────────────────────", Style::default().fg(Color::DarkGray))));

    let filtered: Vec<&turbotransfer_core::transfer::TransferSummary> = app
        .cached_transfers
        .iter()
        .filter(|t| match app.transfers_tab {
            TransfersTab::Current => t.status == TransferStatus::InProgress,
            TransfersTab::Resumable => t.status == TransferStatus::Paused || t.status == TransferStatus::Failed,
            TransfersTab::Completed => t.status == TransferStatus::Completed,
        })
        .collect();

    if filtered.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("   No transfer records found in this category.", Style::default().fg(Color::DarkGray))));
    } else {
        for (i, t) in filtered.iter().enumerate() {
            let is_selected = i == app.selected_transfer_index;
            let prefix = if is_selected { " ► " } else { "   " };

            let name_style = if is_selected {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };

            let size_str = format!("{:.2} MB", t.file_size as f64 / (1024.0 * 1024.0));
            let role_str = match t.role {
                turbotransfer_core::manifest::TransferRole::Sender => "Sender (TX)",
                turbotransfer_core::manifest::TransferRole::Receiver => "Receiver (RX)",
            };

            let (status_str, status_style) = match t.status {
                TransferStatus::InProgress => ("IN_PROGRESS", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                TransferStatus::Completed => ("COMPLETED", Style::default().fg(Color::Green)),
                TransferStatus::Paused => ("PAUSED", Style::default().fg(Color::Yellow)),
                TransferStatus::Cancelled => ("CANCELLED", Style::default().fg(Color::DarkGray)),
                TransferStatus::Failed => ("FAILED", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
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
                Span::styled(format!("{:<30}", t.file_name), name_style),
                Span::styled(format!("{:<15}", size_str), Style::default().fg(Color::White)),
                Span::styled(format!("{:<12}", role_str), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:<16}", status_str), status_style),
                Span::styled(format!("{}", t.transfer_id), Style::default().fg(Color::DarkGray)),
            ]));
        }
    }

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, chunks[1]);
}

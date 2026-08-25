use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Gauge, Paragraph};
use ratatui::Frame;
use turbotransfer_core::manifest::TransferStatus;

use crate::app::AppState;

pub fn render_transfer_screen(f: &mut Frame, app: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(4), // Gauge Progress Bar
            Constraint::Min(8),    // Metrics & Transport Breakdown
        ])
        .split(area);

    // Header
    let file_name = app
        .active_progress
        .as_ref()
        .map(|p| p.file_name.as_str())
        .or_else(|| app.selected_file_path.as_ref().and_then(|p| p.file_name()).and_then(|n| n.to_str()))
        .unwrap_or("active_file.bin");

    let is_completed = app.active_progress.as_ref().map(|p| p.status) == Some(TransferStatus::Completed);

    let status_str = match app.active_progress.as_ref().map(|p| p.status) {
        Some(TransferStatus::Completed) => "COMPLETED (100%)",
        Some(TransferStatus::Failed) => "FAILED",
        Some(TransferStatus::Paused) => "PAUSED",
        Some(TransferStatus::Cancelled) => "CANCELLED",
        Some(TransferStatus::InProgress) => "TRANSFERRING",
        None => "INITIALIZING / CONNECTING",
    };

    let status_color = match app.active_progress.as_ref().map(|p| p.status) {
        Some(TransferStatus::Completed) => Color::Green,
        Some(TransferStatus::Failed) => Color::Red,
        Some(TransferStatus::Paused) => Color::Yellow,
        Some(TransferStatus::Cancelled) => Color::DarkGray,
        Some(TransferStatus::InProgress) => Color::Cyan,
        None => Color::Yellow,
    };

    let header_text = Line::from(vec![
        Span::styled(" LIVE TRANSFER MONITOR ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(format!("— {} [", file_name), Style::default().fg(Color::Gray)),
        Span::styled(status_str, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
        Span::styled("]", Style::default().fg(Color::Gray)),
    ]);
    let header_para = Paragraph::new(header_text).alignment(Alignment::Center);
    f.render_widget(header_para, chunks[0]);

    // Progress percentage
    let (percent, bytes_transferred, total_bytes, total_mbps, usb_mbps, wifi_mbps, eta_str) =
        if let Some(ref p) = app.active_progress {
            let eta = p.eta_seconds.map(|s| format!("{}s", s)).unwrap_or_else(|| "--".to_string());
            (
                (p.percent.round() as u16).min(100),
                p.bytes_transferred,
                p.file_size,
                p.aggregate_throughput_bps / (1024.0 * 1024.0),
                p.usb_throughput_bps / (1024.0 * 1024.0),
                p.wifi_throughput_bps / (1024.0 * 1024.0),
                eta,
            )
        } else {
            let total = app.selected_file_path.as_ref()
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| m.len())
                .unwrap_or(0);
            (0, 0, total, 0.0, 0.0, 0.0, "--".to_string())
        };

    // Gauge Progress Bar
    let gauge_title = format!(
        " {:.2} MB / {:.2} MB ({}%) ",
        bytes_transferred as f64 / (1024.0 * 1024.0),
        total_bytes as f64 / (1024.0 * 1024.0),
        if is_completed { 100 } else { percent }
    );
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).title(" Progress "))
        .gauge_style(Style::default().fg(if is_completed { Color::Green } else { Color::Cyan }).bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .percent(if is_completed { 100 } else { percent })
        .label(gauge_title);
    f.render_widget(gauge, chunks[1]);

    let is_failed = app.active_progress.as_ref().map(|p| p.status) == Some(TransferStatus::Failed);

    // Metrics & Transport Breakdown Block
    let metrics_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(if is_completed {
            " Transfer Summary & Final Throughput "
        } else if is_failed {
            " Transfer Failure Details "
        } else {
            " Multipath Throughput & Scheduler Metrics "
        });

    let metrics_lines = if is_completed {
        vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("   ✔ File transfer finished and verified cleanly!", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("   Average Throughput: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{:.2} MB/s", total_mbps), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::styled(format!("   │  Total Size: {:.2} MB", total_bytes as f64 / (1024.0 * 1024.0)), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("   ├─ USB Link Peak:   ", Style::default().fg(Color::White)),
                Span::styled(format!("{:>6.2} MB/s", usb_mbps), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("   └─ 5 GHz Wi-Fi Peak:", Style::default().fg(Color::White)),
                Span::styled(format!("{:>6.2} MB/s", wifi_mbps), Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("   [Enter] Return to Receiver / Menu   │   [Esc] Main Menu   │   [D] View Details", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]),
        ]
    } else if is_failed {
        let err_msg = app.status_message.as_deref().unwrap_or("Transfer interrupted or rejected by peer");
        vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("   ✖ File transfer failed to complete", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("   Diagnostics: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled(err_msg, Style::default().fg(Color::LightRed)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("   [R] Retry / Resume   │   [Esc] Main Menu   │   [D] Diagnostics Details", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]),
        ]
    } else {
        vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("   Aggregate Throughput: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{:.2} MB/s", total_mbps), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                Span::styled(format!("  (ETA: {})", eta_str), Style::default().fg(Color::Yellow)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("   ├─ USB (ADB Tunnel):     ", Style::default().fg(Color::White)),
                Span::styled(format!("{:>6.2} MB/s", usb_mbps), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled(
                    if usb_mbps > 0.01 { "  ● Active Streaming" } else { "  ○ Idle / Standby" },
                    Style::default().fg(if usb_mbps > 0.01 { Color::Green } else { Color::DarkGray }),
                ),
            ]),
            Line::from(vec![
                Span::styled("   └─ 5 GHz Wi-Fi Direct:   ", Style::default().fg(Color::White)),
                Span::styled(format!("{:>6.2} MB/s", wifi_mbps), Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
                Span::styled(
                    if wifi_mbps > 0.01 { "  ● Active Streaming" } else { "  ○ Idle / Standby" },
                    Style::default().fg(if wifi_mbps > 0.01 { Color::Green } else { Color::DarkGray }),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("   Scheduler Policy: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled(&app.settings.scheduling, Style::default().fg(Color::Yellow)),
                Span::styled("  │  Buffer Pool: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{} buffers (bounded RAM)", app.settings.buffer_count), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("   [P] Pause  │  [R] Resume  │  [C] Cancel  │  [D] Details  │  [Esc] Back", Style::default().fg(Color::DarkGray)),
            ]),
        ]
    };

    let metrics_para = Paragraph::new(metrics_lines).block(metrics_block);
    f.render_widget(metrics_para, chunks[2]);
}

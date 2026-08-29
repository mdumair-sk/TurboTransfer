use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub const MAIN_MENU_ITEMS: [(&str, &str, &str); 6] = [
    ("Send Files", "Transmit files over high-speed USB/Wi-Fi Direct", "1"),
    ("Receive Files", "Listen for incoming transfers on port 9876", "2"),
    ("Devices", "Discover and manage paired Android & PC nodes", "3"),
    ("Transfers", "Monitor active transfers and resume past sessions", "4"),
    ("Benchmark", "Measure raw transport throughput across links", "5"),
    ("Settings", "Configure buffer pools, chunk size & priority", "6"),
];

pub fn render_main_menu(f: &mut Frame, app: &AppState, area: Rect) {
    let main_panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Left: Clean Menu List
            Constraint::Percentage(50), // Right: System & Network Overview
        ])
        .split(area);

    // --- LEFT PANE: Clean Menu List (No nested border soup) ---
    let menu_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Menu ");

    let mut menu_lines = Vec::new();
    menu_lines.push(Line::from(""));

    for (i, (title, desc, key)) in MAIN_MENU_ITEMS.iter().enumerate() {
        let is_selected = i == app.selected_index;

        let (cursor, key_style, title_style, desc_style, bg_style) = if is_selected {
            (
                " ❯ ",
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                Style::default().fg(Color::Gray),
                Style::default().bg(Color::Rgb(35, 38, 48)),
            )
        } else {
            (
                "   ",
                Style::default().fg(Color::DarkGray),
                Style::default().fg(Color::Gray),
                Style::default().fg(Color::DarkGray),
                Style::default(),
            )
        };

        let line = Line::from(vec![
            Span::styled(cursor, if is_selected { Style::default().fg(Color::White).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::DarkGray) }),
            Span::styled(format!("[{}] ", key), key_style),
            Span::styled(format!("{:<15}", title), title_style),
            Span::styled(format!(" {}", desc), desc_style),
        ]).style(bg_style);

        menu_lines.push(line);
        menu_lines.push(Line::from(""));
    }

    let menu_para = Paragraph::new(menu_lines).block(menu_block);
    f.render_widget(menu_para, main_panes[0]);

    // --- RIGHT PANE: System Status & Overview ---
    let status_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" System Overview ");

    let connected_count = app.cached_devices.iter().filter(|d| d.is_connected).count();
    let total_devices = app.cached_devices.len();

    let mut status_lines = vec![
        Line::from(""),
        Line::from(Span::styled("   ENGINE RUNTIME", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD))),
        Line::from(vec![
            Span::styled("   • Core Engine:     ", Style::default().fg(Color::DarkGray)),
            Span::styled("Tokio Native Async", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(" (Zero-Copy Ring)", Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("   • Scheduler:       ", Style::default().fg(Color::DarkGray)),
            Span::styled(&app.settings.scheduling, Style::default().fg(Color::White)),
            Span::styled(" (Rate-Adaptive)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("   • Chunk Size:      ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{} MiB", app.settings.chunk_size_mib), Style::default().fg(Color::White)),
            Span::styled(" (xxHash64 frame / CRC32c verify)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled("   • Buffer Pool:     ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{} buffers ({} MB bounded RAM)", app.settings.buffer_count, app.settings.buffer_count as u32 * app.settings.chunk_size_mib),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled("   NETWORK & TRANSPORT STATUS", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD))),
        Line::from(vec![
            Span::styled("   • USB Link:        ", Style::default().fg(Color::DarkGray)),
            Span::styled("127.0.0.1:9876", Style::default().fg(Color::White)),
            Span::styled("  ● Ready (ADB Tunnel)", Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("   • Wi-Fi Direct:    ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("0.0.0.0:9876 [{}]", app.settings.p2p_band), Style::default().fg(Color::White)),
            Span::styled("  ● Listening", Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("   • Connected Peers: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if total_devices > 0 {
                    format!("{} connected ({} detected)", connected_count, total_devices)
                } else {
                    "Searching for nearby devices...".to_string()
                },
                Style::default().fg(if connected_count > 0 { Color::Green } else { Color::White }),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled("   STORAGE DESTINATION", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD))),
        Line::from(vec![
            Span::styled("   • Download Folder: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&app.settings.download_dir, Style::default().fg(Color::White)),
        ]),
    ];

    if let Some(ref progress) = app.active_progress {
        status_lines.push(Line::from(""));
        status_lines.push(Line::from(Span::styled("   ACTIVE TRANSFER", Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD))));
        status_lines.push(Line::from(vec![
            Span::styled("   • File:            ", Style::default().fg(Color::DarkGray)),
            Span::styled(&progress.file_name, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]));
        status_lines.push(Line::from(vec![
            Span::styled("   • Throughput:      ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                format!("{:.1} MB/s", progress.aggregate_throughput_bps / (1024.0 * 1024.0)),
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("  ({:.1}%)", progress.percent), Style::default().fg(Color::White)),
        ]));
    }

    let status_para = Paragraph::new(status_lines).block(status_block);
    f.render_widget(status_para, main_panes[1]);
}

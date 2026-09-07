use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;
use crate::ui::transport_selection::TRANSPORTS;

pub const BENCHMARK_SIZES: [u32; 4] = [100, 250, 500, 1000];

pub fn render_benchmark(f: &mut Frame, app: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // Transport selector in one clean block
            Constraint::Length(6), // Target Peer & Payload Config Block
            Constraint::Min(8),    // Benchmark & Calibration Actions / Progress Card
        ])
        .split(area);

    // 1. Transport Options Block
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Select Benchmark Transport ");

    let mut lines = Vec::new();
    lines.push(Line::from(""));

    for (i, (title, desc, _)) in TRANSPORTS.iter().enumerate() {
        let is_selected = i == app.benchmark_transport_index;

        let (cursor, title_style, desc_style, bg_style) = if is_selected {
            (
                " ❯ ",
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                Style::default().fg(Color::Gray),
                Style::default().bg(Color::Rgb(35, 38, 48)),
            )
        } else {
            (
                "   ",
                Style::default().fg(Color::Gray),
                Style::default().fg(Color::DarkGray),
                Style::default(),
            )
        };

        let line = Line::from(vec![
            Span::styled(cursor, if is_selected { Style::default().fg(Color::White).add_modifier(Modifier::BOLD) } else { Style::default().fg(Color::DarkGray) }),
            Span::styled(format!("{:<38}", title), title_style),
            Span::styled(format!(" — {}", desc), desc_style),
        ]).style(bg_style);

        lines.push(line);
    }

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, chunks[0]);

    // 2. Peer & Configuration Block
    let config_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Target Peer & Test Configuration ");

    let is_editing = app.input_mode == crate::app::InputMode::Editing;
    let peer_display = if is_editing {
        vec![
            Span::styled("   Target Peer Address:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{}_", app.peer_address_input), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled("  (Editing... [Enter]/[Esc] to finish)", Style::default().fg(Color::DarkGray)),
        ]
    } else if app.peer_address_input.trim().is_empty() {
        vec![
            Span::styled("   Target Peer Address:   ", Style::default().fg(Color::DarkGray)),
            Span::styled("Auto-detect via USB tunnel / Wi-Fi Direct", Style::default().fg(Color::Gray)),
            Span::styled("  (Press [P] to set IP:port)", Style::default().fg(Color::DarkGray)),
        ]
    } else {
        vec![
            Span::styled("   Target Peer Address:   ", Style::default().fg(Color::DarkGray)),
            Span::styled(&app.peer_address_input, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled("  (Press [P] to edit)", Style::default().fg(Color::DarkGray)),
        ]
    };

    let config_lines = vec![
        Line::from(""),
        Line::from(peer_display),
        Line::from(vec![
            Span::styled("   Selected Test Payload: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{} MB memory stream", app.benchmark_size_mb), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("  (Press [S] to cycle size)", Style::default().fg(Color::DarkGray)),
        ]),
    ];

    let config_para = Paragraph::new(config_lines).block(config_block);
    f.render_widget(config_para, chunks[1]);

    // 3. Actions / Live Calibration Progress Card
    let action_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Benchmark & Calibration Execution ");

    let action_lines = if app.is_calibrating {
        let step = app.calibration_progress.as_ref().map(|p| p.current_step).unwrap_or(1);
        let total = 10u32;
        let stage = app.calibration_progress.as_ref().map(|p| p.stage.as_str()).unwrap_or("initializing");
        let pct = (step as f64 / total as f64).clamp(0.0, 1.0);
        let filled = ((pct * 40.0).round() as usize).min(40);
        let bar_str = format!("{}{}", "█".repeat(filled), "░".repeat(40 - filled));

        let last_speed = app.calibration_progress.as_ref()
            .and_then(|p| p.last_result_mbps)
            .map_or("-".to_string(), |v| format!("{:.2} MB/s", v));

        vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("   ⚡ CALIBRATION SWEEP IN PROGRESS: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled(format!("Step {}/{} (Stage: {})", step, total, stage), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("   Progress: [", Style::default().fg(Color::DarkGray)),
                Span::styled(bar_str, Style::default().fg(Color::Cyan)),
                Span::styled(format!("] {:.0}%", pct * 100.0), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled(format!("  (Last Speed: {})", last_speed), Style::default().fg(Color::Green)),
            ]),
            Line::from(""),
            Line::from(Span::styled("   ► Press [X] to CANCEL CALIBRATION", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))),
        ]
    } else if app.is_benchmarking {
        vec![
            Line::from(""),
            Line::from(Span::styled("   ► Benchmarking in progress... please wait", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        ]
    } else {
        vec![
            Line::from(""),
            Line::from(Span::styled("   ► Press [Enter] to RUN BENCHMARK", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))),
            Line::from(Span::styled("   ► Press [C] to RUN LINK CALIBRATION (10-Step Adaptive Sweep)", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from(Span::styled("   Calibration automatically sweeps Wi-Fi stream counts (2..4), chunk sizes (1..4 MiB),", Style::default().fg(Color::DarkGray))),
            Line::from(Span::styled("   and TCP window presets (Balanced..Max) to discover & save optimal link parameters.", Style::default().fg(Color::DarkGray))),
        ]
    };

    let action_para = Paragraph::new(action_lines).block(action_block);
    f.render_widget(action_para, chunks[2]);
}

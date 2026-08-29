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
            Constraint::Min(6),    // Payload size & Launch card
        ])
        .split(area);

    // Transport Options Block
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

    // Launch Card
    let launch_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Test Configuration ");

    let launch_lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("   Selected Test Payload: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{} MB memory stream", app.benchmark_size_mb), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("  (Press [S] to cycle size)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(Span::styled("   ► Press [Enter] to RUN BENCHMARK", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))),
    ];

    let launch_para = Paragraph::new(launch_lines).block(launch_block);
    f.render_widget(launch_para, chunks[1]);
}

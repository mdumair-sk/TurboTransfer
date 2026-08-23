use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
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
            Constraint::Length(3),  // Header
            Constraint::Length(14), // Transport selector
            Constraint::Min(6),     // Payload size & Launch card
            Constraint::Length(3),  // Footer
        ])
        .split(area);

    // Header
    let header_text = Line::from(vec![
        Span::styled(" HARDWARE THROUGHPUT BENCHMARK ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled("— Measure raw byte throughput across ADB & 5 GHz Wi-Fi (TRD §8, §9)", Style::default().fg(Color::Gray)),
    ]);
    let header_para = Paragraph::new(header_text).alignment(Alignment::Center);
    f.render_widget(header_para, chunks[0]);

    // Transport Options
    let card_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(vec![Constraint::Length(3); TRANSPORTS.len()])
        .split(chunks[1]);

    for (i, (title, desc, _)) in TRANSPORTS.iter().enumerate() {
        let is_selected = i == app.benchmark_transport_index;
        let border_style = if is_selected {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(if is_selected { BorderType::Double } else { BorderType::Plain })
            .border_style(border_style);

        let title_span = Span::styled(
            format!("{:<38}", title),
            if is_selected {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            },
        );
        let desc_span = Span::styled(format!("  — {}", desc), Style::default().fg(Color::DarkGray));

        let line = Line::from(vec![title_span, desc_span]);
        let para = Paragraph::new(line).block(block);
        f.render_widget(para, card_chunks[i]);
    }

    // Launch Card
    let launch_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(" Test Configuration ");

    let launch_lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("   Selected Test Payload: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{} MB memory stream", app.benchmark_size_mb), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::styled("  (Press [S] to cycle size)", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(Span::styled("   ► Press [Enter] to RUN BENCHMARK via Transfer API run_benchmark()", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))),
    ];

    let launch_para = Paragraph::new(launch_lines).block(launch_block);
    f.render_widget(launch_para, chunks[2]);

    // Footer
    let footer_text = vec![
        Span::styled(" [Enter] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::raw("Start Benchmark  "),
        Span::styled(" [S] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw("Cycle Size  "),
        Span::styled(" [Up/Down] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw("Select Transport  "),
        Span::styled(" [Esc] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw("Main Menu"),
    ];
    let footer_para = Paragraph::new(Line::from(footer_text))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer_para, chunks[3]);
}

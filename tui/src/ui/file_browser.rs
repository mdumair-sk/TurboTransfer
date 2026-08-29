use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::AppState;

pub fn render_file_browser(f: &mut Frame, app: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Path banner
            Constraint::Min(8),    // File & Folder list
        ])
        .split(area);

    // Current Directory Banner & Search Pill
    let mut banner_spans = vec![
        Span::styled(" Directory: ", Style::default().fg(Color::DarkGray)),
        Span::styled(app.browser_current_dir.display().to_string(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
    ];

    if !app.file_search_query.is_empty() {
        banner_spans.push(Span::styled("   🔍 Filter: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)));
        banner_spans.push(Span::styled(format!(" \"{}\" ", app.file_search_query), Style::default().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD)));
        banner_spans.push(Span::styled(" (Type to filter, Backspace/Esc to clear)", Style::default().fg(Color::DarkGray)));
    }

    let banner_text = Line::from(banner_spans);
    let banner_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));
    let banner_para = Paragraph::new(banner_text).block(banner_block);
    f.render_widget(banner_para, chunks[0]);

    // File list
    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Files & Directories (Enter: Select / Backspace: Up) ");

    let mut lines = Vec::new();

    // Up directory entry
    let is_parent_selected = app.browser_selected_index == 0;
    let parent_prefix = if is_parent_selected { " ► " } else { "   " };
    lines.push(Line::from(vec![
        Span::styled(
            parent_prefix,
            if is_parent_selected {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ),
        Span::styled("📁 ", Style::default().fg(Color::White)),
        Span::styled(
            ".. (Parent Directory)",
            if is_parent_selected {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            },
        ),
    ]));

    for (i, path) in app.browser_entries.iter().enumerate() {
        let is_selected = (i + 1) == app.browser_selected_index;
        let prefix = if is_selected { " ► " } else { "   " };

        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?");
        let is_dir = path.is_dir();

        let (icon, size_str, style) = if is_dir {
            (
                "📁 ",
                "<DIR>".to_string(),
                if is_selected {
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                },
            )
        } else {
            let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            let formatted_size = if size > 1024 * 1024 * 1024 {
                format!("{:.2} GB", size as f64 / (1024.0 * 1024.0 * 1024.0))
            } else if size > 1024 * 1024 {
                format!("{:.2} MB", size as f64 / (1024.0 * 1024.0))
            } else if size > 1024 {
                format!("{:.1} KB", size as f64 / 1024.0)
            } else {
                format!("{} B", size)
            };
            (
                "📄 ",
                formatted_size,
                if is_selected {
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Gray)
                },
            )
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
            Span::styled(icon, Style::default().fg(Color::White)),
            Span::styled(format!("{:<44}", name), style),
            Span::styled(format!("{:>14}", size_str), Style::default().fg(Color::DarkGray)),
        ]));
    }

    if app.browser_entries.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("   (Empty directory)", Style::default().fg(Color::DarkGray))));
    }

    let visible_rows = (chunks[1].height.saturating_sub(2) as usize).max(1);
    let scroll_y = if app.browser_selected_index >= visible_rows {
        (app.browser_selected_index - visible_rows + 1) as u16
    } else {
        0
    };

    let list_para = Paragraph::new(lines)
        .block(list_block)
        .scroll((scroll_y, 0));
    f.render_widget(list_para, chunks[1]);
}

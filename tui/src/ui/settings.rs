use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{AppState, SettingsTab};

pub fn render_settings(f: &mut Frame, app: &AppState, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tab Bar
            Constraint::Min(10),   // Active Tab Body
        ])
        .split(area);

    // Render Sub-Tabs
    let titles: Vec<Line> = SettingsTab::ALL
        .iter()
        .map(|t| {
            let style = if *t == app.settings_tab {
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
                .title(" Settings Category "),
        )
        .select(app.settings_tab as usize);
    f.render_widget(tabs, chunks[0]);

    // Active Tab Body
    let body_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(format!(" {} Configuration ", app.settings_tab.title()));

    let mut lines = Vec::new();
    lines.push(Line::from(""));

    match app.settings_tab {
        SettingsTab::Transport => {
            lines.push(render_setting_row("Transport Priority", &app.settings.transport_pref, app.settings_item == 0, "Default channel: Automatic (Combined multipath)"));
            lines.push(render_setting_row("Wi-Fi Direct Band", &app.settings.p2p_band, app.settings_item == 1, "5 GHz primary for wire-speed (~35 MB/s); 2.4 GHz fallback"));
        }
        SettingsTab::Storage => {
            lines.push(render_setting_row("Default Download Directory", &app.settings.download_dir, false, "Target folder where received files are saved"));
            lines.push(render_setting_row("Progress Polling Interval", &format!("{} ms", app.settings.poll_interval_ms), false, "UI refresh cadence"));
        }
    }

    let para = Paragraph::new(lines).block(body_block);
    f.render_widget(para, chunks[1]);
}

fn render_setting_row(label: &str, value: &str, is_active: bool, hint: &str) -> Line<'static> {
    let prefix = if is_active { " ► " } else { "   " };
    let prefix_style = if is_active {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let label_style = if is_active {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    let value_style = if is_active {
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White)
    };

    Line::from(vec![
        Span::styled(prefix.to_string(), prefix_style),
        Span::styled(format!("{:<30}", label), label_style),
        Span::styled(format!(": {:<25}", value), value_style),
        Span::styled(format!("  ({})", hint), Style::default().fg(Color::DarkGray)),
    ])
}

pub mod benchmark;
pub mod benchmark_results;
pub mod device_selection;
pub mod devices;
pub mod file_browser;
pub mod incoming_prompt;
pub mod main_menu;
pub mod receive_files;
pub mod resume;
pub mod send_files;
pub mod settings;
pub mod transfer_details;
pub mod transfer_screen;
pub mod transfers;
pub mod transport_selection;

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, Screen};

pub fn render_ui(f: &mut Frame, app: &AppState) {
    let size = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Top Header Bar
            Constraint::Min(12),   // Main Viewport
            Constraint::Length(3), // Bottom Footer Shortcuts
        ])
        .split(size);

    // 1. Top Header Bar
    render_header(f, app, chunks[0]);

    // 2. Main Viewport Router
    match app.current_screen {
        Screen::MainMenu => main_menu::render_main_menu(f, app, chunks[1]),
        Screen::SendFiles => send_files::render_send_files(f, app, chunks[1]),
        Screen::FileBrowser => file_browser::render_file_browser(f, app, chunks[1]),
        Screen::DeviceSelection => device_selection::render_device_selection(f, app, chunks[1]),
        Screen::TransportSelection => transport_selection::render_transport_selection(f, app, chunks[1]),
        Screen::TransferScreen => transfer_screen::render_transfer_screen(f, app, chunks[1]),
        Screen::TransferDetails => transfer_details::render_transfer_details(f, app, chunks[1]),
        Screen::ReceiveFiles => receive_files::render_receive_files(f, app, chunks[1]),
        Screen::IncomingPrompt => incoming_prompt::render_incoming_prompt(f, app, chunks[1]),
        Screen::Devices => devices::render_devices(f, app, chunks[1]),
        Screen::Transfers => transfers::render_transfers(f, app, chunks[1]),
        Screen::Resume => resume::render_resume(f, app, chunks[1]),
        Screen::Benchmark => benchmark::render_benchmark(f, app, chunks[1]),
        Screen::BenchmarkResults => benchmark_results::render_benchmark_results(f, app, chunks[1]),
        Screen::Settings => settings::render_settings(f, app, chunks[1]),
    }

    // Render modal overlay if incoming prompt is active
    if app.current_screen == Screen::ReceiveFiles && app.incoming_prompt.is_some() {
        incoming_prompt::render_incoming_prompt(f, app, size);
    }

    // 3. Bottom Footer Shortcuts
    render_footer(f, app, chunks[2]);
}

fn render_header(f: &mut Frame, app: &AppState, area: Rect) {
    let breadcrumb = match app.current_screen {
        Screen::MainMenu => "Dashboard",
        Screen::SendFiles => "Send Files",
        Screen::FileBrowser => "Send Files » File Browser",
        Screen::DeviceSelection => "Send Files » Select Device",
        Screen::TransportSelection => "Send Files » Select Transport",
        Screen::TransferScreen => "Transfer Monitor",
        Screen::TransferDetails => "Transfer Diagnostics",
        Screen::ReceiveFiles => "Receive Mode",
        Screen::IncomingPrompt => "Incoming Transfer Request",
        Screen::Devices => "Discovered Devices",
        Screen::Transfers => "Transfers History",
        Screen::Resume => "Cold Resume Selector",
        Screen::Benchmark => "Benchmark Tool",
        Screen::BenchmarkResults => "Benchmark Results",
        Screen::Settings => "Settings",
    };

    let title_line = Line::from(vec![
        Span::styled(" TURBOTRANSFER ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled(breadcrumb, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  [Transport: {} │ Band: {}]", app.settings.transport_pref, app.settings.p2p_band), Style::default().fg(Color::DarkGray)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));

    let para = Paragraph::new(title_line).block(block);
    f.render_widget(para, area);
}

fn render_footer(f: &mut Frame, app: &AppState, area: Rect) {
    let shortcuts = match app.current_screen {
        Screen::MainMenu => vec![
            Span::styled(" [1-6] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Jump  ", Style::default().fg(Color::Gray)),
            Span::styled(" [↑/↓] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Select  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Enter] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Open  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Q] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Quit", Style::default().fg(Color::Gray)),
        ],
        Screen::SendFiles => vec![
            Span::styled(" [↑/↓] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Select  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Enter] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Confirm  ", Style::default().fg(Color::Gray)),
            Span::styled(" [B] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Browse Files  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Esc] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Main Menu", Style::default().fg(Color::Gray)),
        ],
        Screen::FileBrowser => vec![
            Span::styled(" [Enter/→] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Open/Select  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Backspace/←] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Parent Dir  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Tab] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Path Autocomplete  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Esc] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Back", Style::default().fg(Color::Gray)),
        ],
        Screen::DeviceSelection => vec![
            Span::styled(" [↑/↓] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Select Device  ", Style::default().fg(Color::Gray)),
            Span::styled(" [R] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Refresh  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Enter] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Confirm  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Esc] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Back", Style::default().fg(Color::Gray)),
        ],
        Screen::TransportSelection => vec![
            Span::styled(" [1-4] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Quick Select  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Enter] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Start Transfer  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Esc] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Back", Style::default().fg(Color::Gray)),
        ],
        Screen::TransferScreen => vec![
            Span::styled(" [P] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Pause  ", Style::default().fg(Color::Gray)),
            Span::styled(" [R] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled("Resume  ", Style::default().fg(Color::Gray)),
            Span::styled(" [C] ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled("Cancel  ", Style::default().fg(Color::Gray)),
            Span::styled(" [D] ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled("Diagnostics  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Esc] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Dashboard", Style::default().fg(Color::Gray)),
        ],
        Screen::TransferDetails => vec![
            Span::styled(" [P] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Pause  ", Style::default().fg(Color::Gray)),
            Span::styled(" [C] ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::styled("Cancel  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Esc] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Back to Monitor", Style::default().fg(Color::Gray)),
        ],
        Screen::ReceiveFiles => vec![
            Span::styled(" [S] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Settings  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Esc] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Dashboard", Style::default().fg(Color::Gray)),
        ],
        Screen::Devices => vec![
            Span::styled(" [↑/↓] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Select  ", Style::default().fg(Color::Gray)),
            Span::styled(" [R] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Refresh  ", Style::default().fg(Color::Gray)),
            Span::styled(" [S/Enter] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Send File  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Esc] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Dashboard", Style::default().fg(Color::Gray)),
        ],
        Screen::Transfers => vec![
            Span::styled(" [Tab] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Switch Category  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Enter] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled("Resume Selected  ", Style::default().fg(Color::Gray)),
            Span::styled(" [D] ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled("Details  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Esc] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Dashboard", Style::default().fg(Color::Gray)),
        ],
        Screen::Resume => vec![
            Span::styled(" [Enter] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled("Resume Selected  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Esc] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Back", Style::default().fg(Color::Gray)),
        ],
        Screen::Benchmark => vec![
            Span::styled(" [↑/↓] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Select Transport  ", Style::default().fg(Color::Gray)),
            Span::styled(" [S] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Cycle Size  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Enter] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Run Benchmark  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Esc] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Dashboard", Style::default().fg(Color::Gray)),
        ],
        Screen::BenchmarkResults => vec![
            Span::styled(" [Esc] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Benchmark Screen  ", Style::default().fg(Color::Gray)),
            Span::styled(" [M] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Dashboard", Style::default().fg(Color::Gray)),
        ],
        Screen::Settings => vec![
            Span::styled(" [Tab] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Next Tab  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Enter/Space] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Toggle  ", Style::default().fg(Color::Gray)),
            Span::styled(" [Esc] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Dashboard", Style::default().fg(Color::Gray)),
        ],
        _ => vec![
            Span::styled(" [Esc] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Back  ", Style::default().fg(Color::Gray)),
            Span::styled(" [1-6] ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled("Dashboard Shortcuts", Style::default().fg(Color::Gray)),
        ],
    };

    let status = app.status_message.as_deref().unwrap_or("");
    let status_span = Span::styled(
        format!("  ● {}  ", status),
        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
    );

    let mut full_spans = shortcuts;
    if !status.is_empty() {
        full_spans.push(status_span);
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::DarkGray));

    let para = Paragraph::new(Line::from(full_spans))
        .alignment(Alignment::Center)
        .block(block);
    f.render_widget(para, area);
}

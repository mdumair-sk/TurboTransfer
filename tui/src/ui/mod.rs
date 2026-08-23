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

    // 2. Main Viewport Router (All 15 Screens from TRD §13)
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
        Screen::MainMenu => "Main Menu",
        Screen::SendFiles => "Send Files",
        Screen::FileBrowser => "Send Files » File Browser",
        Screen::DeviceSelection => "Send Files » Target Device",
        Screen::TransportSelection => "Send Files » Transport Selection",
        Screen::TransferScreen => "Live Transfer Screen",
        Screen::TransferDetails => "Transfer Diagnostics & Details",
        Screen::ReceiveFiles => "Receive Files Mode",
        Screen::IncomingPrompt => "Incoming Transfer Prompt",
        Screen::Devices => "Discovered Devices",
        Screen::Transfers => "Transfers Manager",
        Screen::Resume => "Cold Resume Selector",
        Screen::Benchmark => "Benchmark Screen",
        Screen::BenchmarkResults => "Benchmark Results",
        Screen::Settings => "Settings Manager",
    };

    let title_line = Line::from(vec![
        Span::styled(" TurboTransfer ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" » {} ", breadcrumb), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" [Pref: {} | Band: {}] ", app.settings.transport_pref, app.settings.p2p_band), Style::default().fg(Color::DarkGray)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);

    let para = Paragraph::new(title_line).block(block);
    f.render_widget(para, area);
}

fn render_footer(f: &mut Frame, app: &AppState, area: Rect) {
    let shortcuts = match app.current_screen {
        Screen::MainMenu => vec![
            Span::styled(" [1-6] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Direct Jump  "),
            Span::styled(" [Up/Down] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Select  "),
            Span::styled(" [Enter] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Open  "),
            Span::styled(" [Q] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Quit"),
        ],
        Screen::SendFiles => vec![
            Span::styled(" [Up/Down] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Select Option  "),
            Span::styled(" [Enter] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Confirm  "),
            Span::styled(" [B] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Browse Files  "),
            Span::styled(" [Esc] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Main Menu"),
        ],
        Screen::FileBrowser => vec![
            Span::styled(" [Enter/Right] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Open / Select  "),
            Span::styled(" [Backspace/Left] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Parent Dir  "),
            Span::styled(" [Up/Down] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Navigate  "),
            Span::styled(" [Home/End] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Top/Bottom  "),
            Span::styled(" [Esc] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Back"),
        ],
        Screen::DeviceSelection => vec![
            Span::styled(" [Up/Down] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Select Device  "),
            Span::styled(" [R] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Refresh Devices  "),
            Span::styled(" [Enter] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Confirm  "),
            Span::styled(" [Esc] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Back"),
        ],
        Screen::TransportSelection => vec![
            Span::styled(" [1-4] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Quick Select  "),
            Span::styled(" [Up/Down] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Navigate  "),
            Span::styled(" [Enter] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Start Transfer  "),
            Span::styled(" [Esc] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Back"),
        ],
        Screen::TransferScreen => vec![
            Span::styled(" [P] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Pause  "),
            Span::styled(" [R] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("Resume  "),
            Span::styled(" [C] ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw("Cancel  "),
            Span::styled(" [D] ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("Details  "),
            Span::styled(" [Esc] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Main Menu"),
        ],
        Screen::TransferDetails => vec![
            Span::styled(" [P] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Pause  "),
            Span::styled(" [C] ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw("Cancel  "),
            Span::styled(" [Esc] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Back to Monitor"),
        ],
        Screen::ReceiveFiles => vec![
            Span::styled(" [S] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Settings  "),
            Span::styled(" [Esc] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Main Menu"),
        ],
        Screen::Devices => vec![
            Span::styled(" [Up/Down] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Select Device  "),
            Span::styled(" [R] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Refresh  "),
            Span::styled(" [S/Enter] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Send File  "),
            Span::styled(" [Esc] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Main Menu"),
        ],
        Screen::Transfers => vec![
            Span::styled(" [Tab] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Switch Category  "),
            Span::styled(" [R] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("Resume  "),
            Span::styled(" [D] ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("Details  "),
            Span::styled(" [Esc] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Main Menu"),
        ],
        Screen::Resume => vec![
            Span::styled(" [Enter] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("Resume Selected  "),
            Span::styled(" [Esc] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Back"),
        ],
        Screen::Benchmark => vec![
            Span::styled(" [Up/Down] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Select Transport  "),
            Span::styled(" [S] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Cycle Size  "),
            Span::styled(" [Enter] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Run Benchmark  "),
            Span::styled(" [Esc] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Main Menu"),
        ],
        Screen::BenchmarkResults => vec![
            Span::styled(" [Esc] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Benchmark Screen  "),
            Span::styled(" [M] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Main Menu"),
        ],
        Screen::Settings => vec![
            Span::styled(" [Tab] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Next Tab  "),
            Span::styled(" [Enter/Space] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Toggle Setting  "),
            Span::styled(" [Esc] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Main Menu"),
        ],
        _ => vec![
            Span::styled(" [Esc] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Back  "),
            Span::styled(" [1-6] ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw("Main Menu Shortcuts"),
        ],
    };

    let status = app.status_message.as_deref().unwrap_or("");
    let status_span = Span::styled(format!("  {}  ", status), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));

    let mut full_spans = shortcuts;
    if !status.is_empty() {
        full_spans.push(status_span);
    }

    let block = Block::default().borders(Borders::ALL);
    let para = Paragraph::new(Line::from(full_spans)).alignment(Alignment::Center).block(block);
    f.render_widget(para, area);
}

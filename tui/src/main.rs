pub mod app;
pub mod config;
pub mod events;
pub mod ui;

use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::stdout;
use std::time::{Duration, Instant};
use turbotransfer_core::transfer::leave_receive_mode;

use app::AppState;
use events::handle_key_event;
use ui::render_ui;

/// Centralized cleanup to guarantee rollback of receiver state upon exit.
fn perform_cleanup() {
    // Stop all core transfer listeners
    let _ = leave_receive_mode(None);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Set panic hook to ensure terminal restoration and clean exit
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
        perform_cleanup();
        default_panic(info);
    }));

    // 1. Initialize Terminal immediately (<1ms)
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 2. Initialize App State
    let mut app = AppState::new();
    let tick_rate = Duration::from_millis(app.settings.poll_interval_ms);
    let mut last_tick = Instant::now();

    // 3. Main Event & Render Loop
    let mut last_key_time = Instant::now();
    let mut last_key_code: Option<crossterm::event::KeyCode> = None;
    let mut last_key_modifiers: Option<crossterm::event::KeyModifiers> = None;

    while app.running {
        terminal.draw(|f| render_ui(f, &app))?;

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            while event::poll(Duration::from_millis(0))? {
                let ev = event::read()?;
                if let Event::Key(key) = ev {
                    if key.kind == crossterm::event::KeyEventKind::Press {
                        let now = Instant::now();
                        let is_duplicate = match (last_key_code, last_key_modifiers) {
                            (Some(code), Some(mods)) => {
                                code == key.code
                                    && mods == key.modifiers
                                    && now.duration_since(last_key_time) < Duration::from_millis(50)
                            }
                            _ => false,
                        };

                        if !is_duplicate {
                            last_key_time = now;
                            last_key_code = Some(key.code);
                            last_key_modifiers = Some(key.modifiers);
                            handle_key_event(&mut app, key);
                            break;
                        }
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.poll_active_progress();
            last_tick = Instant::now();
        }
    }

    // 4. Clean Terminal Restoration & State Rollback
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    perform_cleanup();

    Ok(())
}

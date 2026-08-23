use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use std::path::PathBuf;
use turbotransfer_core::transfer::TransportPreference;

use crate::app::{AppState, Screen, SettingsTab};
use crate::ui::benchmark::BENCHMARK_SIZES;
use crate::ui::main_menu::MAIN_MENU_ITEMS;
use crate::ui::send_files::SEND_OPTIONS;
use crate::ui::transport_selection::TRANSPORTS;

/// Handles incoming key events across the global application lifecycle (§13).
pub fn handle_key_event(app: &mut AppState, key: KeyEvent) {
    // Only handle key press events to avoid double-processing on Windows (Press + Release)
    if key.kind != KeyEventKind::Press {
        return;
    }

    // Global exit shortcut
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        app.running = false;
        return;
    }

    match app.current_screen {
        Screen::MainMenu => match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                app.running = false;
            }
            KeyCode::Char('1') => app.navigate_to(Screen::SendFiles),
            KeyCode::Char('2') => {
                app.start_receive_mode();
                app.navigate_to(Screen::ReceiveFiles);
            }
            KeyCode::Char('3') => app.navigate_to(Screen::Devices),
            KeyCode::Char('4') => app.navigate_to(Screen::Transfers),
            KeyCode::Char('5') => app.navigate_to(Screen::Benchmark),
            KeyCode::Char('6') => app.navigate_to(Screen::Settings),
            KeyCode::Up | KeyCode::Char('k') => {
                app.prev_item(MAIN_MENU_ITEMS.len());
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.next_item(MAIN_MENU_ITEMS.len());
            }
            KeyCode::Enter | KeyCode::Right => match app.selected_index {
                0 => app.navigate_to(Screen::SendFiles),
                1 => {
                    app.start_receive_mode();
                    app.navigate_to(Screen::ReceiveFiles);
                }
                2 => app.navigate_to(Screen::Devices),
                3 => app.navigate_to(Screen::Transfers),
                4 => app.navigate_to(Screen::Benchmark),
                5 => app.navigate_to(Screen::Settings),
                _ => {}
            },
            KeyCode::Esc => {
                app.running = false;
            }
            _ => {}
        },

        Screen::SendFiles => match key.code {
            KeyCode::Char('b') | KeyCode::Char('B') => {
                app.refresh_browser_entries();
                app.navigate_to(Screen::FileBrowser);
            }
            KeyCode::Char('p') | KeyCode::Char('P') => {
                app.selected_file_path = Some(PathBuf::from("sample_transfer_file.bin"));
                app.status_message = Some("Path entered manually".to_string());
            }
            KeyCode::Up | KeyCode::Char('k') => {
                app.prev_item(SEND_OPTIONS.len());
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.next_item(SEND_OPTIONS.len());
            }
            KeyCode::Enter => {
                if app.selected_file_path.is_some() {
                    app.navigate_to(Screen::DeviceSelection);
                } else {
                    match app.selected_index {
                        0 => {
                            app.refresh_browser_entries();
                            app.navigate_to(Screen::FileBrowser);
                        }
                        1 => {
                            app.selected_file_path = Some(PathBuf::from("manual_input_file.bin"));
                        }
                        _ => {}
                    }
                }
            }
            KeyCode::Esc => app.navigate_to(Screen::MainMenu),
            _ => {}
        },

        Screen::FileBrowser => match key.code {
            KeyCode::Up => {
                let max_len = app.browser_entries.len() + 1;
                if app.browser_selected_index == 0 {
                    app.browser_selected_index = max_len - 1;
                } else {
                    app.browser_selected_index -= 1;
                }
            }
            KeyCode::Down => {
                let max_len = app.browser_entries.len() + 1;
                app.browser_selected_index = (app.browser_selected_index + 1) % max_len;
            }
            KeyCode::Home => {
                app.browser_selected_index = 0;
            }
            KeyCode::End => {
                app.browser_selected_index = app.browser_entries.len();
            }
            KeyCode::PageUp => {
                app.browser_selected_index = app.browser_selected_index.saturating_sub(10);
            }
            KeyCode::PageDown => {
                let max_len = app.browser_entries.len() + 1;
                app.browser_selected_index = (app.browser_selected_index + 10).min(max_len - 1);
            }
            KeyCode::Backspace => {
                if !app.handle_browser_backspace() {
                    if let Some(parent) = app.browser_current_dir.parent().map(|p| p.to_path_buf()) {
                        app.browser_current_dir = parent;
                        app.refresh_browser_entries();
                    }
                }
            }
            KeyCode::Left => {
                app.clear_browser_search();
                if let Some(parent) = app.browser_current_dir.parent().map(|p| p.to_path_buf()) {
                    app.browser_current_dir = parent;
                    app.refresh_browser_entries();
                }
            }
            KeyCode::Enter | KeyCode::Right => {
                app.clear_browser_search();
                if app.browser_selected_index == 0 {
                    if let Some(parent) = app.browser_current_dir.parent().map(|p| p.to_path_buf()) {
                        app.browser_current_dir = parent;
                        app.refresh_browser_entries();
                    }
                } else {
                    let entry_idx = app.browser_selected_index - 1;
                    if let Some(path) = app.browser_entries.get(entry_idx).cloned() {
                        if path.is_dir() {
                            app.browser_current_dir = path;
                            app.refresh_browser_entries();
                        } else {
                            app.selected_file_path = Some(path);
                            app.navigate_to(Screen::DeviceSelection);
                        }
                    }
                }
            }
            KeyCode::Esc => {
                if !app.file_search_query.is_empty() {
                    app.clear_browser_search();
                } else {
                    app.navigate_to(Screen::SendFiles);
                }
            }
            KeyCode::Char(c) => {
                app.handle_browser_type_ahead(c);
            }
            _ => {}
        },

        Screen::DeviceSelection => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                let max_len = app.cached_devices.len().max(1);
                if app.selected_device_index == 0 {
                    app.selected_device_index = max_len - 1;
                } else {
                    app.selected_device_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max_len = app.cached_devices.len().max(1);
                app.selected_device_index = (app.selected_device_index + 1) % max_len;
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                app.refresh_devices();
                app.status_message = Some("Device list refreshed via get_devices()".to_string());
            }
            KeyCode::Enter => {
                app.navigate_to(Screen::TransportSelection);
            }
            KeyCode::Esc => app.navigate_to(Screen::SendFiles),
            _ => {}
        },

        Screen::TransportSelection => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if app.selected_transport_index == 0 {
                    app.selected_transport_index = TRANSPORTS.len() - 1;
                } else {
                    app.selected_transport_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.selected_transport_index = (app.selected_transport_index + 1) % TRANSPORTS.len();
            }
            KeyCode::Char('1') => app.selected_transport_index = 0,
            KeyCode::Char('2') => app.selected_transport_index = 1,
            KeyCode::Char('3') => app.selected_transport_index = 2,
            KeyCode::Char('4') => app.selected_transport_index = 3,
            KeyCode::Enter => {
                if let Some(file_path) = app.selected_file_path.clone() {
                    let pref = match app.selected_transport_index {
                        1 => TransportPreference::Combined,
                        2 => TransportPreference::UsbOnly,
                        3 => TransportPreference::WifiDirectOnly,
                        _ => TransportPreference::Automatic,
                    };

                    let target_device_id = app
                        .cached_devices
                        .get(app.selected_device_index)
                        .map(|d| d.device_id);

                    app.start_send_transfer(file_path, target_device_id, pref);
                }
            }
            KeyCode::Esc => app.navigate_to(Screen::DeviceSelection),
            _ => {}
        },

        Screen::TransferScreen => match key.code {
            KeyCode::Char('p') | KeyCode::Char('P') => app.pause_active(),
            KeyCode::Char('r') | KeyCode::Char('R') => app.resume_active(),
            KeyCode::Char('c') | KeyCode::Char('C') => app.cancel_active(),
            KeyCode::Char('d') | KeyCode::Char('D') => app.navigate_to(Screen::TransferDetails),
            KeyCode::Enter => {
                if app.is_receiving {
                    app.navigate_to(Screen::ReceiveFiles);
                } else {
                    app.navigate_to(Screen::MainMenu);
                }
            }
            KeyCode::Esc => app.navigate_to(Screen::MainMenu),
            _ => {}
        },

        Screen::TransferDetails => match key.code {
            KeyCode::Char('p') | KeyCode::Char('P') => app.pause_active(),
            KeyCode::Char('c') | KeyCode::Char('C') => app.cancel_active(),
            KeyCode::Esc => app.navigate_to(Screen::TransferScreen),
            _ => {}
        },

        Screen::Transfers => match key.code {
            KeyCode::Tab | KeyCode::Right => app.next_transfers_tab(),
            KeyCode::BackTab | KeyCode::Left => app.prev_transfers_tab(),
            KeyCode::Char('r') | KeyCode::Char('R') => app.navigate_to(Screen::Resume),
            KeyCode::Char('d') | KeyCode::Char('D') => app.navigate_to(Screen::TransferDetails),
            KeyCode::Char('c') | KeyCode::Char('C') => app.cancel_active(),
            KeyCode::Esc => app.navigate_to(Screen::MainMenu),
            _ => {}
        },

        Screen::Resume => match key.code {
            KeyCode::Enter => {
                app.resume_active();
            }
            KeyCode::Esc => app.navigate_to(Screen::Transfers),
            _ => {}
        },

        Screen::Benchmark => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if app.benchmark_transport_index == 0 {
                    app.benchmark_transport_index = TRANSPORTS.len() - 1;
                } else {
                    app.benchmark_transport_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                app.benchmark_transport_index = (app.benchmark_transport_index + 1) % TRANSPORTS.len();
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                let idx = BENCHMARK_SIZES
                    .iter()
                    .position(|&s| s == app.benchmark_size_mb)
                    .unwrap_or(0);
                app.benchmark_size_mb = BENCHMARK_SIZES[(idx + 1) % BENCHMARK_SIZES.len()];
            }
            KeyCode::Enter => {
                app.run_benchmark_action();
            }
            KeyCode::Esc => app.navigate_to(Screen::MainMenu),
            _ => {}
        },

        Screen::BenchmarkResults => match key.code {
            KeyCode::Esc => app.navigate_to(Screen::Benchmark),
            KeyCode::Char('m') | KeyCode::Char('M') => app.navigate_to(Screen::MainMenu),
            _ => {}
        },

        Screen::ReceiveFiles => match key.code {
            KeyCode::Char('s') | KeyCode::Char('S') => app.navigate_to(Screen::Settings),
            KeyCode::Esc => app.navigate_to(Screen::MainMenu),
            _ => {}
        },

        Screen::Devices => match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                let max_len = app.cached_devices.len().max(1);
                if app.selected_device_index == 0 {
                    app.selected_device_index = max_len - 1;
                } else {
                    app.selected_device_index -= 1;
                }
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let max_len = app.cached_devices.len().max(1);
                app.selected_device_index = (app.selected_device_index + 1) % max_len;
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                app.refresh_devices();
                app.status_message = Some("Refreshed discovered devices".to_string());
            }
            KeyCode::Char('s') | KeyCode::Char('S') | KeyCode::Enter => {
                app.navigate_to(Screen::SendFiles);
            }
            KeyCode::Esc => app.navigate_to(Screen::MainMenu),
            _ => {}
        },

        Screen::Settings => match key.code {
            KeyCode::Esc => {
                app.navigate_to(Screen::MainMenu);
            }
            KeyCode::Tab | KeyCode::Right => {
                app.next_settings_tab();
            }
            KeyCode::BackTab | KeyCode::Left => {
                app.prev_settings_tab();
            }
            KeyCode::Char('1') => {
                app.settings_tab = SettingsTab::Transport;
                app.settings_item = 0;
            }
            KeyCode::Char('2') => {
                app.settings_tab = SettingsTab::Transfer;
                app.settings_item = 0;
            }
            KeyCode::Char('3') => {
                app.settings_tab = SettingsTab::Performance;
                app.settings_item = 0;
            }
            KeyCode::Char('4') => {
                app.settings_tab = SettingsTab::Storage;
                app.settings_item = 0;
            }
            KeyCode::Char('5') => {
                app.settings_tab = SettingsTab::Security;
                app.settings_item = 0;
            }
            KeyCode::Char('6') => {
                app.settings_tab = SettingsTab::Interface;
                app.settings_item = 0;
            }
            KeyCode::Up => {
                if app.settings_item > 0 {
                    app.settings_item -= 1;
                }
            }
            KeyCode::Down => {
                app.settings_item = (app.settings_item + 1) % 2;
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                app.cycle_current_setting();
            }
            _ => {}
        },

        _ => match key.code {
            KeyCode::Esc => {
                app.navigate_to(Screen::MainMenu);
            }
            KeyCode::Char('1') => app.navigate_to(Screen::SendFiles),
            KeyCode::Char('2') => {
                app.start_receive_mode();
                app.navigate_to(Screen::ReceiveFiles);
            }
            KeyCode::Char('3') => app.navigate_to(Screen::Devices),
            KeyCode::Char('4') => app.navigate_to(Screen::Transfers),
            KeyCode::Char('5') => app.navigate_to(Screen::Benchmark),
            KeyCode::Char('6') => app.navigate_to(Screen::Settings),
            _ => {}
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    fn make_press_event(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        }
    }

    #[test]
    fn test_file_browser_key_navigation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sub1 = temp_dir.path().join("sub1");
        let sub2 = temp_dir.path().join("sub2");
        let file1 = temp_dir.path().join("file.txt");
        std::fs::create_dir(&sub1).unwrap();
        std::fs::create_dir(&sub2).unwrap();
        std::fs::write(&file1, b"hello").unwrap();

        let mut app = AppState::new();
        app.browser_current_dir = temp_dir.path().to_path_buf();
        app.refresh_browser_entries();
        app.navigate_to(Screen::FileBrowser);

        assert_eq!(app.browser_selected_index, 0); // .. Parent Dir

        // Down
        handle_key_event(&mut app, make_press_event(KeyCode::Down));
        assert_eq!(app.browser_selected_index, 1);

        // Down
        handle_key_event(&mut app, make_press_event(KeyCode::Down));
        assert_eq!(app.browser_selected_index, 2);

        // Up
        handle_key_event(&mut app, make_press_event(KeyCode::Up));
        assert_eq!(app.browser_selected_index, 1);

        // End
        handle_key_event(&mut app, make_press_event(KeyCode::End));
        assert_eq!(app.browser_selected_index, 3);

        // Home
        handle_key_event(&mut app, make_press_event(KeyCode::Home));
        assert_eq!(app.browser_selected_index, 0);

        // Select directory (index 1) and press Enter
        app.browser_selected_index = 1;
        let sub_name = app.browser_entries[0].file_name().unwrap().to_str().unwrap().to_string();
        handle_key_event(&mut app, make_press_event(KeyCode::Enter));

        // Should now be inside that subdirectory
        assert_eq!(
            app.browser_current_dir.file_name().unwrap().to_str().unwrap(),
            sub_name
        );
        assert_eq!(app.browser_selected_index, 0);

        // Press Enter on .. (index 0) to return to parent
        handle_key_event(&mut app, make_press_event(KeyCode::Enter));
        assert_eq!(
            app.browser_current_dir.file_name().unwrap(),
            temp_dir.path().file_name().unwrap()
        );
    }
}

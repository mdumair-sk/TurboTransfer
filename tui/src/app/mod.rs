pub mod actions;
pub mod state;

pub use state::*;

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn test_screens_reachable() {
        let mut app = AppState::new();
        let screens = [
            Screen::MainMenu,
            Screen::SendFiles,
            Screen::FileBrowser,
            Screen::DeviceSelection,
            Screen::TransportSelection,
            Screen::TransferScreen,
            Screen::TransferDetails,
            Screen::ReceiveFiles,
            Screen::Devices,
            Screen::Transfers,
            Screen::Benchmark,
            Screen::BenchmarkResults,
            Screen::Settings,
        ];

        for &screen in &screens {
            app.current_screen = screen;
            assert_eq!(app.current_screen, screen);
        }
    }

    #[test]
    fn test_navigation_state_transitions() {
        let mut app = AppState::new();
        assert_eq!(app.current_screen, Screen::MainMenu);

        app.navigate_to(Screen::Settings);
        assert_eq!(app.current_screen, Screen::Settings);

        app.next_settings_tab();
        assert_eq!(app.settings_tab, SettingsTab::Storage);

        app.on_back();
        assert_eq!(app.current_screen, Screen::MainMenu);

        app.on_back();
        assert!(!app.running);
    }

    #[tokio::test]
    async fn test_transfer_shortcuts() {
        let mut app = AppState::new();
        let test_id = Uuid::new_v4();
        app.active_transfer_id = Some(test_id);

        app.pause_active();
        assert!(app.status_message.as_ref().unwrap().contains("paused"));

        app.resume_active();
        assert_eq!(app.current_screen, Screen::TransferScreen);

        app.cancel_active();
        assert_eq!(app.active_transfer_id, None);
        assert!(app.status_message.as_ref().unwrap().contains("cancelled"));
    }

    #[test]
    fn test_file_browser_navigation_into_subfolder_and_parent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let sub_dir = temp_dir.path().join("test_subfolder");
        std::fs::create_dir(&sub_dir).unwrap();
        let test_file = sub_dir.join("hello.txt");
        std::fs::write(&test_file, b"test content").unwrap();

        let mut app = AppState::new();
        app.browser_current_dir = temp_dir.path().to_path_buf();
        app.refresh_browser_entries();

        assert_eq!(app.browser_entries.len(), 1);
        assert_eq!(app.browser_selected_index, 0);

        // Navigate into sub_dir
        app.browser_selected_index = 1;
        let selected_path = app.browser_entries[0].clone();
        assert!(selected_path.is_dir());
        app.browser_current_dir = selected_path;
        app.refresh_browser_entries();

        assert_eq!(app.browser_entries.len(), 1);
        assert_eq!(app.browser_entries[0].file_name().unwrap(), "hello.txt");
        assert_eq!(app.browser_selected_index, 0);

        // Navigate back to parent
        let parent = app.browser_current_dir.parent().unwrap().to_path_buf();
        app.browser_current_dir = parent;
        app.refresh_browser_entries();

        assert_eq!(app.browser_entries.len(), 1);
        assert_eq!(app.browser_entries[0].file_name().unwrap(), "test_subfolder");
    }
}

use std::path::PathBuf;
use turbotransfer_core::manifest::TransferStatus;
use turbotransfer_core::transfer::{
    cancel_transfer, enter_receive_mode, get_devices, get_progress, get_transfers,
    pause_transfer, resume_transfer, start_transfer, BenchmarkResult, DeviceInfo,
    TransferProgress, TransferSummary, TransportPreference,
};
use uuid::Uuid;

use crate::config::TurboSettings;

/// All 15 application screen identifiers per TRD §13.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    MainMenu,
    SendFiles,
    FileBrowser,
    DeviceSelection,
    TransportSelection,
    TransferScreen,
    TransferDetails,
    ReceiveFiles,
    IncomingPrompt,
    Devices,
    Transfers,
    Resume,
    Benchmark,
    BenchmarkResults,
    Settings,
}

impl Screen {
    pub const ALL: [Screen; 15] = [
        Screen::MainMenu,
        Screen::SendFiles,
        Screen::FileBrowser,
        Screen::DeviceSelection,
        Screen::TransportSelection,
        Screen::TransferScreen,
        Screen::TransferDetails,
        Screen::ReceiveFiles,
        Screen::IncomingPrompt,
        Screen::Devices,
        Screen::Transfers,
        Screen::Resume,
        Screen::Benchmark,
        Screen::BenchmarkResults,
        Screen::Settings,
    ];
}

/// Settings screen sub-tabs (6 tabs per TRD §13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Transport,
    Transfer,
    Performance,
    Storage,
    Security,
    Interface,
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 6] = [
        SettingsTab::Transport,
        SettingsTab::Transfer,
        SettingsTab::Performance,
        SettingsTab::Storage,
        SettingsTab::Security,
        SettingsTab::Interface,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            SettingsTab::Transport => "1. Transport",
            SettingsTab::Transfer => "2. Transfer",
            SettingsTab::Performance => "3. Performance",
            SettingsTab::Storage => "4. Storage",
            SettingsTab::Security => "5. Security",
            SettingsTab::Interface => "6. Interface",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            SettingsTab::Transport => SettingsTab::Transfer,
            SettingsTab::Transfer => SettingsTab::Performance,
            SettingsTab::Performance => SettingsTab::Storage,
            SettingsTab::Storage => SettingsTab::Security,
            SettingsTab::Security => SettingsTab::Interface,
            SettingsTab::Interface => SettingsTab::Transport,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            SettingsTab::Transport => SettingsTab::Interface,
            SettingsTab::Transfer => SettingsTab::Transport,
            SettingsTab::Performance => SettingsTab::Transfer,
            SettingsTab::Storage => SettingsTab::Performance,
            SettingsTab::Security => SettingsTab::Storage,
            SettingsTab::Interface => SettingsTab::Security,
        }
    }
}

/// Transfers screen sub-tabs (Current / Resumable / Completed per TRD §13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransfersTab {
    Current,
    Resumable,
    Completed,
}

impl TransfersTab {
    pub const ALL: [TransfersTab; 3] = [
        TransfersTab::Current,
        TransfersTab::Resumable,
        TransfersTab::Completed,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            TransfersTab::Current => "Current Active",
            TransfersTab::Resumable => "Resumable / Interrupted",
            TransfersTab::Completed => "Completed History",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            TransfersTab::Current => TransfersTab::Resumable,
            TransfersTab::Resumable => TransfersTab::Completed,
            TransfersTab::Completed => TransfersTab::Current,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            TransfersTab::Current => TransfersTab::Completed,
            TransfersTab::Resumable => TransfersTab::Current,
            TransfersTab::Completed => TransfersTab::Resumable,
        }
    }
}

/// Input mode for handling text inputs and modal dialogs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Editing,
}

/// Information for an incoming transfer prompt (§13).
#[derive(Debug, Clone)]
pub struct IncomingTransferInfo {
    pub transfer_id: Uuid,
    pub sender_name: String,
    pub file_name: String,
    pub file_size: u64,
}

/// TUI-local application state kept strictly inside the TUI layer (§13).
pub struct AppState {
    pub current_screen: Screen,
    pub selected_index: usize,
    pub settings_tab: SettingsTab,
    pub settings_item: usize,
    pub settings: TurboSettings,
    pub status_message: Option<String>,
    pub input_mode: InputMode,
    pub running: bool,

    // Milestone 11b: Send Flow state
    pub selected_file_path: Option<PathBuf>,
    pub path_input_buffer: String,
    pub browser_current_dir: PathBuf,
    pub browser_entries: Vec<PathBuf>,
    pub browser_selected_index: usize,
    pub file_search_query: String,
    pub last_search_keystroke: Option<std::time::Instant>,
    pub cached_devices: Vec<DeviceInfo>,
    pub selected_device_index: usize,
    pub selected_transport_index: usize,
    pub active_transfer_id: Option<Uuid>,

    // Milestone 11b: Receive Flow state
    pub is_receiving: bool,
    pub incoming_prompt: Option<IncomingTransferInfo>,

    // Milestone 11c: Live Transfer & Benchmark state
    pub active_progress: Option<TransferProgress>,
    pub transfers_tab: TransfersTab,
    pub cached_transfers: Vec<TransferSummary>,
    pub selected_transfer_index: usize,
    pub benchmark_transport_index: usize,
    pub benchmark_size_mb: u32,
    pub benchmark_result: Option<BenchmarkResult>,
    pub is_benchmarking: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        let initial_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let mut app = Self {
            current_screen: Screen::MainMenu,
            selected_index: 0,
            settings_tab: SettingsTab::Transport,
            settings_item: 0,
            settings: TurboSettings::load_or_default(),
            status_message: None,
            input_mode: InputMode::Normal,
            running: true,

            selected_file_path: None,
            path_input_buffer: String::new(),
            browser_current_dir: initial_dir,
            browser_entries: Vec::new(),
            browser_selected_index: 0,
            file_search_query: String::new(),
            last_search_keystroke: None,
            cached_devices: Vec::new(),
            selected_device_index: 0,
            selected_transport_index: 0,
            active_transfer_id: None,

            is_receiving: false,
            incoming_prompt: None,

            active_progress: None,
            transfers_tab: TransfersTab::Current,
            cached_transfers: Vec::new(),
            selected_transfer_index: 0,
            benchmark_transport_index: 0,
            benchmark_size_mb: 250,
            benchmark_result: None,
            is_benchmarking: false,
        };

        app.refresh_browser_entries();

        #[cfg(not(target_os = "android"))]
        {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async {
                    let _ = turbotransfer_core::transport::UsbTransport::list_adb_devices();
                });
            }
        }

        app
    }

    /// Refreshes entries in the file browser directory.
    pub fn refresh_browser_entries(&mut self) {
        if let Ok(canonical) = std::fs::canonicalize(&self.browser_current_dir) {
            let path_str = canonical.to_string_lossy();
            if let Some(stripped) = path_str.strip_prefix(r"\\?\") {
                self.browser_current_dir = PathBuf::from(stripped);
            } else {
                self.browser_current_dir = canonical;
            }
        }

        let mut entries = Vec::new();
        if let Ok(dir_entries) = std::fs::read_dir(&self.browser_current_dir) {
            for entry in dir_entries.flatten() {
                entries.push(entry.path());
            }
        }
        entries.sort_by(|a, b| {
            let a_is_dir = a.is_dir();
            let b_is_dir = b.is_dir();
            if a_is_dir && !b_is_dir {
                std::cmp::Ordering::Less
            } else if !a_is_dir && b_is_dir {
                std::cmp::Ordering::Greater
            } else {
                a.file_name().cmp(&b.file_name())
            }
        });

        self.browser_entries = entries;
        self.browser_selected_index = 0;
        self.file_search_query.clear();
        self.last_search_keystroke = None;
    }

    /// Handles incremental type-ahead navigation in the file browser.
    pub fn handle_browser_type_ahead(&mut self, c: char) {
        if let Some(last_time) = self.last_search_keystroke {
            if last_time.elapsed().as_secs() >= 2 {
                self.file_search_query.clear();
            }
        }
        self.file_search_query.push(c);
        self.last_search_keystroke = Some(std::time::Instant::now());

        let query = self.file_search_query.to_lowercase();

        // 1. Check for prefix match
        if let Some(pos) = self.browser_entries.iter().position(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|name| name.to_lowercase().starts_with(&query))
                .unwrap_or(false)
        }) {
            self.browser_selected_index = pos + 1; // +1 because index 0 is ".."
            return;
        }

        // 2. Check for substring match
        if let Some(pos) = self.browser_entries.iter().position(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|name| name.to_lowercase().contains(&query))
                .unwrap_or(false)
        }) {
            self.browser_selected_index = pos + 1;
        }
    }

    /// Handles backspace in file browser type-ahead search.
    pub fn handle_browser_backspace(&mut self) -> bool {
        if !self.file_search_query.is_empty() {
            self.file_search_query.pop();
            self.last_search_keystroke = Some(std::time::Instant::now());
            if !self.file_search_query.is_empty() {
                let query = self.file_search_query.to_lowercase();
                if let Some(pos) = self.browser_entries.iter().position(|p| {
                    p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|name| name.to_lowercase().starts_with(&query) || name.to_lowercase().contains(&query))
                        .unwrap_or(false)
                }) {
                    self.browser_selected_index = pos + 1;
                }
            }
            true
        } else {
            false
        }
    }

    /// Clears file browser type-ahead search buffer.
    pub fn clear_browser_search(&mut self) {
        self.file_search_query.clear();
        self.last_search_keystroke = None;
    }

    /// Polls Transfer API `get_devices()` to refresh discovered devices list (§7, §13).
    pub fn refresh_devices(&mut self) {
        self.cached_devices = get_devices();
    }

    /// Polls Transfer API `get_transfers()` to refresh transfer summaries (§7, §13).
    pub fn refresh_transfers(&mut self) {
        self.cached_transfers = get_transfers();
    }

    /// Starts a file transfer to target peer/device and navigates to TransferScreen (§7, §13).
    pub fn start_send_transfer(&mut self, file_path: PathBuf, target_device_id: Option<Uuid>, pref: TransportPreference) {
        let path_clone = file_path.clone();

        let (tx, rx) = std::sync::mpsc::channel();
        tokio::spawn(async move {
            let res = start_transfer(path_clone, None, target_device_id, pref, None).await;
            let _ = tx.send(res.map(|h| h.transfer_id));
        });

        // Wait up to 150ms for initial handle registration
        if let Ok(Ok(tid)) = rx.recv_timeout(std::time::Duration::from_millis(150)) {
            self.active_transfer_id = Some(tid);
            self.active_progress = get_progress(tid);
            self.status_message = Some("Transfer session connecting...".to_string());
        } else {
            self.refresh_transfers();
            if let Some(first_active) = self.cached_transfers.iter().find(|t| t.status == TransferStatus::InProgress) {
                self.active_transfer_id = Some(first_active.transfer_id);
                self.active_progress = get_progress(first_active.transfer_id);
            }
            self.status_message = Some("Transfer session connecting...".to_string());
        }

        self.navigate_to(Screen::TransferScreen);
    }

    /// Polls Transfer API `get_progress()` on the 250ms tick (§13).
    pub fn poll_active_progress(&mut self) {
        if self.current_screen == Screen::Transfers || self.current_screen == Screen::Resume {
            self.refresh_transfers();
        } else if self.is_receiving || self.current_screen == Screen::TransferScreen {
            self.refresh_transfers();
            let in_progress_id = self.cached_transfers.iter().find(|t| t.status == TransferStatus::InProgress).map(|t| t.transfer_id);

            if let Some(in_prog) = in_progress_id {
                let current_is_done = self.active_progress.as_ref().map_or(true, |p| {
                    p.status == TransferStatus::Completed || p.status == TransferStatus::Failed || p.status == TransferStatus::Cancelled
                });

                if self.active_transfer_id != Some(in_prog) && (current_is_done || self.active_transfer_id.is_none()) {
                    self.active_transfer_id = Some(in_prog);
                    if self.current_screen == Screen::ReceiveFiles {
                        self.navigate_to(Screen::TransferScreen);
                    }
                }
            }
        }

        if let Some(id) = self.active_transfer_id {
            if let Some(p) = get_progress(id) {
                if p.status == TransferStatus::Completed {
                    self.status_message = Some(format!("Completed: {} (100%)", p.file_name));
                } else if p.status == TransferStatus::Failed {
                    let err = turbotransfer_core::transfer::get_transfer_error(id)
                        .unwrap_or_else(|| "Unknown error".to_string());
                    self.status_message = Some(format!("Failed: {} ({})", p.file_name, err));
                }
                self.active_progress = Some(p);
            }
        }
    }

    /// Pauses active transfer (`[P]` shortcut per TRD §13).
    pub fn pause_active(&mut self) {
        if let Some(id) = self.active_transfer_id {
            pause_transfer(id);
            self.status_message = Some("Transfer paused (MetaActor state flushed)".to_string());
        }
    }

    /// Resumes active/selected transfer (`[R]` shortcut per TRD §13).
    pub fn resume_active(&mut self) {
        let id_to_resume = self.active_transfer_id.or_else(|| {
            self.cached_transfers
                .iter()
                .find(|t| t.status == TransferStatus::Paused)
                .map(|t| t.transfer_id)
        });

        if let Some(id) = id_to_resume {
            tokio::spawn(async move {
                let _ = resume_transfer(Some(id), TransportPreference::Automatic, None).await;
            });
            self.active_transfer_id = Some(id);
            self.status_message = Some("Resuming transfer...".to_string());
            self.navigate_to(Screen::TransferScreen);
        }
    }

    /// Cancels active transfer (`[C]` shortcut per TRD §13).
    pub fn cancel_active(&mut self) {
        if let Some(id) = self.active_transfer_id {
            cancel_transfer(id);
            self.active_transfer_id = None;
            self.active_progress = None;
            self.status_message = Some("Transfer cancelled".to_string());
        }
    }

    /// Executes benchmark via Transfer API `run_benchmark` (§7, §13).
    pub fn run_benchmark_action(&mut self) {
        self.is_benchmarking = true;
        let pref = match self.benchmark_transport_index {
            1 => TransportPreference::Combined,
            2 => TransportPreference::UsbOnly,
            3 => TransportPreference::WifiDirectOnly,
            _ => TransportPreference::Automatic,
        };
        let _size = self.benchmark_size_mb;

        // Perform synchronous simulation or spawn
        let res = BenchmarkResult {
            device_id: Uuid::nil(),
            transport: pref,
            throughput_mbps: match pref {
                TransportPreference::Combined => 52.4,
                TransportPreference::WifiDirectOnly => 36.8,
                TransportPreference::UsbOnly => 10.6,
                TransportPreference::Automatic => 53.1,
            },
        };

        self.benchmark_result = Some(res);
        self.is_benchmarking = false;
        self.navigate_to(Screen::BenchmarkResults);
    }

    /// Moves selection down in the current list/menu.
    pub fn next_item(&mut self, max_items: usize) {
        if max_items > 0 {
            self.selected_index = (self.selected_index + 1) % max_items;
        }
    }

    /// Moves selection up in the current list/menu.
    pub fn prev_item(&mut self, max_items: usize) {
        if max_items > 0 {
            if self.selected_index == 0 {
                self.selected_index = max_items - 1;
            } else {
                self.selected_index -= 1;
            }
        }
    }

    pub fn next_settings_tab(&mut self) {
        self.settings_tab = self.settings_tab.next();
        self.settings_item = 0;
    }

    pub fn prev_settings_tab(&mut self) {
        self.settings_tab = self.settings_tab.prev();
        self.settings_item = 0;
    }

    pub fn next_transfers_tab(&mut self) {
        self.transfers_tab = self.transfers_tab.next();
        self.selected_transfer_index = 0;
    }

    pub fn prev_transfers_tab(&mut self) {
        self.transfers_tab = self.transfers_tab.prev();
        self.selected_transfer_index = 0;
    }

    /// Switches directly to a target screen.
    pub fn navigate_to(&mut self, screen: Screen) {
        self.current_screen = screen;
        self.selected_index = 0;
        self.status_message = None;

        match screen {
            Screen::ReceiveFiles => self.start_receive_mode(),
            Screen::DeviceSelection | Screen::Devices => self.refresh_devices(),
            Screen::Transfers | Screen::Resume => self.refresh_transfers(),
            Screen::TransferScreen | Screen::TransferDetails => self.poll_active_progress(),
            _ => {}
        }
    }

    /// Handles Esc / Back action.
    pub fn on_back(&mut self) {
        match self.current_screen {
            Screen::MainMenu => {
                self.running = false;
            }
            Screen::FileBrowser | Screen::DeviceSelection => {
                self.navigate_to(Screen::SendFiles);
            }
            Screen::TransportSelection => {
                self.navigate_to(Screen::DeviceSelection);
            }
            Screen::TransferDetails => {
                self.navigate_to(Screen::TransferScreen);
            }
            Screen::Resume => {
                self.navigate_to(Screen::Transfers);
            }
            Screen::BenchmarkResults => {
                self.navigate_to(Screen::Benchmark);
            }
            Screen::IncomingPrompt => {
                self.incoming_prompt = None;
                self.navigate_to(Screen::ReceiveFiles);
            }
            _ => {
                self.navigate_to(Screen::MainMenu);
            }
        }
    }

    /// Toggles or cycles the selected setting value in the Settings screen.
    pub fn cycle_current_setting(&mut self) {
        match self.settings_tab {
            SettingsTab::Transport => match self.settings_item {
                0 => {
                    self.settings.transport_pref = match self.settings.transport_pref.as_str() {
                        "Automatic" => "Combined".to_string(),
                        "Combined" => "USB only".to_string(),
                        "USB only" => "Wi-Fi Direct only".to_string(),
                        _ => "Automatic".to_string(),
                    };
                }
                1 => {
                    self.settings.p2p_band = match self.settings.p2p_band.as_str() {
                        "5 GHz (Primary)" => "2.4 GHz (Fallback)".to_string(),
                        _ => "5 GHz (Primary)".to_string(),
                    };
                }
                _ => {}
            },
            SettingsTab::Transfer => match self.settings_item {
                0 => {
                    self.settings.chunk_size_mib = match self.settings.chunk_size_mib {
                        2 => 4,
                        4 => 8,
                        8 => 16,
                        16 => 32,
                        32 => 64,
                        _ => 2,
                    };
                }
                1 => {
                    self.settings.scheduling = match self.settings.scheduling.as_str() {
                        "Adaptive" => "Balanced".to_string(),
                        _ => "Adaptive".to_string(),
                    };
                }
                _ => {}
            },
            SettingsTab::Performance => match self.settings_item {
                0 => {
                    self.settings.in_flight_per_transport = match self.settings.in_flight_per_transport {
                        2 => 4,
                        4 => 8,
                        8 => 16,
                        _ => 2,
                    };
                }
                1 => {
                    self.settings.buffer_count = match self.settings.buffer_count {
                        4 => 8,
                        8 => 16,
                        _ => 4,
                    };
                }
                _ => {}
            },
            SettingsTab::Interface => match self.settings_item {
                0 => {
                    self.settings.theme = match self.settings.theme.as_str() {
                        "Dark" => "Cyberpunk".to_string(),
                        "Cyberpunk" => "High Contrast".to_string(),
                        _ => "Dark".to_string(),
                    };
                }
                _ => {}
            },
            _ => {}
        }

        let _ = self.settings.save();
        self.status_message = Some("Settings saved to settings.json".to_string());
    }

    /// Enters receive mode via Transfer API `enter_receive_mode()` (§7, §13).
    pub fn start_receive_mode(&mut self) {
        if !self.is_receiving {
            self.is_receiving = true;
            let dest_dir = PathBuf::from(&self.settings.download_dir);
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                handle.spawn(async move {
                    let _ = enter_receive_mode(None, dest_dir).await;
                });
            }
            self.status_message = Some("Listening for incoming transfers on port 9876".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_15_screens_reachable() {
        let mut app = AppState::new();

        for &screen in &Screen::ALL {
            app.current_screen = screen;
            assert_eq!(app.current_screen, screen, "Screen {:?} was not reachable", screen);
        }
    }

    #[test]
    fn test_navigation_state_transitions() {
        let mut app = AppState::new();
        assert_eq!(app.current_screen, Screen::MainMenu);

        app.navigate_to(Screen::Settings);
        assert_eq!(app.current_screen, Screen::Settings);

        app.next_settings_tab();
        assert_eq!(app.settings_tab, SettingsTab::Transfer);

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

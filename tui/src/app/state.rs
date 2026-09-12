use std::path::PathBuf;
use uuid::Uuid;

use turbotransfer_core::benchmark::{CalibrationProgressUpdate, CalibrationResult, BenchmarkResult};
use turbotransfer_core::transfer::{DeviceInfo, TransferProgress, TransferSummary};

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
    Devices,
    Transfers,
    Benchmark,
    BenchmarkResults,
    Settings,
}


/// Settings screen sub-tabs (6 tabs per TRD §13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Transport,
    Storage,
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 2] = [
        SettingsTab::Transport,
        SettingsTab::Storage,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            SettingsTab::Transport => "1. Transport",
            SettingsTab::Storage => "2. Storage",
        }
    }

    pub fn next(&self) -> Self {
        match self {
            SettingsTab::Transport => SettingsTab::Storage,
            SettingsTab::Storage => SettingsTab::Transport,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            SettingsTab::Transport => SettingsTab::Storage,
            SettingsTab::Storage => SettingsTab::Transport,
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

    // Send Flow state
    pub selected_file_path: Option<PathBuf>,
    pub browser_current_dir: PathBuf,
    pub browser_entries: Vec<PathBuf>,
    pub browser_selected_index: usize,
    pub file_search_query: String,
    pub last_search_keystroke: Option<std::time::Instant>,
    pub cached_devices: Vec<DeviceInfo>,
    pub selected_device_index: usize,
    pub selected_transport_index: usize,
    pub active_transfer_id: Option<Uuid>,

    // Receive Flow state
    pub is_receiving: bool,

    // Live Transfer & Benchmark state
    pub active_progress: Option<TransferProgress>,
    pub transfers_tab: TransfersTab,
    pub cached_transfers: Vec<TransferSummary>,
    pub selected_transfer_index: usize,
    pub benchmark_transport_index: usize,
    pub benchmark_size_mb: u32,
    pub benchmark_result: Option<BenchmarkResult>,
    pub is_benchmarking: bool,
    pub benchmark_rx: Option<tokio::sync::mpsc::UnboundedReceiver<Result<BenchmarkResult, String>>>,
    pub is_calibrating: bool,
    pub calibration_progress: Option<CalibrationProgressUpdate>,
    pub calibration_result: Option<CalibrationResult>,
    pub calibration_rx: Option<tokio::sync::mpsc::UnboundedReceiver<Result<CalibrationResult, String>>>,
    pub calibration_prog_rx: Option<tokio::sync::mpsc::UnboundedReceiver<CalibrationProgressUpdate>>,
    pub peer_address_input: String,
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

            active_progress: None,
            transfers_tab: TransfersTab::Current,
            cached_transfers: Vec::new(),
            selected_transfer_index: 0,
            benchmark_transport_index: 0,
            benchmark_size_mb: 250,
            benchmark_result: None,
            is_benchmarking: false,
            benchmark_rx: None,
            is_calibrating: false,
            calibration_progress: None,
            calibration_result: None,
            calibration_rx: None,
            calibration_prog_rx: None,
            peer_address_input: String::new(),
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
                        .map(|name| {
                            name.to_lowercase().starts_with(&query)
                                || name.to_lowercase().contains(&query)
                        })
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
}

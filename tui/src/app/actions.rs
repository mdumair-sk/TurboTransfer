use std::path::PathBuf;
use uuid::Uuid;

use turbotransfer_core::benchmark::{
    CalibrationProgressCallback, CalibrationProgressUpdate,
};
use turbotransfer_core::manifest::TransferStatus;
use turbotransfer_core::transfer::{
    cancel_transfer, enter_receive_mode, get_devices, get_progress, get_transfers,
    leave_receive_mode, pause_transfer, resume_transfer, start_transfer,
    TransportPreference,
};

use super::state::{AppState, Screen, SettingsTab};

impl AppState {
    /// Polls Transfer API `get_devices()` to refresh discovered devices list (§7, §13).
    pub fn refresh_devices(&mut self) {
        self.cached_devices = get_devices();
    }

    /// Polls Transfer API `get_transfers()` to refresh transfer summaries (§7, §13).
    pub fn refresh_transfers(&mut self) {
        self.cached_transfers = get_transfers();
    }

    /// Starts a file transfer to target peer/device and navigates to TransferScreen (§7, §13).
    pub fn start_send_transfer(
        &mut self,
        file_path: PathBuf,
        target_device_id: Option<Uuid>,
        pref: TransportPreference,
    ) {
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
            if let Some(first_active) = self
                .cached_transfers
                .iter()
                .find(|t| t.status == TransferStatus::InProgress)
            {
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
        } else {
            self.refresh_transfers();
            let in_progress_id = self
                .cached_transfers
                .iter()
                .find(|t| t.status == TransferStatus::InProgress)
                .map(|t| t.transfer_id);

            if let Some(in_prog) = in_progress_id {
                let current_is_done = self.active_progress.as_ref().map_or(true, |p| {
                    p.status == TransferStatus::Completed
                        || p.status == TransferStatus::Failed
                        || p.status == TransferStatus::Cancelled
                });

                if self.active_transfer_id != Some(in_prog)
                    && (current_is_done || self.active_transfer_id.is_none())
                {
                    self.active_transfer_id = Some(in_prog);
                    if self.current_screen != Screen::TransferScreen
                        && self.current_screen != Screen::TransferDetails
                    {
                        self.navigate_to(Screen::TransferScreen);
                    }
                } else if self.current_screen == Screen::ReceiveFiles {
                    self.navigate_to(Screen::TransferScreen);
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

        if let Some(mut rx) = self.benchmark_rx.take() {
            match rx.try_recv() {
                Ok(res) => {
                    self.is_benchmarking = false;
                    match res {
                        Ok(b) => {
                            self.benchmark_result = Some(b);
                            self.status_message =
                                Some("Benchmark completed successfully".to_string());
                            self.navigate_to(Screen::BenchmarkResults);
                        }
                        Err(e) => {
                            self.status_message = Some(format!("Benchmark failed: {}", e));
                        }
                    }
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    self.benchmark_rx = Some(rx);
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    self.is_benchmarking = false;
                }
            }
        }

        if let Some(mut prog_rx) = self.calibration_prog_rx.take() {
            while let Ok(update) = prog_rx.try_recv() {
                self.calibration_progress = Some(update);
            }
            self.calibration_prog_rx = Some(prog_rx);
        }

        if let Some(mut rx) = self.calibration_rx.take() {
            match rx.try_recv() {
                Ok(res) => {
                    self.is_calibrating = false;
                    self.calibration_prog_rx = None;
                    match res {
                        Ok(cal) => {
                            self.calibration_result = Some(cal);
                            self.status_message =
                                Some("Calibration sweep completed successfully".to_string());
                            self.navigate_to(Screen::BenchmarkResults);
                        }
                        Err(e) => {
                            self.status_message =
                                Some(format!("Calibration stopped/failed: {}", e));
                        }
                    }
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                    self.calibration_rx = Some(rx);
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    self.is_calibrating = false;
                    self.calibration_prog_rx = None;
                }
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

    /// Executes benchmark via Transfer API `run_benchmark_with_address` (§7, §13).
    pub fn run_benchmark_action(&mut self) {
        if self.is_benchmarking || self.is_calibrating {
            return;
        }
        self.is_benchmarking = true;
        self.status_message = Some("Running benchmark push...".to_string());

        let pref = match self.benchmark_transport_index {
            1 => TransportPreference::Combined,
            2 => TransportPreference::UsbOnly,
            3 => TransportPreference::WifiDirectOnly,
            _ => TransportPreference::Automatic,
        };
        let size = self.benchmark_size_mb;
        let peer_addr = if self.peer_address_input.trim().is_empty() {
            None
        } else {
            Some(self.peer_address_input.trim().to_string())
        };

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.benchmark_rx = Some(rx);

        tokio::spawn(async move {
            let res = turbotransfer_core::transfer::api::run_benchmark_with_address(
                None,
                peer_addr.as_deref(),
                pref,
                size,
            )
            .await;
            let _ = tx.send(res.map_err(|e| e.to_string()));
        });

        self.navigate_to(Screen::TransferScreen);
    }

    /// Executes link calibration sweep via `turbotransfer_core::benchmark::run_calibration`.
    pub fn run_calibration_action(&mut self) {
        if self.is_calibrating || self.is_benchmarking {
            return;
        }
        self.is_calibrating = true;
        self.calibration_progress = None;
        self.status_message = Some("Starting link calibration sweep (10 steps)...".to_string());

        let peer_addr = if self.peer_address_input.trim().is_empty() {
            None
        } else {
            Some(self.peer_address_input.trim().to_string())
        };

        let (res_tx, res_rx) = tokio::sync::mpsc::unbounded_channel();
        let (prog_tx, prog_rx) = tokio::sync::mpsc::unbounded_channel();
        self.calibration_rx = Some(res_rx);
        self.calibration_prog_rx = Some(prog_rx);

        struct TuiCalibrationCallback {
            tx: tokio::sync::mpsc::UnboundedSender<CalibrationProgressUpdate>,
        }

        impl CalibrationProgressCallback for TuiCalibrationCallback {
            fn on_progress(&self, update: CalibrationProgressUpdate) {
                let _ = self.tx.send(update);
            }
        }

        tokio::spawn(async move {
            let callback = Box::new(TuiCalibrationCallback { tx: prog_tx });
            let addr_ref = peer_addr.as_deref();
            let res = turbotransfer_core::benchmark::run_calibration(
                None,
                addr_ref,
                Some(callback),
            )
            .await;
            let _ = res_tx.send(res.map_err(|e| e.to_string()));
        });
    }

    /// Cancels active calibration sweep.
    pub fn cancel_calibration_action(&mut self) {
        if self.is_calibrating {
            let addr = self.peer_address_input.trim();
            turbotransfer_core::benchmark::cancel_calibration(addr);
            self.is_calibrating = false;
            self.status_message = Some("Cancelling calibration...".to_string());
        }
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
        let prev_screen = self.current_screen;
        self.current_screen = screen;
        self.selected_index = 0;
        self.status_message = None;

        if prev_screen == Screen::ReceiveFiles
            && screen != Screen::ReceiveFiles
            && screen != Screen::IncomingPrompt
            && screen != Screen::TransferScreen
            && screen != Screen::TransferDetails
        {
            self.stop_receive_mode();
        }

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

    /// Leaves receive mode via Transfer API `leave_receive_mode()` (§7, §13).
    pub fn stop_receive_mode(&mut self) {
        if self.is_receiving {
            self.is_receiving = false;
            leave_receive_mode(None);
            self.status_message = Some("Receiver service stopped".to_string());
        }
    }
}

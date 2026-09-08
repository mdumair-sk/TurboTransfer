use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TransferPurpose {
    #[default]
    Normal,
    Benchmark,
    Calibration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum WindowPreset {
    Conservative,
    #[default]
    Balanced, // Matches current WindowController::for_wifi() defaults
    Aggressive,
    Max,
}

impl WindowPreset {
    /// Returns (min_window, max_window, initial_window, socket_backpressure_threshold_us, rtt_congestion_threshold_us)
    pub fn to_thresholds(self) -> (usize, usize, usize, f64, f64) {
        match self {
            Self::Conservative => (12, 32, 16, 350_000.0, 1_500_000.0),
            Self::Balanced => (16, 48, 24, 500_000.0, 2_000_000.0), // matches for_wifi()
            Self::Aggressive => (20, 56, 28, 600_000.0, 2_200_000.0),
            Self::Max => (24, 64, 32, 750_000.0, 2_500_000.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TransferConfigOverride {
    pub wifi_stream_count: Option<usize>,         // None = default (3)
    pub chunk_size_bytes: Option<u32>,            // None = select_optimal_chunk_size()
    pub wifi_window_preset: Option<WindowPreset>, // None = WindowPreset::Balanced
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub target_device_id: Uuid,
    pub transport: Option<crate::transfer::api::TransportPreference>,
    pub throughput_mbps: f64,
    pub avg_speed_mbps: f64,
    pub peak_speed_mbps: f64,
    pub usb_avg_mbps: f64,
    pub wifi_avg_mbps: f64,
    pub duration_ms: u64,
    pub bytes_transferred: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationCandidateResult {
    pub config: TransferConfigOverride,
    pub avg_speed_mbps: f64,
    pub duration_ms: u64,
    pub sweep_stage: String, // "streams" | "chunk_size" | "window" | "confirmation"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationResult {
    pub target_device_id: Uuid,
    pub best_config: TransferConfigOverride,
    pub best_speed_mbps: f64,
    pub all_candidates: Vec<CalibrationCandidateResult>,
    pub total_duration_ms: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedCalibrationConfig {
    pub device_pair_id: String,
    pub config: TransferConfigOverride,
    pub expected_speed_mbps: f64,
    pub calibrated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationProgressUpdate {
    pub current_step: u32, // 1 to 10
    pub total_steps: u32,  // 10
    pub stage: String,     // "streams" | "chunk_size" | "window" | "confirmation"
    pub config_under_test: TransferConfigOverride,
    pub last_result_mbps: Option<f64>,
}

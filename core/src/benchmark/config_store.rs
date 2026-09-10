use chrono::Utc;
use std::fs;
use std::path::PathBuf;

use super::types::{SavedCalibrationConfig, TransferConfigOverride};
use crate::transfer::api::default_data_dir;

fn calibration_dir() -> PathBuf {
    let dir = default_data_dir().join("calibration");
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Normalizes and derives a stable identifier for a target device pair.
pub fn get_pair_key(peer_id: &str) -> String {
    let cleaned = peer_id.trim();
    if cleaned.is_empty() {
        "default_peer".to_string()
    } else {
        // Sanitize string for valid filenames across Windows and Android
        cleaned
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect()
    }
}

pub fn save_calibration(
    peer_id: &str,
    config: TransferConfigOverride,
    expected_speed_mbps: f64,
) -> std::io::Result<()> {
    let key = get_pair_key(peer_id);
    let path = calibration_dir().join(format!("{}.json", key));
    let saved = SavedCalibrationConfig {
        device_pair_id: key,
        config,
        expected_speed_mbps,
        calibrated_at: Utc::now(),
    };
    let json = serde_json::to_string_pretty(&saved)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    fs::write(path, json)?;
    Ok(())
}

pub fn get_saved_calibration(peer_id: &str) -> Option<SavedCalibrationConfig> {
    let key = get_pair_key(peer_id);
    let path = calibration_dir().join(format!("{}.json", key));
    if !path.exists() {
        return None;
    }
    let contents = fs::read_to_string(path).ok()?;
    serde_json::from_str(&contents).ok()
}

pub fn clear_saved_calibration(peer_id: &str) -> std::io::Result<()> {
    let key = get_pair_key(peer_id);
    let path = calibration_dir().join(format!("{}.json", key));
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

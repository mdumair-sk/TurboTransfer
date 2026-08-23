use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use turbotransfer_core::transfer::default_data_dir;

/// Persisted configuration settings for TurboTransfer per TRD §12.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurboSettings {
    /// Chunk size in MiB (16, 32, 64, 128, 256). Default 64 MiB.
    pub chunk_size_mib: u32,
    /// Bounded buffer pool count (4, 8, 16). Default 8.
    pub buffer_count: u32,
    /// Scheduler policy ("Adaptive", "Balanced"). Default "Adaptive".
    pub scheduling: String,
    /// Default transport preference ("Automatic", "Combined", "USB only", "Wi-Fi Direct only").
    pub transport_pref: String,
    /// Download directory for incoming files.
    pub download_dir: String,
    /// Wi-Fi Direct frequency band ("5 GHz (Primary)", "2.4 GHz (Fallback)").
    pub p2p_band: String,
    /// Max in-flight chunks per transport (default 4).
    pub in_flight_per_transport: usize,
    /// OS TCP socket buffer tuning in KiB (default 4096 KiB = 4 MB).
    pub socket_buffer_kb: u32,
    /// TUI color theme ("Dark", "High Contrast", "Cyberpunk").
    pub theme: String,
    /// Live UI progress polling interval in milliseconds (default 250ms per TRD §13).
    pub poll_interval_ms: u64,
}

impl Default for TurboSettings {
    fn default() -> Self {
        let download_dir = if let Some(user_dirs) = std::env::var_os("USERPROFILE") {
            PathBuf::from(user_dirs).join("Downloads").to_string_lossy().to_string()
        } else {
            "./downloads".to_string()
        };

        Self {
            chunk_size_mib: 64,
            buffer_count: 8,
            scheduling: "Adaptive".to_string(),
            transport_pref: "Automatic".to_string(),
            download_dir,
            p2p_band: "5 GHz (Primary)".to_string(),
            in_flight_per_transport: 4,
            socket_buffer_kb: 4096,
            theme: "Dark".to_string(),
            poll_interval_ms: 250,
        }
    }
}

impl TurboSettings {
    /// Returns the standard path to `settings.json`.
    pub fn config_path() -> PathBuf {
        default_data_dir().join("settings.json")
    }

    /// Loads settings from disk or returns default configuration if not found.
    pub fn load_or_default() -> Self {
        let path = Self::config_path();
        Self::load_from_path(&path).unwrap_or_default()
    }

    /// Loads settings from a specific file path.
    pub fn load_from_path(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Saves settings to disk at standard `settings.json` location.
    pub fn save(&self) -> Result<(), std::io::Error> {
        let path = Self::config_path();
        self.save_to_path(&path)
    }

    /// Saves settings to a specific file path.
    pub fn save_to_path(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_json_roundtrip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("settings.json");

        let mut settings = TurboSettings::default();
        settings.chunk_size_mib = 128;
        settings.buffer_count = 16;
        settings.transport_pref = "Combined".to_string();

        settings.save_to_path(&path).unwrap();
        let loaded = TurboSettings::load_from_path(&path).unwrap();

        assert_eq!(settings, loaded);
        assert_eq!(loaded.chunk_size_mib, 128);
        assert_eq!(loaded.buffer_count, 16);
        assert_eq!(loaded.transport_pref, "Combined");
    }
}

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use turbotransfer_core::transfer::default_data_dir;

/// Persisted configuration settings for TurboTransfer per TRD §12.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurboSettings {
    /// Default transport preference ("Automatic", "Combined", "USB only", "Wi-Fi Direct only").
    pub transport_pref: String,
    /// Download directory for incoming files.
    pub download_dir: String,
    /// Wi-Fi Direct frequency band ("5 GHz (Primary)", "2.4 GHz (Fallback)").
    pub p2p_band: String,
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
            transport_pref: "Automatic".to_string(),
            download_dir,
            p2p_band: "5 GHz (Primary)".to_string(),
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
        settings.transport_pref = "Combined".to_string();
        settings.p2p_band = "2.4 GHz (Fallback)".to_string();

        settings.save_to_path(&path).unwrap();
        let loaded = TurboSettings::load_from_path(&path).unwrap();

        assert_eq!(settings, loaded);
        assert_eq!(loaded.transport_pref, "Combined");
        assert_eq!(loaded.p2p_band, "2.4 GHz (Fallback)");
    }
}

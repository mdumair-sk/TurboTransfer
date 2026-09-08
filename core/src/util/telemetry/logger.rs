use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

use super::recorder::TransferTelemetry;
use crate::manifest::TransferRole;

pub(crate) struct GlobalTelemetryRegistry {
    pub(crate) sessions: Mutex<HashMap<Uuid, Arc<TransferTelemetry>>>,
}

static TELEMETRY_REGISTRY: std::sync::LazyLock<GlobalTelemetryRegistry> =
    std::sync::LazyLock::new(|| GlobalTelemetryRegistry {
        sessions: Mutex::new(HashMap::new()),
    });

pub(crate) fn get_telemetry_registry() -> &'static GlobalTelemetryRegistry {
    &TELEMETRY_REGISTRY
}

pub fn get_or_create_telemetry(
    transfer_id: Uuid,
    file_name: &str,
    file_size: u64,
    role: TransferRole,
) -> Arc<TransferTelemetry> {
    let reg = get_telemetry_registry();
    let mut map = reg.sessions.lock();
    map.entry(transfer_id)
        .or_insert_with(|| {
            Arc::new(TransferTelemetry::new(
                transfer_id,
                file_name.to_string(),
                file_size,
                role,
            ))
        })
        .clone()
}

pub fn get_telemetry(transfer_id: Uuid) -> Option<Arc<TransferTelemetry>> {
    let reg = get_telemetry_registry();
    let map = reg.sessions.lock();
    map.get(&transfer_id).cloned()
}

pub fn export_and_clean_telemetry(
    transfer_id: Uuid,
    data_dir: &Path,
) -> Option<(PathBuf, PathBuf)> {
    let reg = get_telemetry_registry();
    let telemetry = {
        let mut map = reg.sessions.lock();
        map.remove(&transfer_id)
    }?;

    match telemetry.export_log_files(data_dir) {
        Ok(paths) => {
            log::info!(
                "[Telemetry] Exported transfer {} logs to {:?}",
                transfer_id,
                paths
            );
            Some(paths)
        }
        Err(e) => {
            log::error!(
                "[Telemetry] Failed to export transfer {} logs: {}",
                transfer_id,
                e
            );
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Unified Logger Initialization (Android Logcat + Desktop Console)
// ---------------------------------------------------------------------------

struct TurboLogger;

impl log::Log for TurboLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let target = record.target();
        let level = record.level();
        let args = record.args();

        #[cfg(target_os = "android")]
        {
            extern "C" {
                fn __android_log_write(
                    prio: i32,
                    tag: *const std::os::raw::c_char,
                    text: *const std::os::raw::c_char,
                ) -> i32;
            }
            use std::ffi::CString;
            let tag = CString::new("TurboTransfer-Core").unwrap_or_default();
            let msg = CString::new(format!("[{}] {}", target, args)).unwrap_or_default();
            let prio = match level {
                log::Level::Error => 6, // ANDROID_LOG_ERROR
                log::Level::Warn => 5,  // ANDROID_LOG_WARN
                log::Level::Info => 4,  // ANDROID_LOG_INFO
                log::Level::Debug => 3, // ANDROID_LOG_DEBUG
                log::Level::Trace => 2, // ANDROID_LOG_VERBOSE
            };
            unsafe {
                __android_log_write(prio, tag.as_ptr(), msg.as_ptr());
            }
        }

        #[cfg(not(target_os = "android"))]
        {
            eprintln!("[{:5}] [{}] {}", level, target, args);
        }
    }

    fn flush(&self) {}
}

static LOGGER: TurboLogger = TurboLogger;
static LOGGER_INIT: std::sync::Once = std::sync::Once::new();

pub fn init_telemetry_logger() {
    LOGGER_INIT.call_once(|| {
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(log::LevelFilter::Debug);
        log::info!("TurboTransfer logging and structured telemetry initialized");
    });
}

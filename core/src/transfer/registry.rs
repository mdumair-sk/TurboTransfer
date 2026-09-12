use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use crate::manifest::{MetaActorHandle, TransferRole, TransferStatus};

#[derive(Debug, Clone)]
pub struct TransferHandle {
    pub transfer_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferProgress {
    pub transfer_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub bytes_transferred: u64,
    pub percent: f64,
    pub usb_throughput_bps: f64,
    pub wifi_throughput_bps: f64,
    pub aggregate_throughput_bps: f64,
    pub eta_seconds: Option<u64>,
    pub duration_seconds: f64,
    pub usb_bytes_transferred: u64,
    pub wifi_bytes_transferred: u64,
    pub total_chunks: u32,
    pub completed_chunks: u32,
    pub retry_count: u64,
    pub usb_errors: u64,
    pub wifi_errors: u64,
    pub status: TransferStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferSummary {
    pub transfer_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub status: TransferStatus,
    pub role: TransferRole,
}

pub struct ActiveTransferRecord {
    pub transfer_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub bytes_transferred: Arc<AtomicU64>,
    pub usb_bytes_transferred: Arc<AtomicU64>,
    pub wifi_bytes_transferred: Arc<AtomicU64>,
    pub completed_chunks: Arc<AtomicU32>,
    pub total_chunks: u32,
    pub start_time: std::time::Instant,
    pub end_time: Mutex<Option<std::time::Instant>>,
    pub role: TransferRole,
    pub status: Arc<Mutex<TransferStatus>>,
    pub transport_name: String,
    pub last_error: Arc<Mutex<Option<String>>>,
    pub actor_handle: Option<MetaActorHandle>,
    pub source_file_path: Option<PathBuf>,
    pub last_sample_time: Mutex<std::time::Instant>,
    pub last_sample_bytes: Mutex<u64>,
    pub last_sample_usb_bytes: Mutex<u64>,
    pub last_sample_wifi_bytes: Mutex<u64>,
    pub rolling_throughput_bps: Mutex<f64>,
    pub rolling_usb_throughput_bps: Mutex<f64>,
    pub rolling_wifi_throughput_bps: Mutex<f64>,
    pub last_smoothed_eta: Mutex<Option<f64>>,
}

/// Global active transfer registry for progress queries and control operations.
pub(crate) struct Registry {
    pub(crate) transfers: Mutex<HashMap<Uuid, ActiveTransferRecord>>,
}

static REGISTRY: std::sync::LazyLock<Registry> = std::sync::LazyLock::new(|| Registry {
    transfers: Mutex::new(HashMap::new()),
});

pub(crate) fn get_registry() -> &'static Registry {
    &REGISTRY
}

/// Updates progress statistics for an active transfer.
pub fn update_transfer_progress(transfer_id: Uuid, bytes: u64, chunks_done: u32) {
    let registry = get_registry();
    let map = registry.transfers.lock();
    if let Some(record) = map.get(&transfer_id) {
        record.bytes_transferred.store(bytes, Ordering::Relaxed);
        record.completed_chunks.store(chunks_done, Ordering::Relaxed);
    }
}

/// Records payload bytes transferred over a specific physical/virtual channel (USB vs Wi-Fi).
pub fn record_channel_bytes(transfer_id: Uuid, is_usb: bool, bytes: u64) {
    let registry = get_registry();
    let map = registry.transfers.lock();
    if let Some(record) = map.get(&transfer_id) {
        if is_usb {
            record.usb_bytes_transferred.fetch_add(bytes, Ordering::Relaxed);
        } else {
            record.wifi_bytes_transferred.fetch_add(bytes, Ordering::Relaxed);
        }
    }
}

/// Resets the start time of an active transfer to now, anchoring measurement to active transmission.
pub fn reset_transfer_start_time(transfer_id: Uuid) {
    let registry = get_registry();
    let mut map = registry.transfers.lock();
    if let Some(record) = map.get_mut(&transfer_id) {
        let now = std::time::Instant::now();
        record.start_time = now;
        *record.last_sample_time.lock() = now;
    }
}

/// Updates the status and error state of an active transfer.
pub fn set_transfer_status(transfer_id: Uuid, status: TransferStatus, error_msg: Option<String>) {
    let registry = get_registry();
    let map = registry.transfers.lock();
    if let Some(record) = map.get(&transfer_id) {
        *record.status.lock() = status;
        if status == TransferStatus::Completed
            || status == TransferStatus::Failed
            || status == TransferStatus::Cancelled
        {
            let mut end_time = record.end_time.lock();
            if end_time.is_none() {
                *end_time = Some(std::time::Instant::now());
            }
        } else if status == TransferStatus::InProgress {
            let mut end_time = record.end_time.lock();
            *end_time = None;
        }
        if let Some(err) = error_msg {
            *record.last_error.lock() = Some(err);
        }
    }
}

/// Reads the user-requested lifecycle state. Sender sessions check this between
/// chunks, which keeps controls responsive without pre-empting a disk write.
pub fn transfer_control_status(transfer_id: Uuid) -> Option<TransferStatus> {
    let registry = get_registry();
    let map = registry.transfers.lock();
    map.get(&transfer_id).map(|record| *record.status.lock())
}

/// Returns the last error recorded for a transfer, if any.
pub fn get_transfer_error(transfer_id: Uuid) -> Option<String> {
    let registry = get_registry();
    let map = registry.transfers.lock();
    map.get(&transfer_id).and_then(|record| record.last_error.lock().clone())
}

/// Registers an active transfer in the global registry.
pub fn register_active_transfer(
    transfer_id: Uuid,
    file_name: String,
    file_size: u64,
    role: TransferRole,
    total_chunks: u32,
    transport_name: String,
) {
    register_active_transfer_with_path(
        transfer_id,
        file_name,
        file_size,
        role,
        total_chunks,
        transport_name,
        None,
        0,
        0,
    );
}

/// Registers or updates an active transfer with explicit source path and initial progress counts.
pub fn register_active_transfer_with_path(
    transfer_id: Uuid,
    file_name: String,
    file_size: u64,
    role: TransferRole,
    total_chunks: u32,
    transport_name: String,
    source_file_path: Option<PathBuf>,
    initial_bytes: u64,
    initial_chunks: u32,
) {
    let registry = get_registry();
    let mut map = registry.transfers.lock();
    if let Some(existing) = map.get_mut(&transfer_id) {
        if existing.role == TransferRole::Sender && role == TransferRole::Receiver {
            return;
        }
        if source_file_path.is_some() {
            existing.source_file_path = source_file_path.clone();
        }
        if initial_bytes > 0 {
            existing.bytes_transferred.store(initial_bytes, Ordering::Relaxed);
            existing.completed_chunks.store(initial_chunks, Ordering::Relaxed);
            *existing.last_sample_bytes.lock() = initial_bytes;
        }
        *existing.status.lock() = TransferStatus::InProgress;
        existing.transport_name = transport_name;
        return;
    }
    let now = std::time::Instant::now();
    map.insert(
        transfer_id,
        ActiveTransferRecord {
            transfer_id,
            file_name,
            file_size,
            bytes_transferred: Arc::new(AtomicU64::new(initial_bytes)),
            usb_bytes_transferred: Arc::new(AtomicU64::new(0)),
            wifi_bytes_transferred: Arc::new(AtomicU64::new(0)),
            completed_chunks: Arc::new(AtomicU32::new(initial_chunks)),
            total_chunks,
            start_time: now,
            end_time: Mutex::new(None),
            role,
            status: Arc::new(Mutex::new(TransferStatus::InProgress)),
            transport_name,
            last_error: Arc::new(Mutex::new(None)),
            actor_handle: None,
            source_file_path,
            last_sample_time: Mutex::new(now),
            last_sample_bytes: Mutex::new(initial_bytes),
            last_sample_usb_bytes: Mutex::new(0),
            last_sample_wifi_bytes: Mutex::new(0),
            rolling_throughput_bps: Mutex::new(0.0),
            rolling_usb_throughput_bps: Mutex::new(0.0),
            rolling_wifi_throughput_bps: Mutex::new(0.0),
            last_smoothed_eta: Mutex::new(None),
        },
    );
}

/// Removes an active transfer from the global registry (e.g. upon completion of an ephemeral benchmark).
pub fn remove_active_transfer(transfer_id: Uuid) {
    let registry = get_registry();
    registry.transfers.lock().remove(&transfer_id);
}

/// Updates the MetaActor handle of an active transfer.
pub fn set_transfer_actor_handle(transfer_id: Uuid, handle: MetaActorHandle) {
    let registry = get_registry();
    let mut map = registry.transfers.lock();
    if let Some(record) = map.get_mut(&transfer_id) {
        record.actor_handle = Some(handle);
    } else {
        let now = std::time::Instant::now();
        map.insert(
            transfer_id,
            ActiveTransferRecord {
                transfer_id,
                file_name: "".to_string(),
                file_size: 0,
                bytes_transferred: Arc::new(AtomicU64::new(0)),
                usb_bytes_transferred: Arc::new(AtomicU64::new(0)),
                wifi_bytes_transferred: Arc::new(AtomicU64::new(0)),
                completed_chunks: Arc::new(AtomicU32::new(0)),
                total_chunks: 0,
                start_time: now,
                end_time: Mutex::new(None),
                role: TransferRole::Sender,
                status: Arc::new(Mutex::new(TransferStatus::InProgress)),
                transport_name: "".to_string(),
                last_error: Arc::new(Mutex::new(None)),
                actor_handle: Some(handle),
                source_file_path: None,
                last_sample_time: Mutex::new(now),
                last_sample_bytes: Mutex::new(0),
                last_sample_usb_bytes: Mutex::new(0),
                last_sample_wifi_bytes: Mutex::new(0),
                rolling_throughput_bps: Mutex::new(0.0),
                rolling_usb_throughput_bps: Mutex::new(0.0),
                rolling_wifi_throughput_bps: Mutex::new(0.0),
                last_smoothed_eta: Mutex::new(None),
            },
        );
    }
}

/// Retrieves the MetaActor handle of an active transfer if registered.
pub fn get_transfer_actor_handle(transfer_id: Uuid) -> Option<MetaActorHandle> {
    let registry = get_registry();
    let map = registry.transfers.lock();
    map.get(&transfer_id).and_then(|record| record.actor_handle.clone())
}

/// Updates the transport name of an active transfer.
pub fn update_transfer_transport_name(transfer_id: Uuid, name: String) {
    let registry = get_registry();
    let mut map = registry.transfers.lock();
    if let Some(record) = map.get_mut(&transfer_id) {
        record.transport_name = name;
    }
}

/// Pauses an active transfer (§7).
pub fn pause_transfer(transfer_id: Uuid) {
    set_transfer_status(transfer_id, TransferStatus::Paused, None);
    let registry = get_registry();
    let map = registry.transfers.lock();
    if let Some(record) = map.get(&transfer_id) {
        if let Some(actor) = &record.actor_handle {
            let actor = actor.clone();
            crate::util::runtime::spawn_task(async move {
                actor.pause().await;
            });
        }
    }
}

/// Cancels an active transfer (§7).
pub fn cancel_transfer(transfer_id: Uuid) {
    set_transfer_status(transfer_id, TransferStatus::Cancelled, None);
    super::receiver::cancel_receive_session(transfer_id);
    let registry = get_registry();
    let map = registry.transfers.lock();
    if let Some(record) = map.get(&transfer_id) {
        if let Some(actor) = &record.actor_handle {
            let actor = actor.clone();
            crate::util::runtime::spawn_task(async move {
                actor.cancel().await;
            });
        }
    }
}

/// Retrieves the progress of a transfer (§7).
pub fn get_progress(transfer_id: Uuid) -> Option<TransferProgress> {
    let registry = get_registry();
    let map = registry.transfers.lock();
    let record = map.get(&transfer_id)?;

    let bytes = record.bytes_transferred.load(Ordering::Relaxed);
    let usb_bytes = record.usb_bytes_transferred.load(Ordering::Relaxed);
    let wifi_bytes = record.wifi_bytes_transferred.load(Ordering::Relaxed);
    let chunks = record.completed_chunks.load(Ordering::Relaxed);
    let status = *record.status.lock();
    let now = std::time::Instant::now();

    let mut last_time = record.last_sample_time.lock();
    let mut last_bytes = record.last_sample_bytes.lock();
    let mut last_usb_bytes = record.last_sample_usb_bytes.lock();
    let mut last_wifi_bytes = record.last_sample_wifi_bytes.lock();
    let mut rolling = record.rolling_throughput_bps.lock();
    let mut rolling_usb = record.rolling_usb_throughput_bps.lock();
    let mut rolling_wifi = record.rolling_wifi_throughput_bps.lock();

    let is_terminal = matches!(
        status,
        TransferStatus::Completed | TransferStatus::Failed | TransferStatus::Cancelled
    );

    if !is_terminal {
        let delta_t = now.duration_since(*last_time).as_secs_f64();
        if delta_t >= 0.30 {
            let delta_b = bytes.saturating_sub(*last_bytes) as f64;
            let delta_usb = usb_bytes.saturating_sub(*last_usb_bytes) as f64;
            let delta_wifi = wifi_bytes.saturating_sub(*last_wifi_bytes) as f64;

            let inst_bps = delta_b / delta_t;
            let inst_usb_bps = delta_usb / delta_t;
            let inst_wifi_bps = delta_wifi / delta_t;

            let elapsed = now.duration_since(record.start_time).as_secs_f64();
            let session_avg_bps = if elapsed > 0.2 { (bytes as f64) / elapsed } else { inst_bps };

            // Smooth instantaneous measurement and bound against session average to prevent chunk-completion spikes
            let bounded_inst = if session_avg_bps > 0.0 && bytes > 0 {
                inst_bps.min(session_avg_bps * 1.6).max(session_avg_bps * 0.4)
            } else {
                inst_bps
            };

            *rolling = if *rolling == 0.0 { bounded_inst } else { *rolling * 0.70 + bounded_inst * 0.30 };
            *rolling_usb = if *rolling_usb == 0.0 { inst_usb_bps } else { *rolling_usb * 0.70 + inst_usb_bps * 0.30 };
            *rolling_wifi = if *rolling_wifi == 0.0 { inst_wifi_bps } else { *rolling_wifi * 0.70 + inst_wifi_bps * 0.30 };

            let rolling_mbps = *rolling / (1024.0 * 1024.0);
            if let Some(telemetry) = crate::util::telemetry::get_telemetry(transfer_id) {
                telemetry.update_peak_throughput(rolling_mbps);
            }

            *last_time = now;
            *last_bytes = bytes;
            *last_usb_bytes = usb_bytes;
            *last_wifi_bytes = wifi_bytes;
        }
    }

    let (throughput, usb_speed, wifi_speed, duration_secs) = if status == TransferStatus::Completed {
        let mut end_time = record.end_time.lock();
        let end_instant = *end_time.get_or_insert(now);
        let elapsed = end_instant.duration_since(record.start_time).as_secs_f64().max(0.05);
        let avg = (bytes as f64) / elapsed;
        let avg_mbps = avg / (1024.0 * 1024.0);
        if let Some(telemetry) = crate::util::telemetry::get_telemetry(transfer_id) {
            telemetry.update_peak_throughput(avg_mbps);
        }
        if (usb_bytes > 0 || wifi_bytes > 0) && elapsed > 0.05 {
            (
                avg,
                (usb_bytes as f64) / elapsed,
                (wifi_bytes as f64) / elapsed,
                elapsed,
            )
        } else {
            let is_usb = record.transport_name.contains("USB") || record.transport_name.contains("ADB") || record.transport_name.contains("127.0.0.1");
            let is_wifi = record.transport_name.contains("Wi-Fi") || record.transport_name.contains("Hotspot") || record.transport_name.contains("P2P") || record.transport_name.contains("10.18.") || record.transport_name.contains("192.168.");
            if is_usb && is_wifi {
                (avg, avg * 0.5, avg * 0.5, elapsed)
            } else if is_wifi {
                (avg, 0.0, avg, elapsed)
            } else {
                (avg, avg, 0.0, elapsed)
            }
        }
    } else {
        let elapsed = now.duration_since(record.start_time).as_secs_f64().max(0.0);
        // If channel counters were recorded, use them; otherwise fall back to transport name heuristics
        if *rolling_usb > 0.0 || *rolling_wifi > 0.0 {
            (*rolling, *rolling_usb, *rolling_wifi, elapsed)
        } else {
            let is_usb = record.transport_name.contains("USB") || record.transport_name.contains("ADB") || record.transport_name.contains("127.0.0.1");
            let is_wifi = record.transport_name.contains("Wi-Fi") || record.transport_name.contains("Hotspot") || record.transport_name.contains("P2P") || record.transport_name.contains("10.18.") || record.transport_name.contains("192.168.");
            if is_usb && is_wifi {
                (*rolling, *rolling * 0.5, *rolling * 0.5, elapsed)
            } else if is_wifi {
                (*rolling, 0.0, *rolling, elapsed)
            } else {
                (*rolling, *rolling, 0.0, elapsed)
            }
        }
    };

    let percent = if record.file_size > 0 {
        ((bytes as f64 / record.file_size as f64) * 100.0).min(100.0)
    } else {
        100.0
    };

    let eta = if status == TransferStatus::Completed || bytes >= record.file_size {
        Some(0)
    } else if is_terminal {
        None
    } else {
        let elapsed = now.duration_since(record.start_time).as_secs_f64();
        let overall_avg_bps = if elapsed > 0.3 { (bytes as f64) / elapsed } else { 0.0 };
        let rolling_bps = *rolling;

        // Smarter weighted speed blending: blend 2-second moving average (70%) with session average (30%)
        let effective_speed_bps = if elapsed < 2.0 {
            if rolling_bps > 0.0 && overall_avg_bps > 0.0 {
                rolling_bps.max(overall_avg_bps)
            } else if rolling_bps > 0.0 {
                rolling_bps
            } else {
                overall_avg_bps
            }
        } else {
            if rolling_bps > 0.0 && overall_avg_bps > 0.0 {
                (rolling_bps * 0.70) + (overall_avg_bps * 0.30)
            } else if rolling_bps > 0.0 {
                rolling_bps
            } else {
                overall_avg_bps
            }
        };

        if effective_speed_bps > 1024.0 && bytes < record.file_size {
            let raw_eta = ((record.file_size - bytes) as f64) / effective_speed_bps;
            let mut last_eta_guard = record.last_smoothed_eta.lock();
            let smoothed = match *last_eta_guard {
                Some(prev) => {
                    // Low-pass filter to eliminate sudden Wi-Fi retransmission spikes
                    let s = (prev * 0.40) + (raw_eta * 0.60);
                    *last_eta_guard = Some(s);
                    s
                }
                None => {
                    *last_eta_guard = Some(raw_eta);
                    raw_eta
                }
            };
            Some(smoothed.round().max(1.0) as u64)
        } else {
            None
        }
    };

    Some(TransferProgress {
        transfer_id: record.transfer_id,
        file_name: record.file_name.clone(),
        file_size: record.file_size,
        bytes_transferred: bytes,
        percent,
        usb_throughput_bps: usb_speed,
        wifi_throughput_bps: wifi_speed,
        aggregate_throughput_bps: throughput,
        eta_seconds: eta,
        duration_seconds: duration_secs,
        usb_bytes_transferred: usb_bytes,
        wifi_bytes_transferred: wifi_bytes,
        total_chunks: record.total_chunks,
        completed_chunks: chunks,
        retry_count: 0,
        usb_errors: 0,
        wifi_errors: 0,
        status,
    })
}

/// Returns current (actively transferring), resumable (incomplete), and completed transfer summaries (§7).
pub fn get_transfers() -> Vec<TransferSummary> {
    let registry = get_registry();
    let map = registry.transfers.lock();

    let mut summaries: HashMap<Uuid, TransferSummary> = HashMap::new();

    // 1. Add active in-memory transfers
    for r in map.values() {
        summaries.insert(
            r.transfer_id,
            TransferSummary {
                transfer_id: r.transfer_id,
                file_name: r.file_name.clone(),
                file_size: r.file_size,
                status: *r.status.lock(),
                role: r.role,
            },
        );
    }

    // 2. Scan disk metadata files (only .meta.json, ignore large telemetry .json logs)
    let dir = super::sender::default_data_dir();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.file_name().and_then(|n| n.to_str()).map_or(false, |n| n.ends_with(".meta.json")) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(meta) = serde_json::from_str::<crate::manifest::TransferMeta>(&content) {
                        summaries.entry(meta.transfer_id).or_insert_with(|| {
                            let status = if meta.status == TransferStatus::Completed {
                                TransferStatus::Completed
                            } else {
                                TransferStatus::Paused // Resumable
                            };

                            TransferSummary {
                                transfer_id: meta.transfer_id,
                                file_name: meta.file_name,
                                file_size: meta.file_size,
                                status,
                                role: meta.role,
                            }
                        });
                    }
                }
            }
        }
    }

    summaries.into_values().collect()
}

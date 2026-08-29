### [`core/src/transfer/api.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/core/src/transfer/api.rs)
- **1. Transfer Engine**: Public transfer coordinator (`start_transfer`, `enter_receive_mode`, `pause_transfer`, `resume_transfer`, `get_progress`).
- **6. ACK Handling**: Receiver-side immediate ACK generation with elapsed verify time.
- **8. Receiver Engine**: Multi-channel listener server and frame ingestion daemon.
- **9. Receiver Disk Writer**: Async `DiskWriteCmd` channel and background blocking disk writer thread.
- **12. Session / Configuration**: `ActiveTransferRecord`, `TransportPreference`.

---

### [`core/src/transfer/session.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/core/src/transfer/session.rs)
- **1. Transfer Engine**: Core sender/receiver session loops (`send_file_session`, `send_file_session_multipath`, `receive_file_session`).
- **6. ACK Handling**: ACK/NACK frame dispatchers (`handle_ack_frame`, `handle_multipath_ack_frame`).
- **7. Chunk Queue / Dispatch**: Reader thread, worker dispatch mpsc queues (`prepared_tx`/`prepared_rx`, `retry_tx`/`retry_rx`).
- **8. Receiver Engine**: Single-channel `receive_file_session`.
- **9. Receiver Disk Writer**: Single-channel dedicated `DiskWriteTask` background worker.

---

### [`core/src/scheduler/multipath.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/core/src/scheduler/multipath.rs)
- **2. Scheduler**: `MultipathScheduler`, candidate evaluation (`CandidateEval`), min-$E[T]$ selection, bounded fairness invariant.
- **12. Session / Configuration**: `SchedulerConfig`, dynamic feature flags (`enable_dynamic_scheduler`, `enable_dynamic_window`).

---

### [`core/src/scheduler/model.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/core/src/scheduler/model.rs)
- **2. Scheduler**: `ChannelPerformanceModel`, completion time predictor $E[T]$, EWMA throughput/variance modeling, capacity estimation, prediction error telemetry.

---

### [`core/src/scheduler/tracker.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/core/src/scheduler/tracker.rs)
- **3. Channel Manager**: `ChannelTracker`, lifecycle state machine (`Unknown`, `WarmingUp`, `Active`, `Degraded`, `Probing`), in-flight chunk/byte accounting, session utilization (`inflight_utilization_pct`), socket blocking ratio.

---

### [`core/src/scheduler/window.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/core/src/scheduler/window.rs)
- **5. In-Flight Window / AIMD Controller**: `WindowController`, AIMD additive increase / multiplicative decrease, throughput-gain gated scaling, corroborated backpressure detection, cooldowns.

---

### [`core/src/scheduler/metrics.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/core/src/scheduler/metrics.rs)
- **3. Channel Manager**: `ThroughputTracker`, `RollingWindowTracker`.
- **10. Telemetry / Metrics**: Real-time transport throughput counters.

---

### [`core/src/scheduler/buffer_pool.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/core/src/scheduler/buffer_pool.rs)
- **7. Chunk Queue / Dispatch**: `BufferPool` zero-allocation memory recycling.

---

### [`core/src/chunk/mod.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/core/src/chunk/mod.rs)
- **7. Chunk Queue / Dispatch**: `calculate_chunk_plan` (lines 41–58), `total_chunks`, `ChunkPlanEntry`, `read_chunk_at`, `read_chunk_into_slice`.

---

### [`core/src/protocol/messages.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/core/src/protocol/messages.rs)
- **6. ACK Handling**: `ChunkAckData`, `BatchChunkAckData`, `ChunkNackData` message definitions and codecs with `receiver_verify_us`.

---

### [`core/src/transport/mod.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/core/src/transport/mod.rs)
- **4. Channel / Transport**: `Transport` trait, `TransportKind`, `TransportError`.

---

### [`core/src/transport/stream.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/core/src/transport/stream.rs)
- **4. Channel / Transport**: `StreamTransport`, async TCP socket stream wrapper.

---

### [`core/src/transport/vectored.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/core/src/transport/vectored.rs)
- **4. Channel / Transport**: Zero-copy OS vectored socket I/O (`write_all_vectored`).

---

### [`transport/usb/src/lib.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/transport/usb/src/lib.rs)
- **4. Channel / Transport**: USB ADB reverse forward / bulk transport provider.

---

### [`transport/wifi_direct/src/lib.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/transport/wifi_direct/src/lib.rs)
- **4. Channel / Transport**: Wi-Fi Direct multi-stream socket provider.

---

### [`core/src/util/telemetry.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/core/src/util/telemetry.rs)
- **10. Telemetry / Metrics**: `TransferTelemetry`, monotonic sub-stage latencies ($T_{\text{queue}}, T_{\text{socket}}, T_{\text{ack\_turnaround}}, T_{\text{receiver\_verify}}, T_{\text{ack\_residual}}$), rolling 1s peak sampler (`peak_1s_mbps`), JSON and formatted session logs.
- **11. Bottleneck Detector**: `generate_bottleneck_report()`, role-aware latency breakdown, diagnostic classifier (`SOCKET_BUFFER_BACKPRESSURE`, `CHUNK_COMPLETION_JITTER`, `NETWORK_BANDWIDTH_LIMIT`, `PACKET_CORRUPTION`, `CHANNEL_DISCONNECT`).

---

### [`core/src/util/storage.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/core/src/util/storage.rs)
- **9. Receiver Disk Writer**: `preallocate_file`, sequential read prefetch advisories (`advise_sequential_read`).

---

### [`core/src/manifest/transfer_plan.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/core/src/manifest/transfer_plan.rs)
- **12. Session / Configuration**: `TransferManifest`, `TransferStatus`, `TransferRole`.


code:
d:\MyDocuments\Programming\android\Aug26\TurboTransfer\core\src\transfer\api.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use log::debug;

use super::session::{send_file_session, send_file_session_multipath, TransferSessionError};
use super::tracker::{ChunkTracker, InMemoryChunkTracker};
use crate::checksum::{compute_file_crc32c, compute_xxhash64};
use crate::manifest::{MetaActor, MetaActorHandle, TransferRole, TransferStatus};
use crate::protocol::{
    ChunkAckData, ChunkNackData, HelloData, Message, TransferAcceptData,
};
use crate::transport::{
    TcpListenerTransport, TcpTransport, Transport, UsbTransport, UsbTransportConfig,
    WifiDirectTransport,
};
use crate::util::telemetry::{
    export_and_clean_telemetry, get_or_create_telemetry, EventLevel, TransferStage,
    TransferTelemetry,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportPreference {
    Automatic,
    Combined,
    UsbOnly,
    WifiDirectOnly,
}

impl Default for TransportPreference {
    fn default() -> Self {
        Self::Automatic
    }
}

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
    pub total_chunks: u32,
    pub completed_chunks: u32,
    pub retry_count: u64,
    pub usb_errors: u64,
    pub wifi_errors: u64,
    pub status: TransferStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: Uuid,
    pub device_name: String,
    pub transport: String,
    pub is_connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferSummary {
    pub transfer_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub status: TransferStatus,
    pub role: TransferRole,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub device_id: Uuid,
    pub transport: TransportPreference,
    pub throughput_mbps: f64,
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
struct Registry {
    transfers: Mutex<HashMap<Uuid, ActiveTransferRecord>>,
}

static REGISTRY: std::sync::OnceLock<Registry> = std::sync::OnceLock::new();

fn get_registry() -> &'static Registry {
    REGISTRY.get_or_init(|| Registry {
        transfers: Mutex::new(HashMap::new()),
    })
}

/// Receive listeners are process-owned resources, not UI state.  Keeping their
/// abort handles here lets every frontend stop the exact listener it started.
struct ReceiveListener {
    abort: tokio::task::AbortHandle,
}

static RECEIVE_LISTENERS: std::sync::OnceLock<Mutex<HashMap<String, ReceiveListener>>> =
    std::sync::OnceLock::new();

fn get_receive_listeners() -> &'static Mutex<HashMap<String, ReceiveListener>> {
    RECEIVE_LISTENERS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Updates progress statistics for an active transfer.
pub fn update_transfer_progress(transfer_id: Uuid, bytes: u64, chunks_done: u32) {
    let registry = get_registry();
    let map = registry.transfers.lock().unwrap();
    if let Some(record) = map.get(&transfer_id) {
        record.bytes_transferred.store(bytes, Ordering::Relaxed);
        record.completed_chunks.store(chunks_done, Ordering::Relaxed);
    }
}

/// Records payload bytes transferred over a specific physical/virtual channel (USB vs Wi-Fi).
pub fn record_channel_bytes(transfer_id: Uuid, is_usb: bool, bytes: u64) {
    let registry = get_registry();
    let map = registry.transfers.lock().unwrap();
    if let Some(record) = map.get(&transfer_id) {
        if is_usb {
            record.usb_bytes_transferred.fetch_add(bytes, Ordering::Relaxed);
        } else {
            record.wifi_bytes_transferred.fetch_add(bytes, Ordering::Relaxed);
        }
    }
}

/// Updates the status and error state of an active transfer.
pub fn set_transfer_status(transfer_id: Uuid, status: TransferStatus, error_msg: Option<String>) {
    let registry = get_registry();
    let map = registry.transfers.lock().unwrap();
    if let Some(record) = map.get(&transfer_id) {
        *record.status.lock().unwrap() = status;
        if status == TransferStatus::Completed
            || status == TransferStatus::Failed
            || status == TransferStatus::Cancelled
        {
            let mut end_time = record.end_time.lock().unwrap();
            if end_time.is_none() {
                *end_time = Some(std::time::Instant::now());
            }
        } else if status == TransferStatus::InProgress {
            let mut end_time = record.end_time.lock().unwrap();
            *end_time = None;
        }
        if let Some(err) = error_msg {
            *record.last_error.lock().unwrap() = Some(err);
        }
    }
}

/// Reads the user-requested lifecycle state. Sender sessions check this between
/// chunks, which keeps controls responsive without pre-empting a disk write.
pub fn transfer_control_status(transfer_id: Uuid) -> Option<TransferStatus> {
    let registry = get_registry();
    let map = registry.transfers.lock().unwrap();
    map.get(&transfer_id).map(|record| *record.status.lock().unwrap())
}

/// Returns the last error recorded for a transfer, if any.
pub fn get_transfer_error(transfer_id: Uuid) -> Option<String> {
    let registry = get_registry();
    let map = registry.transfers.lock().unwrap();
    map.get(&transfer_id).and_then(|record| record.last_error.lock().unwrap().clone())
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
    let registry = get_registry();
    let mut map = registry.transfers.lock().unwrap();
    if let Some(existing) = map.get(&transfer_id) {
        if existing.role == TransferRole::Sender && role == TransferRole::Receiver {
            return;
        }
    }
    let now = std::time::Instant::now();
    map.insert(
        transfer_id,
        ActiveTransferRecord {
            transfer_id,
            file_name,
            file_size,
            bytes_transferred: Arc::new(AtomicU64::new(0)),
            usb_bytes_transferred: Arc::new(AtomicU64::new(0)),
            wifi_bytes_transferred: Arc::new(AtomicU64::new(0)),
            completed_chunks: Arc::new(AtomicU32::new(0)),
            total_chunks,
            start_time: now,
            end_time: Mutex::new(None),
            role,
            status: Arc::new(Mutex::new(TransferStatus::InProgress)),
            transport_name,
            last_error: Arc::new(Mutex::new(None)),
            actor_handle: None,
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

/// Default loopback TCP address for Milestone 5 / 6 transfers.
pub const DEFAULT_LOOPBACK_ADDR: &str = "127.0.0.1:9876";

/// Default listen address for Milestone 6 real network transfers.
pub const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0:9876";

/// Updates the transport name of an active transfer.
pub fn update_transfer_transport_name(transfer_id: Uuid, name: String) {
    let registry = get_registry();
    let mut map = registry.transfers.lock().unwrap();
    if let Some(record) = map.get_mut(&transfer_id) {
        record.transport_name = name;
    }
}

pub const DEFAULT_WIFI_PARALLEL_STREAMS: usize = 4;

/// Starts a file transfer to a peer over `TcpTransport` or `UsbTransport` (§6, §7, §8).
pub async fn start_transfer(
    file_path: PathBuf,
    custom_file_name: Option<String>,
    device_id: Option<Uuid>,
    transport_pref: TransportPreference,
    address: Option<String>,
) -> Result<TransferHandle, TransferSessionError> {
    let sender_id = Uuid::new_v4();
    let _target_device_id = device_id.unwrap_or_else(Uuid::new_v4);
    let transfer_id = Uuid::new_v4();
    let chunk_size = 2 * 1024 * 1024; // 2 MiB chunks for optimal pipeline flow and wire-speed Wi-Fi throughput

    // Stop local receive listener to ensure port 9876 is free for outgoing USB/ADB tunnel
    leave_receive_mode(None);

    let file_name = custom_file_name.clone().unwrap_or_else(|| {
        let resolved_path = std::fs::read_link(&file_path).unwrap_or_else(|_| file_path.clone());
        resolved_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string()
    });
    let file_size = match std::fs::metadata(&file_path) {
        Ok(m) => m.len(),
        Err(e) => return Err(TransferSessionError::Io(e)),
    };
    let plan = crate::chunk::calculate_chunk_plan(file_size, chunk_size);
    let total_chunks = plan.len().max(1) as u32;

    // Register active transfer in registry immediately so UI/API can track progress from initiation
    register_active_transfer(
        transfer_id,
        file_name.clone(),
        file_size,
        TransferRole::Sender,
        total_chunks,
        match transport_pref {
            TransportPreference::UsbOnly => "USB (Connecting...)".to_string(),
            TransportPreference::WifiDirectOnly => "Wi-Fi Direct (Connecting...)".to_string(),
            TransportPreference::Combined => "Multipath (Connecting...)".to_string(),
            TransportPreference::Automatic => "Connecting...".to_string(),
        },
    );

    let addr_default = DEFAULT_LOOPBACK_ADDR.to_string();
    let addr = address.as_deref().unwrap_or(&addr_default);
    let mut transports: Vec<(Box<dyn Transport>, bool)> = Vec::new();
    let mut transport_names: Vec<String> = Vec::new();

    let connect_res: Result<(), TransferSessionError> = async {
        match transport_pref {
            TransportPreference::UsbOnly => {
                if let Ok(t) = TcpTransport::connect(addr).await {
                    transports.push((Box::new(t), true));
                    transport_names.push("USB (ADB Tunnel)".to_string());
                } else {
                    let config = UsbTransportConfig::new(9876, 9876);
                    let t = UsbTransport::connect(config).await?;
                    transports.push((Box::new(t), true));
                    transport_names.push("USB (ADB Tunnel)".to_string());
                }
            }
            TransportPreference::WifiDirectOnly => {
                if let Some(ref explicit_addr) = address {
                    for stream_idx in 1..=DEFAULT_WIFI_PARALLEL_STREAMS {
                        if let Ok(transport) = TcpTransport::connect(explicit_addr).await {
                            transports.push((Box::new(transport), false));
                            transport_names.push(format!("5 GHz Wi-Fi Direct (Stream #{})", stream_idx));
                        }
                    }
                    if transports.is_empty() {
                        let transport = TcpTransport::connect(explicit_addr).await?;
                        transports.push((Box::new(transport), false));
                        transport_names.push("5 GHz Wi-Fi Direct".to_string());
                    }
                } else {
                    let config = WifiDirectTransport::discover_android_hotspot(None)
                        .await
                        .ok_or_else(|| TransferSessionError::Rejected(
                            "No Android Local-Only Hotspot was discovered over USB control channel".into(),
                        ))?;
                    let target_ip = if config.target_ip.is_empty() {
                        WifiDirectTransport::resolve_windows_default_gateway().unwrap_or_else(|_| "10.18.163.130".to_string())
                    } else {
                        config.target_ip.clone()
                    };
                    let target_addr = format!("{}:{}", target_ip, config.port);
                    for stream_idx in 1..=DEFAULT_WIFI_PARALLEL_STREAMS {
                        if let Ok(t) = TcpTransport::connect(&target_addr).await {
                            transports.push((Box::new(t), false));
                            transport_names.push(format!("5 GHz Local-Only Hotspot (Stream #{})", stream_idx));
                        }
                    }
                    if transports.is_empty() {
                        let transport = WifiDirectTransport::connect(config).await?;
                        transports.push((Box::new(transport), false));
                        transport_names.push("5 GHz Local-Only Hotspot".to_string());
                    }
                }
            }
            TransportPreference::Combined => {
                if let Some(ref explicit_addr) = address {
                    if explicit_addr.contains(',') {
                        for single_addr in explicit_addr.split(',') {
                            let trimmed = single_addr.trim();
                            if !trimmed.is_empty() {
                                let is_usb = trimmed.contains("127.0.0.1") || trimmed.contains("localhost") || trimmed.contains("usb");
                                if is_usb {
                                    if let Ok(t) = TcpTransport::connect(trimmed).await {
                                        transports.push((Box::new(t), true));
                                        transport_names.push("USB (ADB Tunnel)".to_string());
                                    }
                                } else {
                                    for stream_idx in 1..=DEFAULT_WIFI_PARALLEL_STREAMS {
                                        if let Ok(t) = TcpTransport::connect(trimmed).await {
                                            transports.push((Box::new(t), false));
                                            transport_names.push(format!("5 GHz Wi-Fi Direct (Stream #{})", stream_idx));
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        let is_usb = explicit_addr.contains("127.0.0.1") || explicit_addr.contains("localhost") || explicit_addr.contains("usb");
                        if is_usb {
                            if let Ok(t) = TcpTransport::connect(explicit_addr).await {
                                transports.push((Box::new(t), true));
                                transport_names.push("USB (ADB Tunnel)".to_string());
                            }
                        } else {
                            for stream_idx in 1..=DEFAULT_WIFI_PARALLEL_STREAMS {
                                if let Ok(t) = TcpTransport::connect(explicit_addr).await {
                                    transports.push((Box::new(t), false));
                                    transport_names.push(format!("5 GHz Wi-Fi Direct (Stream #{})", stream_idx));
                                }
                            }
                        }
                    }
                }

                if transports.is_empty() {
                    // 1. Connect USB channel
                    if let Ok(t) = TcpTransport::connect(DEFAULT_LOOPBACK_ADDR).await {
                        transports.push((Box::new(t), true));
                        transport_names.push("USB (ADB Tunnel)".to_string());
                    } else {
                        let usb_config = UsbTransportConfig::new(9876, 9876);
                        if let Ok(t) = UsbTransport::connect(usb_config).await {
                            transports.push((Box::new(t), true));
                            transport_names.push("USB (ADB Tunnel)".to_string());
                        }
                    }
                    // 2. Connect Wi-Fi Direct channel with bonded sockets
                    for hotspot_ip in &[
                        "10.18.163.1:9876",
                        "10.18.163.2:9876",
                        "10.18.163.130:9876",
                        "10.78.112.40:9876",
                        "192.168.43.1:9876",
                        "192.168.43.2:9876",
                        "192.168.137.1:9876",
                        "192.168.1.19:9876",
                    ] {
                        let mut connected_any = false;
                        for stream_idx in 1..=DEFAULT_WIFI_PARALLEL_STREAMS {
                            if let Ok(t) = tokio::time::timeout(tokio::time::Duration::from_millis(500), TcpTransport::connect(hotspot_ip)).await {
                                if let Ok(transport) = t {
                                    transports.push((Box::new(transport), false));
                                    transport_names.push(format!("5 GHz Wi-Fi Direct (Stream #{})", stream_idx));
                                    connected_any = true;
                                }
                            }
                        }
                        if connected_any {
                            break;
                        }
                    }
                }

                if transports.is_empty() {
                    return Err(TransferSessionError::Transport(crate::transport::TransportError::Disconnected(
                        "Failed to connect over either USB or Wi-Fi Direct".into(),
                    )));
                }
            }
            TransportPreference::Automatic => {
                if let Some(ref explicit_addr) = address {
                    if explicit_addr.contains(',') {
                        for single_addr in explicit_addr.split(',') {
                            let trimmed = single_addr.trim();
                            if !trimmed.is_empty() {
                                let is_usb = trimmed.contains("127.0.0.1") || trimmed.contains("localhost") || trimmed.contains("usb");
                                if is_usb {
                                    if let Ok(Ok(t)) = tokio::time::timeout(tokio::time::Duration::from_millis(800), TcpTransport::connect(trimmed)).await {
                                        transports.push((Box::new(t), true));
                                        transport_names.push("USB (ADB Tunnel)".to_string());
                                    }
                                } else {
                                    for stream_idx in 1..=DEFAULT_WIFI_PARALLEL_STREAMS {
                                        if let Ok(Ok(t)) = tokio::time::timeout(tokio::time::Duration::from_millis(800), TcpTransport::connect(trimmed)).await {
                                            transports.push((Box::new(t), false));
                                            transport_names.push(format!("5 GHz Wi-Fi Direct (Stream #{})", stream_idx));
                                        }
                                    }
                                }
                            }
                        }
                    } else {
                        let is_usb = explicit_addr.contains("127.0.0.1") || explicit_addr.contains("localhost") || explicit_addr.contains("usb");
                        if is_usb {
                            if let Ok(Ok(t)) = tokio::time::timeout(tokio::time::Duration::from_millis(800), TcpTransport::connect(explicit_addr)).await {
                                transports.push((Box::new(t), true));
                                transport_names.push("USB (ADB Tunnel)".to_string());
                            }
                        } else {
                            for stream_idx in 1..=DEFAULT_WIFI_PARALLEL_STREAMS {
                                if let Ok(Ok(t)) = tokio::time::timeout(tokio::time::Duration::from_millis(800), TcpTransport::connect(explicit_addr)).await {
                                    transports.push((Box::new(t), false));
                                    transport_names.push(format!("5 GHz Wi-Fi Direct (Stream #{})", stream_idx));
                                }
                            }
                        }
                    }
                }

                if transports.is_empty() {
                    #[cfg(target_os = "android")]
                    {
                        // Probe USB reverse tunnel
                        if let Ok(t) = tokio::time::timeout(tokio::time::Duration::from_millis(800), TcpTransport::connect(addr)).await {
                            if let Ok(transport) = t {
                                transports.push((Box::new(transport), true));
                                transport_names.push("USB ADB Reverse Tunnel".to_string());
                            }
                        }
                        // Probe Wi-Fi Direct / Hotspot gateway and ARP peers
                        let mut probe_ips: Vec<String> = vec![
                            "10.18.163.1:9876".to_string(),
                            "10.18.163.2:9876".to_string(),
                            "10.18.163.130:9876".to_string(),
                            "10.78.112.40:9876".to_string(),
                            "192.168.43.1:9876".to_string(),
                            "192.168.43.2:9876".to_string(),
                            "192.168.137.1:9876".to_string(),
                            "192.168.1.19:9876".to_string(),
                        ];
                        if let Ok(arp_content) = std::fs::read_to_string("/proc/net/arp") {
                            for line in arp_content.lines().skip(1) {
                                let parts: Vec<&str> = line.split_whitespace().collect();
                                if let Some(ip) = parts.first() {
                                    if !ip.is_empty() && ip.contains('.') {
                                        let addr = format!("{}:9876", ip);
                                        if !probe_ips.contains(&addr) {
                                            probe_ips.push(addr);
                                        }
                                    }
                                }
                            }
                        }

                        for hotspot_ip in &probe_ips {
                            let mut connected_any = false;
                            for stream_idx in 1..=DEFAULT_WIFI_PARALLEL_STREAMS {
                                if let Ok(t) = tokio::time::timeout(tokio::time::Duration::from_millis(500), TcpTransport::connect(hotspot_ip)).await {
                                    if let Ok(transport) = t {
                                        transports.push((Box::new(transport), false));
                                        transport_names.push(format!("5 GHz Wi-Fi Direct (Stream #{})", stream_idx));
                                        connected_any = true;
                                    }
                                }
                            }
                            if connected_any {
                                break;
                            }
                        }
                    }

                    #[cfg(not(target_os = "android"))]
                    {
                        let usb_config = UsbTransportConfig::new(9876, 9876);
                        if let Ok(t) = UsbTransport::connect(usb_config).await {
                            transports.push((Box::new(t), true));
                            transport_names.push("USB (ADB Tunnel)".to_string());
                        } else if let Ok(t) = TcpTransport::connect(addr).await {
                            transports.push((Box::new(t), true));
                            transport_names.push("USB Tunnel".to_string());
                        }

                        // If USB is already connected or probe hotspot, connect Wi-Fi with bonded streams
                        for hotspot_ip in &[
                            "10.18.163.1:9876",
                            "10.18.163.2:9876",
                            "10.18.163.130:9876",
                            "10.78.112.40:9876",
                            "192.168.43.1:9876",
                            "192.168.43.2:9876",
                            "192.168.137.1:9876",
                        ] {
                            let mut connected_any = false;
                            for stream_idx in 1..=DEFAULT_WIFI_PARALLEL_STREAMS {
                                if let Ok(t) = tokio::time::timeout(tokio::time::Duration::from_millis(400), TcpTransport::connect(hotspot_ip)).await {
                                    if let Ok(transport) = t {
                                        transports.push((Box::new(transport), false));
                                        transport_names.push(format!("5 GHz Wi-Fi Direct (Stream #{})", stream_idx));
                                        connected_any = true;
                                    }
                                }
                            }
                            if connected_any {
                                break;
                            }
                        }
                    }
                }

                if transports.is_empty() {
                    // addr may be comma-separated (e.g. "127.0.0.1:9876,10.18.163.1:9876");
                    // try each component individually instead of passing raw to connect
                    let fallback_addrs: Vec<&str> = addr.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
                    for fallback_addr in &fallback_addrs {
                        if let Ok(Ok(t)) = tokio::time::timeout(
                            tokio::time::Duration::from_millis(800),
                            TcpTransport::connect(fallback_addr),
                        ).await {
                            let is_usb = fallback_addr.contains("127.0.0.1") || fallback_addr.contains("localhost");
                            transports.push((Box::new(t), is_usb));
                            transport_names.push("TCP Transport".to_string());
                            break;
                        }
                    }
                }
            }
        }
        Ok(())
    }.await;

    if let Err(e) = connect_res {
        log::error!("start_transfer connection failure: {}", e);
        if let Some(telemetry) = crate::util::telemetry::get_telemetry(transfer_id) {
            telemetry.mark_failed(&e.to_string());
            let data_dir = default_data_dir();
            export_and_clean_telemetry(transfer_id, &data_dir);
        }
        set_transfer_status(transfer_id, TransferStatus::Failed, Some(e.to_string()));
        return Err(e);
    }

    let transport_name = if transport_names.len() > 1 {
        format!("{} (Multipath Active)", transport_names.join(" + "))
    } else {
        transport_names.into_iter().next().unwrap_or_else(|| "TCP Transport".to_string())
    };
    log::info!("start_transfer connected successfully via {}", transport_name);

    update_transfer_transport_name(transfer_id, transport_name);

    tokio::spawn(async move {
        let res = send_file_session_multipath(
            sender_id,
            "TurboSender",
            &file_path,
            chunk_size,
            transfer_id,
            transports,
            custom_file_name.as_deref(),
        )
        .await;

        match res {
            Ok(()) => {
                set_transfer_status(transfer_id, TransferStatus::Completed, None);
            }
            Err(TransferSessionError::Paused | TransferSessionError::Cancelled) => {
                // The public control operation already recorded the terminal
                // state. Do not overwrite it with a transport failure.
            }
            Err(e) => {
                set_transfer_status(transfer_id, TransferStatus::Failed, Some(e.to_string()));
            }
        }
    });

    Ok(TransferHandle { transfer_id })
}

/// Enters receive mode, listening on a real network interface (e.g. `0.0.0.0:9876`) or loopback via `TcpListenerTransport`.
pub async fn enter_receive_mode(
    address: Option<String>,
    dest_dir: PathBuf,
) -> Result<tokio::task::JoinHandle<Result<PathBuf, TransferSessionError>>, TransferSessionError> {
    let addr = address.as_deref().unwrap_or(DEFAULT_LISTEN_ADDR);
    let listener = TcpListenerTransport::bind(addr).await?;

    let bound_addr = listener.local_addr()?.to_string();
    {
        let listeners = get_receive_listeners().lock().unwrap();
        if listeners.contains_key(&bound_addr) {
            return Err(TransferSessionError::Rejected(format!(
                "Receive mode is already active on {}", bound_addr
            )));
        }
    }

    // ADB and Windows WLAN association are desktop responsibilities. Android
    // already has the local listener and must never try to execute `adb`.
    #[cfg(not(target_os = "android"))]
    {
        if address.is_none() || address.as_deref() == Some(DEFAULT_LISTEN_ADDR) {
            tokio::spawn(async {
                if let Ok(devices) = UsbTransport::list_adb_devices() {
                    for dev in devices {
                        if dev.state == "device" {
                            let _ = UsbTransport::setup_default_adb_tunnels(&dev.serial);
                            let _ = UsbTransport::trigger_android_hotspot(&dev.serial);
                        }
                    }
                }

                if let Some(config) = WifiDirectTransport::discover_android_hotspot(None).await {
                    #[cfg(target_os = "windows")]
                    {
                        let _ = WifiDirectTransport::associate_wlan_windows(&config).await;
                    }
                }
            });
        }
    }

    let bound_addr_cleanup = bound_addr.clone();
    let (completion_tx, mut completion_rx) = tokio::sync::mpsc::unbounded_channel();

    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                accept_res = listener.accept() => {
                    match accept_res {
                        Ok((transport, peer_addr)) => {
                            let is_usb = peer_addr.ip().is_loopback();
                            let tx = completion_tx.clone();
                            let ddir = dest_dir.clone();
                            tokio::spawn(async move {
                                if let Err(e) =
                                    handle_incoming_receive_transport(Box::new(transport), is_usb, ddir, tx).await
                                {
                                    debug!("Incoming receive transport closed/error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            get_receive_listeners().lock().unwrap().remove(&bound_addr_cleanup);
                            return Err(TransferSessionError::Transport(e));
                        }
                    }
                }
                Some(path) = completion_rx.recv() => {
                    log::info!("Incoming file transfer completed and persisted: {:?}", path);
                    // Do NOT terminate receiver loop or remove listener; keep listening for subsequent incoming files
                }
            }
        }
    });

    get_receive_listeners().lock().unwrap().insert(
        bound_addr,
        ReceiveListener {
            abort: handle.abort_handle(),
        },
    );

    Ok(handle)
}

#[allow(dead_code)]
enum DiskWriteCmd {
    Write {
        chunk_id: u32,
        file_offset: u64,
        payload: Vec<u8>,
        queue_depth: u32,
    },
    Flush(tokio::sync::oneshot::Sender<std::io::Result<()>>),
    Close(tokio::sync::oneshot::Sender<std::io::Result<()>>),
}

struct ActiveReceiveSession {
    pub file_path: PathBuf,
    pub part_path: PathBuf,
    pub disk_tx: tokio::sync::mpsc::Sender<DiskWriteCmd>,
    pub tracker: Arc<parking_lot::Mutex<InMemoryChunkTracker>>,
    pub chunk_crcs: Arc<parking_lot::Mutex<HashMap<u32, (u32, usize)>>>,
    pub total_chunks: u32,
    pub bytes_recv_total: Arc<AtomicU64>,
    pub completed_chunks_count: Arc<AtomicU32>,
    pub is_completed: Arc<std::sync::atomic::AtomicBool>,
    pub is_sender_in_same_process: bool,
    pub telemetry: Arc<TransferTelemetry>,
}

static ACTIVE_RECEIVE_SESSIONS: std::sync::OnceLock<Mutex<HashMap<Uuid, Arc<ActiveReceiveSession>>>> =
    std::sync::OnceLock::new();

fn get_active_receive_sessions() -> &'static Mutex<HashMap<Uuid, Arc<ActiveReceiveSession>>> {
    ACTIVE_RECEIVE_SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn handle_incoming_receive_transport(
    mut transport: Box<dyn Transport>,
    is_usb: bool,
    dest_dir: PathBuf,
    completion_tx: tokio::sync::mpsc::UnboundedSender<PathBuf>,
) -> Result<(), TransferSessionError> {
    // 1. Handshake: Await Hello
    let _hello = match transport.receive_frame().await? {
        Some(Message::Hello(h)) => h,
        Some(other) => {
            return Err(TransferSessionError::UnexpectedMessage(format!(
                "Expected Hello, got {:?}",
                other
            )))
        }
        None => return Ok(()), // EOF / probe
    };

    // Reply Hello
    let receiver_hello = Message::Hello(HelloData {
        device_id: Uuid::new_v4(),
        device_name: "TurboReceiver".to_string(),
        protocol_version: 1,
    });
    transport.send_frame(&receiver_hello).await?;

    // 2. Await TransferOffer
    let offer = match transport.receive_frame().await? {
        Some(Message::TransferOffer(o)) => o,
        Some(other) => {
            return Err(TransferSessionError::UnexpectedMessage(format!(
                "Expected TransferOffer, got {:?}",
                other
            )))
        }
        None => return Ok(()),
    };

    let telemetry = get_or_create_telemetry(
        offer.transfer_id,
        &offer.file_name,
        offer.file_size,
        TransferRole::Receiver,
    );

    telemetry.record_event(
        TransferStage::Handshake,
        EventLevel::Info,
        "Receiver",
        None,
        None,
        Some(offer.file_size),
        format!("Received and accepted TransferOffer for '{}' ({} bytes, {} chunks)", offer.file_name, offer.file_size, offer.total_chunks),
        None,
    );

    // 3. Get or create ActiveReceiveSession
    let session = {
        let mut map = get_active_receive_sessions().lock().unwrap();
        if let Some(existing) = map.get(&offer.transfer_id) {
            existing.clone()
        } else {
            std::fs::create_dir_all(&dest_dir)?;
            let part_path = dest_dir.join(format!("{}.part", offer.file_name));
            let final_path = dest_dir.join(&offer.file_name);
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(&part_path)?;

            let t_pre = std::time::Instant::now();
            crate::util::storage::preallocate_file(&file, offer.file_size)?;
            let pre_us = t_pre.elapsed().as_micros() as u64;

            telemetry.record_event(
                TransferStage::DiskWrite,
                EventLevel::Debug,
                "ReceiverDisk",
                None,
                Some(pre_us),
                Some(offer.file_size),
                format!("Preallocated {} bytes in {} us", offer.file_size, pre_us),
                None,
            );

            register_active_transfer(
                offer.transfer_id,
                offer.file_name.clone(),
                offer.file_size,
                TransferRole::Receiver,
                offer.total_chunks,
                "Multi-Channel Ingestion".to_string(),
            );

            let is_sender_in_same_process = {
                let reg = get_registry();
                let reg_map = reg.transfers.lock().unwrap();
                reg_map.get(&offer.transfer_id).map_or(false, |r| r.role == TransferRole::Sender)
            };

            let (disk_tx, mut disk_rx) = tokio::sync::mpsc::channel::<DiskWriteCmd>(128);
            let mut writer_file = file;
            let tel_for_disk = telemetry.clone();

            tokio::task::spawn_blocking(move || {
                use std::io::{Seek, SeekFrom, Write};
                while let Some(cmd) = disk_rx.blocking_recv() {
                    match cmd {
                        DiskWriteCmd::Write { chunk_id, file_offset, payload, queue_depth } => {
                            let t_w0 = std::time::Instant::now();
                            let len = payload.len() as u64;
                            if let Err(e) = writer_file.seek(SeekFrom::Start(file_offset)).and_then(|_| writer_file.write_all(&payload)) {
                                log::error!("Background disk write error: {}", e);
                            }
                            let write_us = t_w0.elapsed().as_micros() as u64;
                            tel_for_disk.record_disk_write(chunk_id, len, write_us, queue_depth);
                        }
                        DiskWriteCmd::Flush(reply_tx) => {
                            let res = writer_file.flush();
                            let _ = reply_tx.send(res);
                        }
                        DiskWriteCmd::Close(reply_tx) => {
                            let res = writer_file.flush();
                            drop(writer_file);
                            let _ = reply_tx.send(res);
                            break;
                        }
                    }
                }
            });

            let new_session = Arc::new(ActiveReceiveSession {
                file_path: final_path,
                part_path,
                disk_tx,
                tracker: Arc::new(parking_lot::Mutex::new(InMemoryChunkTracker::new())),
                chunk_crcs: Arc::new(parking_lot::Mutex::new(HashMap::new())),
                total_chunks: offer.total_chunks,
                bytes_recv_total: Arc::new(AtomicU64::new(0)),
                completed_chunks_count: Arc::new(AtomicU32::new(0)),
                is_completed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                is_sender_in_same_process,
                telemetry: telemetry.clone(),
            });
            map.insert(offer.transfer_id, new_session.clone());
            new_session
        }
    };

    // 4. Send TransferAccept
    let accept = Message::TransferAccept(TransferAcceptData {
        transfer_id: offer.transfer_id,
        resume_from: None,
    });
    transport.send_frame(&accept).await?;

    // 5. Receive chunks
    loop {
        let frame = match transport.receive_frame().await? {
            Some(f) => f,
            None => break,
        };

        match frame {
            Message::ChunkData(chunk_data) => {
                let ch_name = if is_usb { "USB" } else { "Wi-Fi" };
                let t_v0 = std::time::Instant::now();
                let computed_checksum = compute_xxhash64(&chunk_data.payload);
                let verify_us = t_v0.elapsed().as_micros() as u64;

                if computed_checksum != chunk_data.checksum {
                    session.telemetry.record_event(
                        TransferStage::Checksum,
                        EventLevel::Warn,
                        ch_name,
                        Some(chunk_data.chunk_id),
                        Some(verify_us),
                        Some(chunk_data.payload.len() as u64),
                        format!("Checksum mismatch on chunk #{}", chunk_data.chunk_id),
                        None,
                    );
                    let nack = Message::ChunkNack(ChunkNackData {
                        transfer_id: chunk_data.transfer_id,
                        chunk_id: chunk_data.chunk_id,
                        reason: "xxHash64 payload mismatch".to_string(),
                    });
                    transport.send_frame(&nack).await?;
                    continue;
                }

                session.telemetry.record_chunk_recv(
                    ch_name,
                    chunk_data.chunk_id,
                    chunk_data.payload_length as u64,
                    0,
                    verify_us,
                );

                let is_duplicate = {
                    let tracker = session.tracker.lock();
                    tracker.is_chunk_completed(
                        chunk_data.transfer_id,
                        chunk_data.file_id,
                        chunk_data.chunk_id,
                        chunk_data.checksum,
                    )
                };

                if is_duplicate {
                    session.telemetry.record_duplicate_chunk(chunk_data.chunk_id);
                    // Fix B2: Ensure chunk_crcs is populated even on duplicates so total_chunks match at Complete
                    {
                        let mut crc_map = session.chunk_crcs.lock();
                        if !crc_map.contains_key(&chunk_data.chunk_id) {
                            let chunk_crc = crate::checksum::compute_crc32c(&chunk_data.payload);
                            crc_map.insert(chunk_data.chunk_id, (chunk_crc, chunk_data.payload.len()));
                        }
                    }
                    let ack = Message::ChunkAck(ChunkAckData {
                        transfer_id: chunk_data.transfer_id,
                        chunk_id: chunk_data.chunk_id,
                        receiver_verify_us: Some(verify_us as u32),
                    });
                    transport.send_frame(&ack).await?;
                    continue;
                }

                let chunk_crc = crate::checksum::compute_crc32c(&chunk_data.payload);
                let payload_len = chunk_data.payload.len();
                {
                    let mut tracker = session.tracker.lock();
                    tracker.mark_chunk_completed(
                        chunk_data.transfer_id,
                        chunk_data.file_id,
                        chunk_data.chunk_id,
                        chunk_data.checksum,
                    );
                    session.chunk_crcs.lock().insert(chunk_data.chunk_id, (chunk_crc, payload_len));
                }

                let total_b = session
                    .bytes_recv_total
                    .fetch_add(chunk_data.payload_length as u64, Ordering::Relaxed)
                    + chunk_data.payload_length as u64;
                let total_c = session.completed_chunks_count.fetch_add(1, Ordering::Relaxed) + 1;

                if !session.is_sender_in_same_process {
                    update_transfer_progress(chunk_data.transfer_id, total_b, total_c);
                    record_channel_bytes(chunk_data.transfer_id, is_usb, chunk_data.payload_length as u64);
                }

                // Send immediate ChunkAck for 100% universal sender compatibility
                let ack = Message::ChunkAck(ChunkAckData {
                    transfer_id: chunk_data.transfer_id,
                    chunk_id: chunk_data.chunk_id,
                    receiver_verify_us: Some(verify_us as u32),
                });
                transport.send_frame(&ack).await?;

                // Queue disk write asynchronously
                let q_depth = (128 - session.disk_tx.capacity()) as u32;
                let _ = session.disk_tx.send(DiskWriteCmd::Write {
                    chunk_id: chunk_data.chunk_id,
                    file_offset: chunk_data.file_offset,
                    payload: chunk_data.payload,
                    queue_depth: q_depth,
                }).await;
            }
            Message::Complete(complete_data) => {
                let was_completed = session.is_completed.swap(true, Ordering::SeqCst);
                if !was_completed {
                    let t_fin0 = std::time::Instant::now();
                    // Close background disk writer and flush before checking file checksum
                    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                    let _ = session.disk_tx.send(DiskWriteCmd::Close(reply_tx)).await;
                    if let Ok(res) = reply_rx.await {
                        res?;
                    }

                    // In-Flight O(1) Checksum calculation via GF(2) matrix CRC32C combination
                    let file_crc = {
                        let crc_map = session.chunk_crcs.lock();
                        if crc_map.len() == session.total_chunks as usize {
                            let mut acc = crate::checksum::Crc32cAccumulator::new();
                            for cid in 0..session.total_chunks {
                                if let Some(&(crc, len)) = crc_map.get(&cid) {
                                    acc.combine(crc, len);
                                }
                            }
                            acc.finalize()
                        } else {
                            // Fallback to disk read only if chunks were missing in memory table (e.g. cold restart)
                            compute_file_crc32c(&session.part_path)?
                        }
                    };

                    if file_crc != complete_data.file_checksum {
                        session.telemetry.mark_failed(&format!(
                            "CRC32C mismatch: expected 0x{:08X}, got 0x{:08X}",
                            complete_data.file_checksum, file_crc
                        ));
                        let data_dir = default_data_dir();
                        export_and_clean_telemetry(complete_data.transfer_id, &data_dir);
                        set_transfer_status(
                            complete_data.transfer_id,
                            TransferStatus::Failed,
                            Some("CRC32C mismatch".to_string()),
                        );
                        return Err(TransferSessionError::ChecksumMismatch(format!(
                            "File CRC32C mismatch: expected 0x{:08X}, got 0x{:08X}",
                            complete_data.file_checksum, file_crc
                        )));
                    }

                    std::fs::rename(&session.part_path, &session.file_path)?;
                    set_transfer_status(complete_data.transfer_id, TransferStatus::Completed, None);

                    let fin_ms = t_fin0.elapsed().as_millis() as u64;
                    session.telemetry.record_finalize(fin_ms, true);
                    session.telemetry.mark_completed();
                    let data_dir = default_data_dir();
                    export_and_clean_telemetry(complete_data.transfer_id, &data_dir);

                    let _ = completion_tx.send(session.file_path.clone());
                    get_active_receive_sessions()
                        .lock()
                        .unwrap()
                        .remove(&complete_data.transfer_id);
                }

                // Send final completion ACK
                let ack = Message::ChunkAck(ChunkAckData {
                    transfer_id: complete_data.transfer_id,
                    chunk_id: u32::MAX,
                    receiver_verify_us: None,
                });
                transport.send_frame(&ack).await?;
                break;
            }
            Message::Pause(pause_data) => {
                session.telemetry.record_event(TransferStage::Control, EventLevel::Info, "Receiver", None, None, None, "Receiver received Pause", None);
                set_transfer_status(pause_data.transfer_id, TransferStatus::Paused, None);
                let ack = Message::ChunkAck(ChunkAckData {
                    transfer_id: pause_data.transfer_id,
                    chunk_id: u32::MAX - 1,
                    receiver_verify_us: None,
                });
                transport.send_frame(&ack).await?;
            }
            Message::Resume(resume_data) => {
                session.telemetry.record_event(TransferStage::Control, EventLevel::Info, "Receiver", None, None, None, "Receiver received Resume", None);
                set_transfer_status(resume_data.transfer_id, TransferStatus::InProgress, None);
                let ack = Message::ChunkAck(ChunkAckData {
                    transfer_id: resume_data.transfer_id,
                    chunk_id: u32::MAX - 2,
                    receiver_verify_us: None,
                });
                transport.send_frame(&ack).await?;
            }
            Message::Cancel(cancel_data) => {
                session.telemetry.record_event(TransferStage::Control, EventLevel::Info, "Receiver", None, None, None, "Receiver received Cancel", None);
                session.telemetry.mark_failed("Transfer cancelled by peer");
                let data_dir = default_data_dir();
                export_and_clean_telemetry(cancel_data.transfer_id, &data_dir);
                set_transfer_status(cancel_data.transfer_id, TransferStatus::Cancelled, None);
                let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                let _ = session.disk_tx.send(DiskWriteCmd::Close(reply_tx)).await;
                let _ = reply_rx.await;
                let _ = std::fs::remove_file(&session.part_path);
                get_active_receive_sessions()
                    .lock()
                    .unwrap()
                    .remove(&cancel_data.transfer_id);
                break;
            }
            Message::Heartbeat(hb) => {
                let reply = Message::Heartbeat(crate::protocol::HeartbeatData {
                    sequence: hb.sequence + 1,
                });
                transport.send_frame(&reply).await?;
            }
            _ => {}
        }
    }

    // Fix B4: If transfer failed or cancelled, clean up the active receive session to release disk writer and file handle
    if let Some(status) = transfer_control_status(offer.transfer_id) {
        if status == TransferStatus::Failed || status == TransferStatus::Cancelled {
            let session_opt = {
                let mut map = get_active_receive_sessions().lock().unwrap();
                map.remove(&offer.transfer_id)
            };
            if let Some(s) = session_opt {
                let (reply_tx, _) = tokio::sync::oneshot::channel();
                let _ = s.disk_tx.send(DiskWriteCmd::Close(reply_tx)).await;
            }
        }
    }

    Ok(())
}

/// Stops a named listener, or every listener when no address is supplied.
pub fn leave_receive_mode(address: Option<&str>) -> bool {
    let mut listeners = get_receive_listeners().lock().unwrap();
    let had_listeners;
    if let Some(addr) = address {
        if let Some(listener) = listeners.remove(addr) {
            listener.abort.abort();
            had_listeners = true;
        } else {
            had_listeners = false;
        }
    } else {
        had_listeners = !listeners.is_empty();
        for (_, listener) in listeners.drain() {
            listener.abort.abort();
        }
    }

    // Clean up ADB tunnels to prevent server deadlock from stale forward/reverse rules
    #[cfg(not(target_os = "android"))]
    if had_listeners {
        UsbTransport::cleanup_all_default_adb_tunnels(None);
    }

    had_listeners
}

/// Pauses an active transfer (§7).
pub fn pause_transfer(transfer_id: Uuid) {
    set_transfer_status(transfer_id, TransferStatus::Paused, None);
    let registry = get_registry();
    let map = registry.transfers.lock().unwrap();
    if let Some(record) = map.get(&transfer_id) {
        if let Some(actor) = &record.actor_handle {
            let actor = actor.clone();
            tokio::spawn(async move {
                actor.pause().await;
            });
        }
    }
}

static CUSTOM_DATA_DIR: parking_lot::RwLock<Option<PathBuf>> = parking_lot::RwLock::new(None);

pub fn set_custom_data_dir(path: PathBuf) {
    let _ = std::fs::create_dir_all(&path);
    *CUSTOM_DATA_DIR.write() = Some(path);
}

/// Returns the default metadata storage directory (§12).
pub fn default_data_dir() -> PathBuf {
    if let Some(ref custom) = *CUSTOM_DATA_DIR.read() {
        let _ = std::fs::create_dir_all(custom);
        return custom.clone();
    }
    if let Ok(dir) = std::env::var("TURBOTRANSFER_DATA_DIR") {
        let p = PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&p);
        return p;
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = PathBuf::from(appdata).join("turbotransfer");
        let _ = std::fs::create_dir_all(&p);
        p
    } else {
        #[cfg(target_os = "android")]
        {
            let candidates = [
                PathBuf::from("/storage/emulated/0/Download/TurboTransfer"),
                PathBuf::from("/sdcard/Download/TurboTransfer"),
                PathBuf::from("/data/data/com.turbotransfer/files"),
                PathBuf::from("/data/user/0/com.turbotransfer/files"),
            ];
            for c in &candidates {
                if std::fs::create_dir_all(c).is_ok() {
                    return c.clone();
                }
            }
        }
        let p = std::env::temp_dir().join("turbotransfer");
        let _ = std::fs::create_dir_all(&p);
        p
    }
}

/// Finds a resumable transfer metadata on disk by ID or returns the most recent incomplete one (§7, §14).
pub fn find_resumable_transfer(target_id: Option<Uuid>) -> Option<(PathBuf, crate::manifest::TransferMeta)> {
    let dir = default_data_dir();
    let entries = std::fs::read_dir(dir).ok()?;

    let mut candidate: Option<(PathBuf, crate::manifest::TransferMeta)> = None;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json")
            || path.file_name().and_then(|n| n.to_str()).map_or(false, |n| n.ends_with(".meta.json"))
        {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(meta) = serde_json::from_str::<crate::manifest::TransferMeta>(&content) {
                    if let Some(tid) = target_id {
                        if meta.transfer_id == tid {
                            return Some((path, meta));
                        }
                    } else if meta.status != TransferStatus::Completed {
                        // Pick most recent incomplete transfer
                        candidate = Some((path, meta));
                    }
                }
            }
        }
    }

    candidate
}

/// Resumes a paused or interrupted transfer from its persisted `meta.json` (§7, §14).
pub async fn resume_transfer(
    transfer_id: Option<Uuid>,
    transport_pref: TransportPreference,
    address: Option<String>,
) -> Result<TransferHandle, TransferSessionError> {
    let (meta_path, meta) = find_resumable_transfer(transfer_id)
        .ok_or_else(|| TransferSessionError::Rejected("No resumable transfer found".into()))?;

    let tid = meta.transfer_id;
    let file_name = meta.file_name.clone();
    let file_size = meta.file_size;
    let role = meta.role;
    let total_chunks = meta.total_chunks;

    // Start MetaActor loading from existing meta_path
    let (_actor_handle, _join) = MetaActor::spawn(meta_path, meta.clone(), 100);

    // Register active transfer
    register_active_transfer(
        tid,
        file_name,
        file_size,
        role,
        total_chunks,
        "Resumed Transfer".to_string(),
    );

    let addr = address.as_deref().unwrap_or(DEFAULT_LOOPBACK_ADDR);
    let transport: Box<dyn Transport> = match transport_pref {
        TransportPreference::UsbOnly => {
            let config = UsbTransportConfig::new(9876, 9876);
            match UsbTransport::connect(config).await {
                Ok(t) => Box::new(t),
                Err(_) => Box::new(TcpTransport::connect(addr).await?),
            }
        }
        TransportPreference::WifiDirectOnly => {
            let config = WifiDirectTransport::discover_android_hotspot(None)
                .await
                .ok_or_else(|| TransferSessionError::Rejected(
                    "No Android Local-Only Hotspot was discovered over USB control channel".into(),
                ))?;
            Box::new(WifiDirectTransport::connect(config).await?)
        }
        TransportPreference::Combined => return Err(TransferSessionError::Rejected(
            "Combined USB + Wi-Fi scheduling is not implemented yet".into(),
        )),
        TransportPreference::Automatic => Box::new(TcpTransport::connect(addr).await?),
    };

    let sender_id = Uuid::new_v4();
    let file_path = PathBuf::from(&meta.file_name);
    let custom_name = meta.file_name.clone();
    let chunk_size = meta.chunk_size;

    tokio::spawn(async move {
        let _ = send_file_session(
            sender_id,
            "TurboSender",
            &file_path,
            chunk_size,
            tid,
            transport,
            Some(&custom_name),
            None,
        )
        .await;
    });

    Ok(TransferHandle { transfer_id: tid })
}

/// Cancels an active transfer (§7).
pub fn cancel_transfer(transfer_id: Uuid) {
    set_transfer_status(transfer_id, TransferStatus::Cancelled, None);
    if let Some(s) = get_active_receive_sessions().lock().unwrap().remove(&transfer_id) {
        let (reply_tx, _) = tokio::sync::oneshot::channel();
        let _ = s.disk_tx.try_send(DiskWriteCmd::Close(reply_tx));
    }
    let registry = get_registry();
    let map = registry.transfers.lock().unwrap();
    if let Some(record) = map.get(&transfer_id) {
        if let Some(actor) = &record.actor_handle {
            let actor = actor.clone();
            tokio::spawn(async move {
                actor.cancel().await;
            });
        }
    }
}

/// Retrieves the progress of a transfer (§7).
pub fn get_progress(transfer_id: Uuid) -> Option<TransferProgress> {
    let registry = get_registry();
    let map = registry.transfers.lock().unwrap();
    let record = map.get(&transfer_id)?;

    let bytes = record.bytes_transferred.load(Ordering::Relaxed);
    let usb_bytes = record.usb_bytes_transferred.load(Ordering::Relaxed);
    let wifi_bytes = record.wifi_bytes_transferred.load(Ordering::Relaxed);
    let chunks = record.completed_chunks.load(Ordering::Relaxed);
    let status = *record.status.lock().unwrap();
    let now = std::time::Instant::now();

    let mut last_time = record.last_sample_time.lock().unwrap();
    let mut last_bytes = record.last_sample_bytes.lock().unwrap();
    let mut last_usb_bytes = record.last_sample_usb_bytes.lock().unwrap();
    let mut last_wifi_bytes = record.last_sample_wifi_bytes.lock().unwrap();
    let mut rolling = record.rolling_throughput_bps.lock().unwrap();
    let mut rolling_usb = record.rolling_usb_throughput_bps.lock().unwrap();
    let mut rolling_wifi = record.rolling_wifi_throughput_bps.lock().unwrap();

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

    let (throughput, usb_speed, wifi_speed) = if status == TransferStatus::Completed {
        let mut end_time = record.end_time.lock().unwrap();
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
            )
        } else {
            let is_usb = record.transport_name.contains("USB") || record.transport_name.contains("ADB") || record.transport_name.contains("127.0.0.1");
            let is_wifi = record.transport_name.contains("Wi-Fi") || record.transport_name.contains("Hotspot") || record.transport_name.contains("P2P") || record.transport_name.contains("10.18.") || record.transport_name.contains("192.168.");
            if is_usb && is_wifi {
                (avg, avg * 0.5, avg * 0.5)
            } else if is_wifi {
                (avg, 0.0, avg)
            } else {
                (avg, avg, 0.0)
            }
        }
    } else {
        // If channel counters were recorded, use them; otherwise fall back to transport name heuristics
        if *rolling_usb > 0.0 || *rolling_wifi > 0.0 {
            (*rolling, *rolling_usb, *rolling_wifi)
        } else {
            let is_usb = record.transport_name.contains("USB") || record.transport_name.contains("ADB") || record.transport_name.contains("127.0.0.1");
            let is_wifi = record.transport_name.contains("Wi-Fi") || record.transport_name.contains("Hotspot") || record.transport_name.contains("P2P") || record.transport_name.contains("10.18.") || record.transport_name.contains("192.168.");
            if is_usb && is_wifi {
                (*rolling, *rolling * 0.5, *rolling * 0.5)
            } else if is_wifi {
                (*rolling, 0.0, *rolling)
            } else {
                (*rolling, *rolling, 0.0)
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
            let mut last_eta_guard = record.last_smoothed_eta.lock().unwrap();
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
        total_chunks: record.total_chunks,
        completed_chunks: chunks,
        retry_count: 0,
        usb_errors: 0,
        wifi_errors: 0,
        status,
    })
}

/// Discovered devices list across available network and transport interfaces (§7, §8, §9).
/// Strictly adheres to the LocalSend model: Only peers actively listening in Receive Mode are displayed.
pub fn get_devices() -> Vec<DeviceInfo> {
    let mut devices = Vec::new();
    let mut seen_ips = std::collections::HashSet::new();

    // 1. Enumerate connected ADB devices
    if let Ok(adb_devs) = UsbTransport::list_adb_devices() {
        let is_pc_receiving = !get_receive_listeners().lock().unwrap().is_empty();
        for d in adb_devs {
            if d.state == "device" {
                let is_listening = if is_pc_receiving {
                    let _ = UsbTransport::setup_receive_adb_tunnels(&d.serial);
                    true
                } else {
                    let _ = UsbTransport::setup_adb_forward(&d.serial, 9876, 9876);
                    if UsbTransport::is_receiver_listening(&d.serial, 9876) {
                        true
                    } else {
                        let _ = UsbTransport::trigger_android_receive(&d.serial);
                        std::thread::sleep(std::time::Duration::from_millis(80));
                        UsbTransport::is_receiver_listening(&d.serial, 9876)
                    }
                };

                let name = if let Some(model) = &d.model {
                    format!("Android Phone: {} ({})", model, d.serial)
                } else if let Some(prod) = &d.product {
                    format!("Android Device: {} ({})", prod, d.serial)
                } else {
                    format!("Android ADB Device ({})", d.serial)
                };

                let transport_desc = if is_listening {
                    "USB (Ready to Receive)".to_string()
                } else {
                    "USB (ADB Connected)".to_string()
                };

                devices.push(DeviceInfo {
                    device_id: Uuid::from_u128(crate::checksum::compute_xxhash64(d.serial.as_bytes()) as u128),
                    device_name: name,
                    transport: transport_desc,
                    is_connected: true,
                });
                seen_ips.insert("127.0.0.1".to_string());
            }
        }
    }

    // 2. Probe 5 GHz Hotspot / Wi-Fi Direct Gateway endpoints
    for &wifi_ip in &["10.18.163.130", "192.168.43.1", "192.168.43.2", "192.168.1.19"] {
        if seen_ips.contains(wifi_ip) {
            continue;
        }
        if let Ok(stream) = std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from((
                wifi_ip.parse::<std::net::Ipv4Addr>().unwrap_or(std::net::Ipv4Addr::UNSPECIFIED),
                9876,
            )),
            std::time::Duration::from_millis(60),
        ) {
            drop(stream);
            seen_ips.insert(wifi_ip.to_string());
            devices.push(DeviceInfo {
                device_id: Uuid::from_u128(crate::checksum::compute_xxhash64(wifi_ip.as_bytes()) as u128),
                device_name: format!("Android Phone (5 GHz Wi-Fi Hotspot: {})", wifi_ip),
                transport: "5 GHz Wi-Fi Direct".to_string(),
                is_connected: true,
            });
        }
    }

    // 3. Fallback LAN / Wi-Fi Active Receivers Probe
    if !seen_ips.contains("127.0.0.1") {
        if let Ok(stream) = std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], 9876)),
            std::time::Duration::from_millis(50),
        ) {
            drop(stream);
            if devices.is_empty() {
                devices.push(DeviceInfo {
                    device_id: Uuid::nil(),
                    device_name: "Active Network Receiver (Port 9876)".to_string(),
                    transport: "TCP Network".to_string(),
                    is_connected: true,
                });
            }
        }
    }

    devices
}

/// Returns current (actively transferring), resumable (incomplete), and completed transfer summaries (§7).
pub fn get_transfers() -> Vec<TransferSummary> {
    let registry = get_registry();
    let map = registry.transfers.lock().unwrap();

    let mut summaries: HashMap<Uuid, TransferSummary> = HashMap::new();

    // 1. Add active in-memory transfers
    for r in map.values() {
        summaries.insert(
            r.transfer_id,
            TransferSummary {
                transfer_id: r.transfer_id,
                file_name: r.file_name.clone(),
                file_size: r.file_size,
                status: *r.status.lock().unwrap(),
                role: r.role,
            },
        );
    }

    // 2. Scan disk metadata files (only .meta.json, ignore large telemetry .json logs)
    let dir = default_data_dir();
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

/// Executes an isolated transport throughput benchmark per TRD §7 and §8.
pub async fn run_benchmark(
    device_id: Option<Uuid>,
    transport_pref: TransportPreference,
    _payload_size_mb: u32,
) -> Result<BenchmarkResult, TransferSessionError> {
    let _ = device_id;
    let _ = transport_pref;
    Err(TransferSessionError::Rejected(
        "Live benchmarks are not implemented yet; refusing to report synthetic throughput".into(),
    ))
}


d:\MyDocuments\Programming\android\Aug26\TurboTransfer\core\src\transfer\session.rs

use std::fs::{rename, OpenOptions};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use uuid::Uuid;

use super::api::{
    default_data_dir, register_active_transfer, set_transfer_status, transfer_control_status,
    update_transfer_progress,
};
use super::tracker::ChunkTracker;
use crate::checksum::{compute_file_crc32c, compute_xxhash64};
use crate::chunk::{calculate_chunk_plan, read_chunk_at};
use crate::manifest::{generate_manifest_with_name, TransferRole, TransferStatus};
use crate::protocol::{
    encode_frame, ChunkAckData, ChunkDataPayload, ChunkNackData, CompleteData,
    HelloData, Message, ProtocolError, TransferAcceptData, TransferOfferData,
};
use crate::transport::{StreamTransport, Transport, TransportError, TransportKind};
use crate::util::telemetry::{
    export_and_clean_telemetry, get_or_create_telemetry, EventLevel, TransferStage,
    TransferTelemetry,
};

#[derive(Error, Debug)]
pub enum TransferSessionError {
    #[error("Transfer rejected by peer: {0}")]
    Rejected(String),

    #[error("Checksum mismatch: {0}")]
    ChecksumMismatch(String),

    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("Transport error: {0}")]
    Transport(#[from] TransportError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Unexpected message: {0:?}")]
    UnexpectedMessage(String),

    #[error("Transfer paused by user")]
    Paused,

    #[error("Transfer cancelled by user")]
    Cancelled,
}

/// Helper to write a framed message directly to an `AsyncWrite` stream.
pub async fn send_msg<W: AsyncWrite + Unpin>(
    writer: &mut W,
    msg: &Message,
) -> Result<(), TransferSessionError> {
    let frame = encode_frame(msg)?;
    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(())
}

/// Prepared chunk payload with pre-computed checksum, ready for immediate network transmission.
struct PreparedChunk {
    entry: crate::chunk::ChunkPlanEntry,
    payload: Vec<u8>,
    checksum: u64,
}

fn handle_ack_frame(
    frame: Message,
    is_usb: bool,
    in_flight: &mut std::collections::HashSet<u32>,
    in_flight_times: &mut std::collections::HashMap<u32, std::time::Instant>,
    completed_set: &mut std::collections::HashSet<u32>,
    plan_map: &std::collections::HashMap<u32, crate::chunk::ChunkPlanEntry>,
    retry_tx: &std::sync::mpsc::Sender<crate::chunk::ChunkPlanEntry>,
    transfer_id: Uuid,
    bytes_sent_total: &mut u64,
    completed_chunks_count: &mut u32,
    telemetry: Option<&TransferTelemetry>,
    channel_name: &str,
) -> Result<(), TransferSessionError> {
    match frame {
        Message::ChunkAck(ack) => {
            in_flight.remove(&ack.chunk_id);
            let bytes_len = plan_map.get(&ack.chunk_id).map_or(0, |e| e.payload_length as u64);
            if let Some(t_disp) = in_flight_times.remove(&ack.chunk_id) {
                let rtt_ms = t_disp.elapsed().as_secs_f64() * 1000.0;
                if let Some(tel) = telemetry {
                    tel.record_chunk_ack(channel_name, ack.chunk_id, rtt_ms, bytes_len);
                }
            }
            if completed_set.insert(ack.chunk_id) {
                if let Some(entry) = plan_map.get(&ack.chunk_id) {
                    *bytes_sent_total += entry.payload_length as u64;
                    *completed_chunks_count += 1;
                    update_transfer_progress(
                        transfer_id,
                        *bytes_sent_total,
                        *completed_chunks_count,
                    );
                    crate::transfer::api::record_channel_bytes(transfer_id, is_usb, entry.payload_length as u64);
                }
            }
        }
        Message::BatchChunkAck(batch) => {
            for cid in batch.chunk_ids {
                in_flight.remove(&cid);
                let bytes_len = plan_map.get(&cid).map_or(0, |e| e.payload_length as u64);
                if let Some(t_disp) = in_flight_times.remove(&cid) {
                    let rtt_ms = t_disp.elapsed().as_secs_f64() * 1000.0;
                    if let Some(tel) = telemetry {
                        tel.record_chunk_ack(channel_name, cid, rtt_ms, bytes_len);
                    }
                }
                if completed_set.insert(cid) {
                    if let Some(entry) = plan_map.get(&cid) {
                        *bytes_sent_total += entry.payload_length as u64;
                        *completed_chunks_count += 1;
                        update_transfer_progress(
                            transfer_id,
                            *bytes_sent_total,
                            *completed_chunks_count,
                        );
                        crate::transfer::api::record_channel_bytes(transfer_id, is_usb, entry.payload_length as u64);
                    }
                }
            }
        }
        Message::ChunkNack(nack) => {
            in_flight.remove(&nack.chunk_id);
            in_flight_times.remove(&nack.chunk_id);
            if let Some(tel) = telemetry {
                tel.record_chunk_nack(channel_name, nack.chunk_id, &nack.reason);
            }
            if let Some(entry) = plan_map.get(&nack.chunk_id) {
                let _ = retry_tx.send(entry.clone());
            }
        }
        other => {
            return Err(TransferSessionError::UnexpectedMessage(format!(
                "Expected ChunkAck/BatchChunkAck/ChunkNack, got {:?}",
                other
            )));
        }
    }
    Ok(())
}


/// Runs the sender side of a transfer session over any generic `Transport` (§6, §7, §8, §9).
/// Implements a high-throughput sliding window pipeline with concurrent in-flight chunks.
pub async fn send_file_session<T>(
    sender_device_id: Uuid,
    sender_device_name: &str,
    file_path: &Path,
    chunk_size: u32,
    transfer_id: Uuid,
    mut transport: T,
    custom_file_name: Option<&str>,
    is_usb_channel: Option<bool>,
) -> Result<(), TransferSessionError>
where
    T: Transport,
{
    let manifest = generate_manifest_with_name(file_path, chunk_size, custom_file_name)?;
    let telemetry = get_or_create_telemetry(transfer_id, &manifest.file_name, manifest.file_size, TransferRole::Sender);
    let is_usb = is_usb_channel.unwrap_or_else(|| transport.kind() == crate::transport::TransportKind::Usb);
    let ch_name = if is_usb { "USB" } else { "Wi-Fi" };

    telemetry.record_event(
        TransferStage::Handshake,
        EventLevel::Info,
        ch_name,
        None,
        None,
        None,
        format!("Sending Hello handshake to peer (sender: {})", sender_device_name),
        None,
    );

    // 1. Handshake: Send Hello
    let hello = Message::Hello(HelloData {
        device_id: sender_device_id,
        device_name: sender_device_name.to_string(),
        protocol_version: 1,
    });
    transport.send_frame(&hello).await?;

    // 2. Handshake: Await Receiver Hello
    let peer_hello = transport
        .receive_frame()
        .await?
        .ok_or(TransferSessionError::UnexpectedMessage(
            "EOF during Hello".into(),
        ))?;
    if !matches!(peer_hello, Message::Hello(_)) {
        return Err(TransferSessionError::UnexpectedMessage(format!(
            "Expected Hello, got {:?}",
            peer_hello
        )));
    }

    telemetry.record_event(
        TransferStage::Handshake,
        EventLevel::Info,
        ch_name,
        None,
        None,
        None,
        format!("Received Hello from peer: {:?}", peer_hello),
        None,
    );

    // 3. Send TransferOffer
    let offer = Message::TransferOffer(TransferOfferData {
        transfer_id,
        file_id: manifest.file_id,
        file_name: manifest.file_name.clone(),
        file_size: manifest.file_size,
        chunk_size: manifest.chunk_size,
        total_chunks: manifest.total_chunks,
        checksum_algo: "xxhash64".to_string(),
    });
    transport.send_frame(&offer).await?;

    telemetry.record_event(
        TransferStage::Handshake,
        EventLevel::Info,
        ch_name,
        None,
        None,
        Some(manifest.file_size),
        format!("Sent TransferOffer: '{}' ({} bytes, {} chunks of {} bytes)", manifest.file_name, manifest.file_size, manifest.total_chunks, manifest.chunk_size),
        None,
    );

    // 4. Await TransferAccept / TransferReject
    let response = transport
        .receive_frame()
        .await?
        .ok_or(TransferSessionError::UnexpectedMessage(
            "EOF during Offer response".into(),
        ))?;

    let resume_ranges = match response {
        Message::TransferAccept(accept) => {
            telemetry.record_event(
                TransferStage::Handshake,
                EventLevel::Info,
                ch_name,
                None,
                None,
                None,
                format!("Received TransferAccept (resume ranges: {:?})", accept.resume_from),
                None,
            );
            accept.resume_from
        }
        Message::TransferReject(reject) => {
            telemetry.record_event(
                TransferStage::Handshake,
                EventLevel::Error,
                ch_name,
                None,
                None,
                None,
                format!("Transfer rejected by peer: {}", reject.reason),
                None,
            );
            return Err(TransferSessionError::Rejected(reject.reason))
        }
        other => {
            return Err(TransferSessionError::UnexpectedMessage(format!(
                "Expected Accept or Reject, got {:?}",
                other
            )));
        }
    };

    // 5. Data Plane: High-Throughput Streaming Engine (Up to 64 in-flight chunks)
    const PIPELINE_DEPTH: usize = 64;
    let plan = calculate_chunk_plan(manifest.file_size, manifest.chunk_size);
    let mut plan_map: std::collections::HashMap<u32, crate::chunk::ChunkPlanEntry> =
        std::collections::HashMap::new();
    let mut bytes_sent_total = 0u64;
    let mut completed_chunks_count = 0u32;

    let mut chunks_to_send = std::collections::VecDeque::new();
    let mut completed_set: std::collections::HashSet<u32> = std::collections::HashSet::new();

    for entry in plan {
        let cid = entry.chunk_id;
        plan_map.insert(cid, entry.clone());
        // Skip chunk if inside completed ranges
        if let Some(ref ranges) = resume_ranges {
            let skip = ranges
                .iter()
                .any(|&(start, end)| cid >= start && cid <= end);
            if skip {
                bytes_sent_total += entry.payload_length as u64;
                completed_chunks_count += 1;
                completed_set.insert(cid);
                update_transfer_progress(transfer_id, bytes_sent_total, completed_chunks_count);
                continue;
            }
        }
        chunks_to_send.push_back(entry);
    }

    let mut in_flight: std::collections::HashSet<u32> = std::collections::HashSet::new();
    let mut in_flight_times: std::collections::HashMap<u32, std::time::Instant> = std::collections::HashMap::new();
    let total_chunks_needed = plan_map.len();
    let (running_crc_tx, running_crc_rx) = tokio::sync::oneshot::channel::<u32>();
    let total_plan_chunks = plan_map.len();

    if completed_set.len() < total_chunks_needed {
        let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel::<PreparedChunk>(8);
        let (retry_tx, retry_rx) = std::sync::mpsc::channel::<crate::chunk::ChunkPlanEntry>();
        let (recycle_tx, recycle_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let is_cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let reader_file_path = file_path.to_path_buf();
        let reader_cancelled = std::sync::Arc::clone(&is_cancelled);
        let chunk_size_bytes = manifest.chunk_size as usize;
        let mut pending_reader_chunks = chunks_to_send;
        let resume_ranges_cloned = resume_ranges.clone();
        let plan_map_for_reader = plan_map.clone();
        let tel_reader = telemetry.clone();

        let reader_handle = tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
            let mut file = crate::util::storage::open_sequential_read(&reader_file_path)?;
            let mut buffer_pool: Vec<Vec<u8>> = Vec::with_capacity(16);
            let mut chunk_crc_map: std::collections::HashMap<u32, (u32, usize)> = std::collections::HashMap::new();

            // Pre-calculate CRC for skipped chunks so sender has all total_plan_chunks in chunk_crc_map without re-reading whole file
            if let Some(ref ranges) = resume_ranges_cloned {
                for &(start, end) in ranges {
                    for cid in start..=end {
                        if let Some(entry) = plan_map_for_reader.get(&cid) {
                            if !chunk_crc_map.contains_key(&cid) {
                                let mut buf = vec![0u8; entry.payload_length as usize];
                                if crate::chunk::read_chunk_into_slice(&mut file, entry.file_offset, &mut buf).is_ok() {
                                    let chunk_crc = crate::checksum::compute_crc32c(&buf);
                                    chunk_crc_map.insert(cid, (chunk_crc, buf.len()));
                                }
                            }
                        }
                    }
                }
            }

            loop {
                if reader_cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }

                while let Ok(buf) = recycle_rx.try_recv() {
                    buffer_pool.push(buf);
                }

                let next_entry = if let Ok(entry) = retry_rx.try_recv() {
                    Some(entry)
                } else if let Some(entry) = pending_reader_chunks.pop_front() {
                    Some(entry)
                } else {
                    match retry_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                        Ok(entry) => Some(entry),
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                };

                let entry = match next_entry {
                    Some(e) => e,
                    None => break,
                };

                let mut buf = buffer_pool.pop().unwrap_or_else(|| Vec::with_capacity(chunk_size_bytes));
                buf.resize(entry.payload_length as usize, 0);

                use std::io::Seek;
                let t_r0 = std::time::Instant::now();
                file.seek(std::io::SeekFrom::Start(entry.file_offset))?;
                crate::chunk::read_chunk_into_slice(&mut file, entry.file_offset, &mut buf)?;
                let read_us = t_r0.elapsed().as_micros() as u64;

                let t_h0 = std::time::Instant::now();
                let chunk_crc = crate::checksum::compute_crc32c(&buf);
                chunk_crc_map.insert(entry.chunk_id, (chunk_crc, buf.len()));
                let checksum = compute_xxhash64(&buf);
                let hash_us = t_h0.elapsed().as_micros() as u64;

                tel_reader.record_chunk_read(entry.chunk_id, entry.payload_length as u64, read_us, hash_us);

                if chunk_tx.blocking_send(PreparedChunk { entry, payload: buf, checksum }).is_err() {
                    break;
                }
            }

            // In-flight O(1) finalization: combine CRC32Cs of all chunks in order if read completely
            if chunk_crc_map.len() == total_plan_chunks {
                let mut acc = crate::checksum::Crc32cAccumulator::new();
                for cid in 0..total_plan_chunks as u32 {
                    if let Some(&(crc, len)) = chunk_crc_map.get(&cid) {
                        acc.combine(crc, len);
                    }
                }
                let _ = running_crc_tx.send(acc.finalize());
            } else if let Ok(crc) = crate::checksum::compute_file_crc32c(&reader_file_path) {
                let _ = running_crc_tx.send(crc);
            }

            Ok(())
        });

        while completed_set.len() < total_chunks_needed {
            match transfer_control_status(transfer_id) {
                Some(TransferStatus::Paused) => {
                    is_cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                    telemetry.record_event(TransferStage::Control, EventLevel::Info, ch_name, None, None, None, "Transfer paused by user", None);
                    return Err(TransferSessionError::Paused);
                }
                Some(TransferStatus::Cancelled) => {
                    is_cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                    telemetry.record_event(TransferStage::Control, EventLevel::Info, ch_name, None, None, None, "Transfer cancelled by user", None);
                    return Err(TransferSessionError::Cancelled);
                }
                _ => {}
            }

            if in_flight.len() < PIPELINE_DEPTH {
                tokio::select! {
                    biased;
                    frame_res = transport.receive_frame() => {
                        match frame_res {
                            Ok(Some(frame)) => {
                                handle_ack_frame(
                                    frame,
                                    is_usb,
                                    &mut in_flight,
                                    &mut in_flight_times,
                                    &mut completed_set,
                                    &plan_map,
                                    &retry_tx,
                                    transfer_id,
                                    &mut bytes_sent_total,
                                    &mut completed_chunks_count,
                                    Some(&telemetry),
                                    ch_name,
                                )?;
                            }
                            Ok(None) => {
                                telemetry.record_channel_disconnect(ch_name, "EOF while waiting for ChunkAck");
                                return Err(TransferSessionError::UnexpectedMessage("EOF while waiting for ChunkAck".into()));
                            }
                            Err(e) => {
                                telemetry.record_channel_disconnect(ch_name, &e.to_string());
                                return Err(TransferSessionError::Transport(e));
                            }
                        }
                    }
                    prepared_opt = chunk_rx.recv() => {
                        if let Some(prepared) = prepared_opt {
                            let chunk_id = prepared.entry.chunk_id;
                            let file_offset = prepared.entry.file_offset;
                            let payload_len = prepared.entry.payload_length;
                            let file_id = manifest.file_id;
                            let checksum = prepared.checksum;

                            let chunk_msg = Message::ChunkData(ChunkDataPayload {
                                transfer_id,
                                file_id,
                                chunk_id,
                                file_offset,
                                payload_length: payload_len,
                                checksum,
                                payload: prepared.payload,
                            });

                            let t_s0 = std::time::Instant::now();
                            transport.send_frame(&chunk_msg).await?;
                            let send_us = t_s0.elapsed().as_micros() as u64;
                            telemetry.record_chunk_sent(ch_name, chunk_id, payload_len as u64, send_us);
                            in_flight_times.insert(chunk_id, std::time::Instant::now());

                            if let Message::ChunkData(d) = chunk_msg {
                                let _ = recycle_tx.send(d.payload);
                            }
                            in_flight.insert(chunk_id);
                        }
                    }
                }
            } else {
                // Pipeline full -> await ACK from receiver to free slot
                let frame = transport.receive_frame().await?.ok_or_else(|| {
                    telemetry.record_channel_disconnect(ch_name, "EOF while waiting for ChunkAck with full pipeline");
                    TransferSessionError::UnexpectedMessage("EOF while waiting for ChunkAck".into())
                })?;
                handle_ack_frame(
                    frame,
                    is_usb,
                    &mut in_flight,
                    &mut in_flight_times,
                    &mut completed_set,
                    &plan_map,
                    &retry_tx,
                    transfer_id,
                    &mut bytes_sent_total,
                    &mut completed_chunks_count,
                    Some(&telemetry),
                    ch_name,
                )?;
            }
        }

        is_cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        drop(retry_tx);
        drop(chunk_rx);
        drop(recycle_tx);
        let _ = reader_handle.await;
    }

    // 6. Complete transfer
    let t_fin0 = std::time::Instant::now();
    let file_checksum = match running_crc_rx.await {
        Ok(c) => c,
        Err(_) => compute_file_crc32c(file_path)?,
    };
    let complete_msg = Message::Complete(CompleteData {
        transfer_id,
        file_checksum,
    });
    transport.send_frame(&complete_msg).await?;

    // Await final completion ACK (drain any in-flight batch acks first)
    loop {
        let final_frame = transport
            .receive_frame()
            .await?
            .ok_or_else(|| TransferSessionError::UnexpectedMessage(
                "EOF waiting for completion ACK".into(),
            ))?;
        match final_frame {
            Message::ChunkAck(ack) if ack.chunk_id == u32::MAX => {
                for entry in plan_map.values() {
                    if completed_set.insert(entry.chunk_id) {
                        bytes_sent_total += entry.payload_length as u64;
                        completed_chunks_count += 1;
                        crate::transfer::api::record_channel_bytes(transfer_id, is_usb, entry.payload_length as u64);
                    }
                }
                update_transfer_progress(transfer_id, bytes_sent_total, completed_chunks_count);
                break;
            }
            Message::ChunkAck(ack) => {
                in_flight.remove(&ack.chunk_id);
                if completed_set.insert(ack.chunk_id) {
                    if let Some(entry) = plan_map.get(&ack.chunk_id) {
                        bytes_sent_total += entry.payload_length as u64;
                        completed_chunks_count += 1;
                        update_transfer_progress(transfer_id, bytes_sent_total, completed_chunks_count);
                        crate::transfer::api::record_channel_bytes(transfer_id, is_usb, entry.payload_length as u64);
                    }
                }
            }
            Message::BatchChunkAck(batch) => {
                for cid in batch.chunk_ids {
                    in_flight.remove(&cid);
                    if completed_set.insert(cid) {
                        if let Some(entry) = plan_map.get(&cid) {
                            bytes_sent_total += entry.payload_length as u64;
                            completed_chunks_count += 1;
                            update_transfer_progress(transfer_id, bytes_sent_total, completed_chunks_count);
                            crate::transfer::api::record_channel_bytes(transfer_id, is_usb, entry.payload_length as u64);
                        }
                    }
                }
            }
            Message::ChunkNack(nack) => {
                in_flight.remove(&nack.chunk_id);
                if let Some(entry) = plan_map.get(&nack.chunk_id) {
                    let payload = read_chunk_at(file_path, entry.file_offset, entry.payload_length)?;
                    let checksum = compute_xxhash64(&payload);
                    let chunk_msg = Message::ChunkData(ChunkDataPayload {
                        transfer_id,
                        file_id: manifest.file_id,
                        chunk_id: entry.chunk_id,
                        file_offset: entry.file_offset,
                        payload_length: entry.payload_length,
                        checksum,
                        payload: payload.to_vec(),
                    });
                    transport.send_frame(&chunk_msg).await?;
                }
            }
            other => {
                return Err(TransferSessionError::UnexpectedMessage(format!(
                    "Expected final Ack, got {:?}",
                    other
                )));
            }
        }
    }

    let fin_ms = t_fin0.elapsed().as_millis() as u64;
    telemetry.record_finalize(fin_ms, true);
    telemetry.mark_completed();
    let data_dir = default_data_dir();
    export_and_clean_telemetry(transfer_id, &data_dir);

    Ok(())
}

async fn handle_multipath_ack_frame(
    frame: Message,
    is_usb: bool,
    worker_in_flight: &mut std::collections::HashSet<u32>,
    worker_in_flight_times: &mut std::collections::HashMap<u32, std::time::Instant>,
    completed: &std::sync::Arc<parking_lot::Mutex<std::collections::HashSet<u32>>>,
    completed_count: &std::sync::Arc<std::sync::atomic::AtomicUsize>,
    plan_map: &std::collections::HashMap<u32, crate::chunk::ChunkPlanEntry>,
    retry_tx: &std::sync::mpsc::Sender<crate::chunk::ChunkPlanEntry>,
    transfer_id: Uuid,
    bytes_sent: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    chunks_done: &std::sync::Arc<std::sync::atomic::AtomicU32>,
    telemetry: Option<&std::sync::Arc<TransferTelemetry>>,
    channel_name: &str,
) -> Result<(), TransferSessionError> {
    match frame {
        Message::ChunkAck(ack) => {
            worker_in_flight.remove(&ack.chunk_id);
            let bytes_len = plan_map.get(&ack.chunk_id).map_or(0, |e| e.payload_length as u64);
            if let Some(t_disp) = worker_in_flight_times.remove(&ack.chunk_id) {
                let rtt_ms = t_disp.elapsed().as_secs_f64() * 1000.0;
                if let Some(tel) = telemetry {
                    tel.record_chunk_ack(channel_name, ack.chunk_id, rtt_ms, bytes_len);
                }
            }
            let is_new = completed.lock().insert(ack.chunk_id);
            if is_new {
                completed_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if let Some(entry) = plan_map.get(&ack.chunk_id) {
                    let total_b = bytes_sent.fetch_add(entry.payload_length as u64, std::sync::atomic::Ordering::Relaxed) + entry.payload_length as u64;
                    let total_c = chunks_done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    update_transfer_progress(transfer_id, total_b, total_c);
                    crate::transfer::api::record_channel_bytes(transfer_id, is_usb, entry.payload_length as u64);
                }
            }
        }
        Message::BatchChunkAck(batch) => {
            for cid in batch.chunk_ids {
                worker_in_flight.remove(&cid);
                let bytes_len = plan_map.get(&cid).map_or(0, |e| e.payload_length as u64);
                if let Some(t_disp) = worker_in_flight_times.remove(&cid) {
                    let rtt_ms = t_disp.elapsed().as_secs_f64() * 1000.0;
                    if let Some(tel) = telemetry {
                        tel.record_chunk_ack(channel_name, cid, rtt_ms, bytes_len);
                    }
                }
                let is_new = completed.lock().insert(cid);
                if is_new {
                    completed_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if let Some(entry) = plan_map.get(&cid) {
                        let total_b = bytes_sent.fetch_add(entry.payload_length as u64, std::sync::atomic::Ordering::Relaxed) + entry.payload_length as u64;
                        let total_c = chunks_done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                        update_transfer_progress(transfer_id, total_b, total_c);
                        crate::transfer::api::record_channel_bytes(transfer_id, is_usb, entry.payload_length as u64);
                    }
                }
            }
        }
        Message::ChunkNack(nack) => {
            worker_in_flight.remove(&nack.chunk_id);
            worker_in_flight_times.remove(&nack.chunk_id);
            if let Some(tel) = telemetry {
                tel.record_chunk_nack(channel_name, nack.chunk_id, &nack.reason);
            }
            if let Some(entry) = plan_map.get(&nack.chunk_id) {
                let _ = retry_tx.send(entry.clone());
            }
        }
        _ => {}
    }
    Ok(())
}

/// Runs a multipath sender transfer session over multiple generic `Transport` channels (§10).
/// Chunks are dynamically dispatched across all active channels to aggregate physical bandwidth.
pub async fn send_file_session_multipath(
    sender_device_id: Uuid,
    sender_device_name: &str,
    file_path: &Path,
    chunk_size: u32,
    transfer_id: Uuid,
    mut transports: Vec<(Box<dyn Transport>, bool)>,
    custom_file_name: Option<&str>,
) -> Result<(), TransferSessionError> {
    if transports.is_empty() {
        return Err(TransferSessionError::Transport(TransportError::Disconnected(
            "No active transports provided for multipath transfer".into(),
        )));
    }
    if transports.len() == 1 {
        let (transport, is_usb) = transports.pop().unwrap();
        return send_file_session(
            sender_device_id,
            sender_device_name,
            file_path,
            chunk_size,
            transfer_id,
            transport,
            custom_file_name,
            Some(is_usb),
        )
        .await;
    }

    let manifest = generate_manifest_with_name(file_path, chunk_size, custom_file_name)?;
    let telemetry = get_or_create_telemetry(transfer_id, &manifest.file_name, manifest.file_size, TransferRole::Sender);
    let plan = calculate_chunk_plan(manifest.file_size, manifest.chunk_size);
    let plan_map: std::collections::HashMap<u32, crate::chunk::ChunkPlanEntry> =
        plan.iter().map(|e| (e.chunk_id, e.clone())).collect();

    telemetry.record_event(
        TransferStage::Handshake,
        EventLevel::Info,
        "Multipath",
        None,
        None,
        Some(manifest.file_size),
        format!("Initiating multipath sender session with {} channels for '{}' ({} bytes)", transports.len(), manifest.file_name, manifest.file_size),
        None,
    );

    // 1. Perform Hello and TransferOffer handshakes across all transports
    // Fault-tolerant: if one stream fails handshake, skip it instead of aborting the entire transfer.
    let mut resume_ranges_combined: Vec<(u32, u32)> = Vec::new();
    let mut failed_indices: Vec<usize> = Vec::new();
    for (idx, (transport, is_usb)) in transports.iter_mut().enumerate() {
        let ch_name = if *is_usb { "USB" } else { "Wi-Fi" };

        let handshake_result: Result<Option<Vec<(u32, u32)>>, TransferSessionError> = async {
            let hello = Message::Hello(HelloData {
                device_id: sender_device_id,
                device_name: sender_device_name.to_string(),
                protocol_version: 1,
            });
            transport.send_frame(&hello).await?;

            let peer_hello = transport
                .receive_frame()
                .await?
                .ok_or_else(|| TransferSessionError::UnexpectedMessage("EOF during Hello on multipath transport".into()))?;
            if !matches!(peer_hello, Message::Hello(_)) {
                return Err(TransferSessionError::UnexpectedMessage(format!(
                    "Expected Hello, got {:?}",
                    peer_hello
                )));
            }

            let offer = Message::TransferOffer(TransferOfferData {
                transfer_id,
                file_id: manifest.file_id,
                file_name: manifest.file_name.clone(),
                file_size: manifest.file_size,
                chunk_size: manifest.chunk_size,
                total_chunks: manifest.total_chunks,
                checksum_algo: "xxhash64".to_string(),
            });
            transport.send_frame(&offer).await?;

            let response = transport
                .receive_frame()
                .await?
                .ok_or_else(|| TransferSessionError::UnexpectedMessage("EOF during Offer response on multipath transport".into()))?;

            match response {
                Message::TransferAccept(accept) => Ok(accept.resume_from),
                Message::TransferReject(reject) => Err(TransferSessionError::Rejected(reject.reason)),
                other => Err(TransferSessionError::UnexpectedMessage(format!(
                    "Expected Accept or Reject, got {:?}",
                    other
                ))),
            }
        }.await;

        match handshake_result {
            Ok(resume_from) => {
                telemetry.record_event(
                    TransferStage::Handshake,
                    EventLevel::Info,
                    &format!("Channel-{}", idx + 1),
                    None,
                    None,
                    None,
                    format!("Channel-{} ({}) handshake accepted", idx + 1, ch_name),
                    None,
                );
                if let Some(ranges) = resume_from {
                    resume_ranges_combined.extend(ranges);
                }
            }
            Err(e) => {
                telemetry.record_event(
                    TransferStage::Handshake,
                    EventLevel::Warn,
                    &format!("Channel-{}", idx + 1),
                    None,
                    None,
                    None,
                    format!("Channel-{} ({}) handshake failed, skipping: {}", idx + 1, ch_name, e),
                    None,
                );
                log::warn!("Multipath channel-{} ({}) handshake failed: {}", idx + 1, ch_name, e);
                failed_indices.push(idx);
            }
        }
    }

    // Remove failed transports in reverse order to preserve indices
    for &idx in failed_indices.iter().rev() {
        transports.remove(idx);
    }

    if transports.is_empty() {
        return Err(TransferSessionError::Transport(
            crate::transport::TransportError::Disconnected(
                "All multipath channels failed handshake — no usable transport".into(),
            ),
        ));
    }

    // 2. Data Plane: Shared state across all transports
    let mut initial_chunks_to_send = std::collections::VecDeque::new();
    let mut bytes_sent_total_init = 0u64;
    let mut completed_chunks_count_init = 0u32;
    let mut completed_set_init = std::collections::HashSet::new();

    for entry in &plan {
        let cid = entry.chunk_id;
        let skip = resume_ranges_combined
            .iter()
            .any(|&(start, end)| cid >= start && cid <= end);
        if skip {
            bytes_sent_total_init += entry.payload_length as u64;
            completed_chunks_count_init += 1;
            completed_set_init.insert(cid);
        } else {
            initial_chunks_to_send.push_back(entry.clone());
        }
    }

    update_transfer_progress(transfer_id, bytes_sent_total_init, completed_chunks_count_init);

    let total_chunks_needed = initial_chunks_to_send.len();
    if total_chunks_needed == 0 {
        // All chunks already completed -> complete immediately
        let t_fin0 = std::time::Instant::now();
        let file_checksum = compute_file_crc32c(file_path)?;
        let complete_msg = Message::Complete(CompleteData {
            transfer_id,
            file_checksum,
        });
        transports[0].0.send_frame(&complete_msg).await?;
        let final_frame = transports[0].0
            .receive_frame()
            .await?
            .ok_or_else(|| TransferSessionError::UnexpectedMessage("EOF waiting for completion ACK".into()))?;
        if !matches!(final_frame, Message::ChunkAck(_)) {
            return Err(TransferSessionError::UnexpectedMessage(format!(
                "Expected final Ack, got {:?}",
                final_frame
            )));
        }
        let fin_ms = t_fin0.elapsed().as_millis() as u64;
        telemetry.record_finalize(fin_ms, true);
        telemetry.mark_completed();
        let data_dir = default_data_dir();
        export_and_clean_telemetry(transfer_id, &data_dir);
        return Ok(());
    }

    let (prepared_tx, prepared_rx) = async_channel::bounded::<PreparedChunk>(48);
    let (retry_tx, retry_rx) = std::sync::mpsc::channel::<crate::chunk::ChunkPlanEntry>();
    let (recycle_tx, recycle_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let is_cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (running_crc_tx, running_crc_rx) = tokio::sync::oneshot::channel::<u32>();
    let total_plan_chunks = plan.len();

    let reader_file_path = file_path.to_path_buf();
    let reader_cancelled = std::sync::Arc::clone(&is_cancelled);
    let chunk_size_bytes = manifest.chunk_size as usize;
    let mut pending_reader_chunks = initial_chunks_to_send;
    let resume_ranges_cloned = resume_ranges_combined.clone();
    let plan_map_for_reader = plan_map.clone();
    let tel_reader = telemetry.clone();

    let reader_handle = tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
        let mut file = crate::util::storage::open_sequential_read(&reader_file_path)?;
        let mut buffer_pool: Vec<Vec<u8>> = Vec::with_capacity(32);
        let mut chunk_crc_map: std::collections::HashMap<u32, (u32, usize)> = std::collections::HashMap::new();
        let mut crc_tx_opt = Some(running_crc_tx);

        // Pre-calculate CRC for skipped chunks so sender has all total_plan_chunks in chunk_crc_map without re-reading whole file
        for &(start, end) in &resume_ranges_cloned {
            for cid in start..=end {
                if let Some(entry) = plan_map_for_reader.get(&cid) {
                    if !chunk_crc_map.contains_key(&cid) {
                        let mut buf = vec![0u8; entry.payload_length as usize];
                        if crate::chunk::read_chunk_into_slice(&mut file, entry.file_offset, &mut buf).is_ok() {
                            let chunk_crc = crate::checksum::compute_crc32c(&buf);
                            chunk_crc_map.insert(cid, (chunk_crc, buf.len()));
                        }
                    }
                }
            }
        }

        loop {
            if reader_cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }

            while let Ok(buf) = recycle_rx.try_recv() {
                buffer_pool.push(buf);
            }

            let next_entry = if let Ok(entry) = retry_rx.try_recv() {
                Some(entry)
            } else if let Some(entry) = pending_reader_chunks.pop_front() {
                Some(entry)
            } else {
                if chunk_crc_map.len() == total_plan_chunks {
                    if let Some(tx) = crc_tx_opt.take() {
                        let mut acc = crate::checksum::Crc32cAccumulator::new();
                        for cid in 0..total_plan_chunks as u32 {
                            if let Some(&(crc, len)) = chunk_crc_map.get(&cid) {
                                acc.combine(crc, len);
                            }
                        }
                        let _ = tx.send(acc.finalize());
                    }
                }
                match retry_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                    Ok(entry) => Some(entry),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            };

            let entry = match next_entry {
                Some(e) => e,
                None => break,
            };

            let mut buf = buffer_pool.pop().unwrap_or_else(|| Vec::with_capacity(chunk_size_bytes));
            buf.resize(entry.payload_length as usize, 0);

            use std::io::Seek;
            let t_r0 = std::time::Instant::now();
            file.seek(std::io::SeekFrom::Start(entry.file_offset))?;
            crate::chunk::read_chunk_into_slice(&mut file, entry.file_offset, &mut buf)?;
            let read_us = t_r0.elapsed().as_micros() as u64;

            let t_h0 = std::time::Instant::now();
            let chunk_crc = crate::checksum::compute_crc32c(&buf);
            chunk_crc_map.insert(entry.chunk_id, (chunk_crc, buf.len()));
            let checksum = compute_xxhash64(&buf);
            let hash_us = t_h0.elapsed().as_micros() as u64;

            tel_reader.record_chunk_read(entry.chunk_id, entry.payload_length as u64, read_us, hash_us);

            if prepared_tx.send_blocking(PreparedChunk { entry, payload: buf, checksum }).is_err() {
                break;
            }
        }

        // In-flight O(1) finalization: combine CRC32Cs of all chunks in order if read completely
        if let Some(tx) = crc_tx_opt.take() {
            if chunk_crc_map.len() == total_plan_chunks {
                let mut acc = crate::checksum::Crc32cAccumulator::new();
                for cid in 0..total_plan_chunks as u32 {
                    if let Some(&(crc, len)) = chunk_crc_map.get(&cid) {
                        acc.combine(crc, len);
                    }
                }
                let _ = tx.send(acc.finalize());
            } else if let Ok(crc) = crate::checksum::compute_file_crc32c(&reader_file_path) {
                let _ = tx.send(crc);
            }
        }

        Ok(())
    });

    let shared_retry_tx = std::sync::Arc::new(retry_tx);
    let shared_recycle_tx = std::sync::Arc::new(recycle_tx);
    let shared_completed = std::sync::Arc::new(parking_lot::Mutex::new(completed_set_init));
    let shared_completed_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(completed_chunks_count_init as usize));
    let shared_bytes_sent = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(bytes_sent_total_init));
    let shared_chunks_done = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(completed_chunks_count_init));
    let shared_plan_map = std::sync::Arc::new(plan_map);

    let mut worker_handles = Vec::new();

    for (idx, (mut transport, is_usb)) in transports.into_iter().enumerate() {
        let prepared_rx = prepared_rx.clone();
        let retry_tx = std::sync::Arc::clone(&shared_retry_tx);
        let recycle_tx = std::sync::Arc::clone(&shared_recycle_tx);
        let completed = std::sync::Arc::clone(&shared_completed);
        let completed_count = std::sync::Arc::clone(&shared_completed_count);
        let bytes_sent = std::sync::Arc::clone(&shared_bytes_sent);
        let chunks_done = std::sync::Arc::clone(&shared_chunks_done);
        let plan_map = std::sync::Arc::clone(&shared_plan_map);
        let cancelled = std::sync::Arc::clone(&is_cancelled);
        let file_id = manifest.file_id;
        let total_chunks = plan.len();
        let telemetry_worker = Some(telemetry.clone());
        let channel_name = if is_usb {
            "USB".to_string()
        } else {
            format!("WiFi-Stream-{}", idx + 1)
        };

        let handle = tokio::spawn(async move {
            let worker_pipeline_depth: usize = if is_usb { 16 } else { 8 };
            let mut worker_in_flight = std::collections::HashSet::new();
            let mut worker_in_flight_times = std::collections::HashMap::new();

            loop {
                if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                    if worker_in_flight.is_empty() {
                        break;
                    }
                }

                if completed_count.load(std::sync::atomic::Ordering::Relaxed) >= total_chunks {
                    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                    prepared_rx.close();
                    if worker_in_flight.is_empty() {
                        break;
                    }
                }

                // Check transfer control status
                match transfer_control_status(transfer_id) {
                    Some(TransferStatus::Paused) => {
                        cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                        prepared_rx.close();
                        if let Some(ref tel) = telemetry_worker {
                            tel.record_event(TransferStage::Control, EventLevel::Info, &channel_name, None, None, None, "Transfer paused by user", None);
                        }
                        return Err(TransferSessionError::Paused);
                    }
                    Some(TransferStatus::Cancelled) => {
                        cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                        prepared_rx.close();
                        if let Some(ref tel) = telemetry_worker {
                            tel.record_event(TransferStage::Control, EventLevel::Info, &channel_name, None, None, None, "Transfer cancelled by user", None);
                        }
                        return Err(TransferSessionError::Cancelled);
                    }
                    _ => {}
                }

                tokio::select! {
                    biased;
                    frame_res = transport.receive_frame() => {
                        match frame_res {
                            Ok(Some(frame)) => {
                                handle_multipath_ack_frame(
                                    frame,
                                    is_usb,
                                    &mut worker_in_flight,
                                    &mut worker_in_flight_times,
                                    &completed,
                                    &completed_count,
                                    &plan_map,
                                    &retry_tx,
                                    transfer_id,
                                    &bytes_sent,
                                    &chunks_done,
                                    telemetry_worker.as_ref(),
                                    &channel_name,
                                ).await?;
                                if completed_count.load(std::sync::atomic::Ordering::Relaxed) >= total_chunks {
                                    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                                    prepared_rx.close();
                                    break;
                                }
                            }
                            Ok(None) | Err(_) => {
                                // Transport disconnected
                                for cid in worker_in_flight.drain() {
                                    worker_in_flight_times.remove(&cid);
                                    if let Some(e) = plan_map.get(&cid) {
                                        let _ = retry_tx.send(e.clone());
                                    }
                                }
                                if let Some(ref tel) = telemetry_worker {
                                    tel.record_channel_disconnect(&channel_name, "Transport disconnected / EOF");
                                }
                                log::warn!("Multipath transport #{} ({}) disconnected -> requeued in-flight chunks", idx, channel_name);
                                return Ok((idx, transport, false));
                            }
                        }
                    }
                    prepared_res = prepared_rx.recv(), if worker_in_flight.len() < worker_pipeline_depth && !cancelled.load(std::sync::atomic::Ordering::Relaxed) => {
                        match prepared_res {
                            Ok(prepared) => {
                                let chunk_id = prepared.entry.chunk_id;
                                let chunk_msg = Message::ChunkData(ChunkDataPayload {
                                    transfer_id,
                                    file_id,
                                    chunk_id,
                                    file_offset: prepared.entry.file_offset,
                                    payload_length: prepared.entry.payload_length,
                                    checksum: prepared.checksum,
                                    payload: prepared.payload,
                                });

                                let t_s0 = std::time::Instant::now();
                                if let Err(e) = transport.send_frame(&chunk_msg).await {
                                    let _ = retry_tx.send(prepared.entry);
                                    for cid in worker_in_flight.drain() {
                                        worker_in_flight_times.remove(&cid);
                                        if let Some(e) = plan_map.get(&cid) {
                                            let _ = retry_tx.send(e.clone());
                                        }
                                    }
                                    if let Some(ref tel) = telemetry_worker {
                                        tel.record_channel_disconnect(&channel_name, &format!("Send error: {}", e));
                                    }
                                    log::warn!("Multipath transport #{} send failed: {} -> requeued chunks", idx, e);
                                    return Ok((idx, transport, false));
                                }
                                let send_us = t_s0.elapsed().as_micros() as u64;
                                if let Some(ref tel) = telemetry_worker {
                                    tel.record_chunk_sent(&channel_name, chunk_id, prepared.entry.payload_length as u64, send_us);
                                }
                                worker_in_flight_times.insert(chunk_id, std::time::Instant::now());

                                if let Message::ChunkData(d) = chunk_msg {
                                    let _ = recycle_tx.send(d.payload);
                                }

                                worker_in_flight.insert(chunk_id);
                            }
                            Err(_) => {
                                // Channel closed (reader finished or encountered error)
                                if worker_in_flight.is_empty() {
                                    break;
                                }
                            }
                        }
                    }
                }
            }

            Ok((idx, transport, true))
        });

        worker_handles.push(handle);
    }

    // Await all workers
    let mut returned_transports = Vec::new();
    for handle in worker_handles {
        if let Ok(Ok((_idx, transport, is_alive))) = handle.await {
            if is_alive {
                returned_transports.push(transport);
            }
        }
    }

    is_cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    drop(shared_retry_tx);
    drop(shared_recycle_tx);
    let _ = reader_handle.await;

    let final_done = {
        let c = shared_completed.lock();
        c.len()
    };

    if final_done < plan.len() {
        telemetry.mark_failed("All multipath transports disconnected before completing transfer");
        let data_dir = default_data_dir();
        export_and_clean_telemetry(transfer_id, &data_dir);
        return Err(TransferSessionError::Transport(TransportError::Disconnected(
            "All multipath transports disconnected before completing transfer".into(),
        )));
    }

    // 3. Complete transfer on the first surviving transport
    let t_fin0 = std::time::Instant::now();
    if let Some(mut primary_transport) = returned_transports.into_iter().next() {
        let file_checksum = match running_crc_rx.await {
            Ok(c) => c,
            Err(_) => compute_file_crc32c(file_path)?,
        };
        let complete_msg = Message::Complete(CompleteData {
            transfer_id,
            file_checksum,
        });
        primary_transport.send_frame(&complete_msg).await?;
        loop {
            let final_frame = primary_transport
                .receive_frame()
                .await?
                .ok_or_else(|| TransferSessionError::UnexpectedMessage("EOF waiting for completion ACK".into()))?;
            match final_frame {
                Message::ChunkAck(ack) if ack.chunk_id == u32::MAX => break,
                Message::ChunkAck(_) | Message::BatchChunkAck(_) => continue,
                other => {
                    return Err(TransferSessionError::UnexpectedMessage(format!(
                        "Expected final Ack, got {:?}",
                        other
                    )));
                }
            }
        }
    }

    let fin_ms = t_fin0.elapsed().as_millis() as u64;
    telemetry.record_finalize(fin_ms, true);
    telemetry.mark_completed();
    let data_dir = default_data_dir();
    export_and_clean_telemetry(transfer_id, &data_dir);

    Ok(())
}

/// Convenience wrapper running `send_file_session` over a raw asynchronous stream.
pub async fn send_file_session_stream<S>(
    sender_device_id: Uuid,
    sender_device_name: &str,
    file_path: &Path,
    chunk_size: u32,
    transfer_id: Uuid,
    stream: S,
) -> Result<(), TransferSessionError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
{
    let transport = StreamTransport::new(stream, TransportKind::Tcp);
    send_file_session(
        sender_device_id,
        sender_device_name,
        file_path,
        chunk_size,
        transfer_id,
        transport,
        None,
        None,
    )
    .await
}

/// Runs the receiver side of a transfer session over any generic `Transport` (§6, §7, §8, §9).
/// Uses a persistent open file handle across all chunk writes to eliminate I/O reopening overhead.
pub async fn receive_file_session<T, Tr>(
    receiver_device_id: Uuid,
    receiver_device_name: &str,
    dest_dir: &Path,
    tracker: &mut Tr,
    mut transport: T,
) -> Result<PathBuf, TransferSessionError>
where
    T: Transport,
    Tr: ChunkTracker,
{
    // 1. Await Sender Hello
    let sender_hello = transport
        .receive_frame()
        .await?
        .ok_or(TransferSessionError::UnexpectedMessage(
            "EOF waiting for Hello".into(),
        ))?;
    if !matches!(sender_hello, Message::Hello(_)) {
        return Err(TransferSessionError::UnexpectedMessage(format!(
            "Expected Hello, got {:?}",
            sender_hello
        )));
    }

    // 2. Send Receiver Hello
    let hello = Message::Hello(HelloData {
        device_id: receiver_device_id,
        device_name: receiver_device_name.to_string(),
        protocol_version: 1,
    });
    transport.send_frame(&hello).await?;

    // 3. Await TransferOffer
    let offer_msg = transport
        .receive_frame()
        .await?
        .ok_or(TransferSessionError::UnexpectedMessage(
            "EOF waiting for TransferOffer".into(),
        ))?;

    let offer = match offer_msg {
        Message::TransferOffer(o) => o,
        other => {
            return Err(TransferSessionError::UnexpectedMessage(format!(
                "Expected TransferOffer, got {:?}",
                other
            )));
        }
    };

    // 4. Send TransferAccept
    let resume_from = tracker.get_completed_ranges();
    let accept = Message::TransferAccept(TransferAcceptData {
        transfer_id: offer.transfer_id,
        resume_from: resume_from.clone(),
    });
    transport.send_frame(&accept).await?;

    // Register incoming transfer
    register_active_transfer(
        offer.transfer_id,
        offer.file_name.clone(),
        offer.file_size,
        TransferRole::Receiver,
        offer.total_chunks,
        "TCP / USB Transport".to_string(),
    );

    let telemetry = get_or_create_telemetry(
        offer.transfer_id,
        &offer.file_name,
        offer.file_size,
        TransferRole::Receiver,
    );

    // 5. Create and pre-allocate .part file, keeping handle open for the entire session
    std::fs::create_dir_all(dest_dir)?;
    let part_path = dest_dir.join(format!("{}.part", offer.file_name));
    let final_path = dest_dir.join(&offer.file_name);

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&part_path)?;
    crate::util::storage::preallocate_file(&file, offer.file_size)?;

    let mut chunk_crcs: std::collections::HashMap<u32, (u32, usize)> = std::collections::HashMap::new();

    // Pre-calculate CRC for resumed chunks from open file handle directly
    if let Some(ref ranges) = resume_from {
        use std::io::{Read, Seek, SeekFrom};
        for &(start, end) in ranges {
            for cid in start..=end {
                let offset = (cid as u64) * (offer.chunk_size as u64);
                let len = if cid == offer.total_chunks - 1 {
                    (offer.file_size - offset) as usize
                } else {
                    offer.chunk_size as usize
                };
                let mut buf = vec![0u8; len];
                if file.seek(SeekFrom::Start(offset)).is_ok() && file.read_exact(&mut buf).is_ok() {
                    let chunk_crc = crate::checksum::compute_crc32c(&buf);
                    chunk_crcs.insert(cid, (chunk_crc, buf.len()));
                }
            }
        }
    }

    // Spawn high-throughput background disk writer to decouple disk I/O from TCP socket reads
    struct DiskWriteTask {
        chunk_id: u32,
        file_offset: u64,
        payload: Vec<u8>,
        queue_depth: u32,
    }

    let (disk_tx, mut disk_rx) = tokio::sync::mpsc::channel::<DiskWriteTask>(128);
    let mut writer_file = file;
    let tel_for_disk = telemetry.clone();
    let disk_writer_handle = tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
        use std::io::{Seek, SeekFrom, Write};
        while let Some(task) = disk_rx.blocking_recv() {
            let t_w0 = std::time::Instant::now();
            let len = task.payload.len() as u64;
            writer_file.seek(SeekFrom::Start(task.file_offset))?;
            writer_file.write_all(&task.payload)?;
            let write_us = t_w0.elapsed().as_micros() as u64;
            tel_for_disk.record_disk_write(task.chunk_id, len, write_us, task.queue_depth);
        }
        writer_file.flush()?;
        drop(writer_file);
        Ok(())
    });

    let mut bytes_recv_total = 0u64;
    let mut completed_chunks_count = 0u32;

    // 6. Data Plane Receive Loop
    loop {
        let frame_res = transport.receive_frame().await;
        let frame = match frame_res {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return Err(TransferSessionError::Transport(e)),
        };

        match frame {
            Message::ChunkData(chunk_data) => {
                let t_v0 = std::time::Instant::now();
                let computed_checksum = compute_xxhash64(&chunk_data.payload);
                let verify_us = t_v0.elapsed().as_micros() as u32;

                if computed_checksum != chunk_data.checksum {
                    let nack = Message::ChunkNack(ChunkNackData {
                        transfer_id: chunk_data.transfer_id,
                        chunk_id: chunk_data.chunk_id,
                        reason: "xxHash64 payload mismatch".to_string(),
                    });
                    transport.send_frame(&nack).await?;
                    continue;
                }

                // Idempotent write check (§5.1)
                if tracker.is_chunk_completed(
                    chunk_data.transfer_id,
                    chunk_data.file_id,
                    chunk_data.chunk_id,
                    chunk_data.checksum,
                ) {
                    if !chunk_crcs.contains_key(&chunk_data.chunk_id) {
                        let chunk_crc = crate::checksum::compute_crc32c(&chunk_data.payload);
                        chunk_crcs.insert(chunk_data.chunk_id, (chunk_crc, chunk_data.payload.len()));
                    }
                    let ack = Message::ChunkAck(ChunkAckData {
                        transfer_id: chunk_data.transfer_id,
                        chunk_id: chunk_data.chunk_id,
                        receiver_verify_us: Some(verify_us),
                    });
                    transport.send_frame(&ack).await?;
                    continue;
                }

                let chunk_crc = crate::checksum::compute_crc32c(&chunk_data.payload);
                chunk_crcs.insert(chunk_data.chunk_id, (chunk_crc, chunk_data.payload.len()));

                tracker.mark_chunk_completed(
                    chunk_data.transfer_id,
                    chunk_data.file_id,
                    chunk_data.chunk_id,
                    chunk_data.checksum,
                );

                let chunk_len = chunk_data.payload_length as u64;
                bytes_recv_total += chunk_len;
                completed_chunks_count += 1;
                update_transfer_progress(
                    chunk_data.transfer_id,
                    bytes_recv_total,
                    completed_chunks_count,
                );
                let is_usb = transport.kind() == crate::transport::TransportKind::Usb;
                crate::transfer::api::record_channel_bytes(chunk_data.transfer_id, is_usb, chunk_len);

                // Send immediate ChunkAck for 100% universal sender compatibility
                let ack = Message::ChunkAck(ChunkAckData {
                    transfer_id: chunk_data.transfer_id,
                    chunk_id: chunk_data.chunk_id,
                    receiver_verify_us: Some(verify_us),
                });
                transport.send_frame(&ack).await?;

                // Dispatch disk write to background worker (async backpressure if queue fills)
                let q_depth = (128 - disk_tx.capacity()) as u32;
                let _ = disk_tx.send(DiskWriteTask {
                    chunk_id: chunk_data.chunk_id,
                    file_offset: chunk_data.file_offset,
                    payload: chunk_data.payload,
                    queue_depth: q_depth,
                }).await;
            }
            Message::Complete(complete_data) => {
                let t_fin0 = std::time::Instant::now();
                // Drop writer channel and await completion of all background disk writes
                drop(disk_tx);
                if let Ok(res) = disk_writer_handle.await {
                    res?;
                }

                // In-Flight O(1) Castagnoli CRC32C verification
                let file_crc = {
                    if chunk_crcs.len() == offer.total_chunks as usize {
                        let mut acc = crate::checksum::Crc32cAccumulator::new();
                        for cid in 0..offer.total_chunks {
                            if let Some(&(crc, len)) = chunk_crcs.get(&cid) {
                                acc.combine(crc, len);
                            }
                        }
                        acc.finalize()
                    } else {
                        compute_file_crc32c(&part_path)?
                    }
                };

                if file_crc != complete_data.file_checksum {
                    set_transfer_status(
                        complete_data.transfer_id,
                        TransferStatus::Failed,
                        Some("CRC32C mismatch".to_string()),
                    );
                    telemetry.mark_failed("CRC32C mismatch");
                    let data_dir = default_data_dir();
                    export_and_clean_telemetry(complete_data.transfer_id, &data_dir);
                    return Err(TransferSessionError::ChecksumMismatch(format!(
                        "File CRC32C mismatch: expected 0x{:08X}, got 0x{:08X}",
                        complete_data.file_checksum, file_crc
                    )));
                }

                // Rename .part file to final file name
                rename(&part_path, &final_path)?;

                set_transfer_status(complete_data.transfer_id, TransferStatus::Completed, None);

                let fin_ms = t_fin0.elapsed().as_millis() as u64;
                telemetry.record_finalize(fin_ms, true);
                telemetry.mark_completed();
                let data_dir = default_data_dir();
                export_and_clean_telemetry(complete_data.transfer_id, &data_dir);

                // Send final completion Ack
                let ack = Message::ChunkAck(ChunkAckData {
                    transfer_id: complete_data.transfer_id,
                    chunk_id: u32::MAX,
                    receiver_verify_us: None,
                });
                transport.send_frame(&ack).await?;
                return Ok(final_path);
            }
            other => {
                return Err(TransferSessionError::UnexpectedMessage(format!(
                    "Unexpected frame in data plane: {:?}",
                    other
                )));
            }
        }
    }

    Err(TransferSessionError::UnexpectedMessage(
        "Stream closed before Complete frame".into(),
    ))
}

/// Convenience wrapper running `receive_file_session` over a raw asynchronous stream.
pub async fn receive_file_session_stream<S, Tr>(
    receiver_device_id: Uuid,
    receiver_device_name: &str,
    dest_dir: &Path,
    tracker: &mut Tr,
    stream: S,
) -> Result<PathBuf, TransferSessionError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
    Tr: ChunkTracker,
{
    let transport = StreamTransport::new(stream, TransportKind::Tcp);
    receive_file_session(
        receiver_device_id,
        receiver_device_name,
        dest_dir,
        tracker,
        transport,
    )
    .await
}


d:\MyDocuments\Programming\android\Aug26\TurboTransfer\core\src\scheduler\multipath.rs

use log::{debug, error, info, warn};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, Notify};
use uuid::Uuid;

use super::buffer_pool::BufferPool;
use super::metrics::ThroughputTracker;
use crate::checksum::compute_xxhash64;
use crate::chunk::{calculate_chunk_plan, read_chunk_at, total_chunks};
use crate::manifest::actor::{MetaActorHandle, TransportType};
use crate::manifest::TransferStatus;
use crate::protocol::{ChunkAckData, ChunkDataPayload, ChunkNackData, Message};
use crate::transport::{Transport, TransportError, TransportKind};

/// Default maximum in-flight chunks per active transport (4 per TRD §10.1).
pub const DEFAULT_MAX_IN_FLIGHT_PER_TRANSPORT: usize = 4;

/// Default buffer pool count (8 buffers per TRD §10.2).
pub const DEFAULT_BUFFER_COUNT: usize = 8;

/// Configuration options for the Multipath Scheduler.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub max_in_flight_per_transport: usize,
    pub buffer_count: usize,
    pub chunk_size: usize,
    pub enable_dynamic_scheduler: bool,
    pub enable_dynamic_window: bool,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_in_flight_per_transport: DEFAULT_MAX_IN_FLIGHT_PER_TRANSPORT,
            buffer_count: DEFAULT_BUFFER_COUNT,
            chunk_size: 64 * 1024 * 1024, // 64 MiB default
            enable_dynamic_scheduler: false,
            enable_dynamic_window: false,
        }
    }
}

/// Candidate evaluation snapshot for explainable scheduler decisions.
#[derive(Debug, Clone)]
pub struct CandidateEval {
    pub channel_name: String,
    pub ewma_throughput_mbps: f64,
    pub in_flight_bytes: u64,
    pub estimated_completion_us: u64,
    pub max_window: usize,
    pub current_in_flight: usize,
}

/// Decision event explaining why a particular channel was chosen.
#[derive(Debug, Clone)]
pub struct SchedulerDecision {
    pub chunk_id: u32,
    pub selected_channel: String,
    pub candidates: Vec<CandidateEval>,
    pub reason: String,
}


/// Dynamic rate-adaptive multipath chunk scheduler (§10).
pub struct MultipathScheduler {
    transfer_id: Uuid,
    file_id: Uuid,
    file_path: PathBuf,
    file_size: u64,
    total_chunks_count: u32,
    chunk_size: usize,
    config: SchedulerConfig,
    pending_chunks: Arc<Mutex<VecDeque<u32>>>,
    in_flight: Arc<Mutex<HashMap<u32, (TransportKind, Instant)>>>,
    completed_set: Arc<Mutex<std::collections::HashSet<u32>>>,
    completed_chunks: Arc<AtomicU32>,
    transports: Arc<Mutex<Vec<Arc<tokio::sync::Mutex<Box<dyn Transport>>>>>>,
    metrics: Arc<ThroughputTracker>,
    status: Arc<Mutex<TransferStatus>>,
    manifest_actor: Option<MetaActorHandle>,
    pause_notify: Arc<Notify>,
    cancelled: Arc<AtomicBool>,
}

impl MultipathScheduler {
    /// Creates a new multipath scheduler session for a sender transfer.
    pub fn new(
        transfer_id: Uuid,
        file_id: Uuid,
        file_path: PathBuf,
        file_size: u64,
        chunk_size: usize,
        config: SchedulerConfig,
        completed_initial: Vec<u32>,
        manifest_actor: Option<MetaActorHandle>,
    ) -> Self {
        let total_chunks_count = total_chunks(file_size, chunk_size as u32);
        let completed_set_init: std::collections::HashSet<u32> = completed_initial.into_iter().collect();
        let mut pending = VecDeque::new();
        for i in 0..total_chunks_count {
            if !completed_set_init.contains(&i) {
                pending.push_back(i);
            }
        }

        let completed_count = (total_chunks_count - pending.len() as u32) as u32;

        Self {
            transfer_id,
            file_id,
            file_path,
            file_size,
            total_chunks_count,
            chunk_size,
            config,
            pending_chunks: Arc::new(Mutex::new(pending)),
            in_flight: Arc::new(Mutex::new(HashMap::new())),
            completed_set: Arc::new(Mutex::new(completed_set_init)),
            completed_chunks: Arc::new(AtomicU32::new(completed_count)),
            transports: Arc::new(Mutex::new(Vec::new())),
            metrics: Arc::new(ThroughputTracker::default()),
            status: Arc::new(Mutex::new(TransferStatus::InProgress)),
            manifest_actor,
            pause_notify: Arc::new(Notify::new()),
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Registers an active transport with the multipath scheduler.
    pub async fn add_transport(&self, transport: Box<dyn Transport>) {
        let mut list = self.transports.lock().await;
        list.push(Arc::new(tokio::sync::Mutex::new(transport)));
        self.pause_notify.notify_waiters();
    }

    /// Returns reference to throughput and metrics tracker.
    pub fn metrics(&self) -> &ThroughputTracker {
        &self.metrics
    }

    /// Returns current transfer progress summary.
    pub async fn get_status(&self) -> TransferStatus {
        *self.status.lock().await
    }

    /// Returns total completed chunks.
    pub fn completed_chunks(&self) -> u32 {
        self.completed_chunks.load(Ordering::Relaxed)
    }

    /// Returns total chunks for this transfer.
    pub fn total_chunks(&self) -> u32 {
        self.total_chunks_count
    }

    /// Manually pauses the transfer.
    pub async fn pause(&self) {
        let mut st = self.status.lock().await;
        *st = TransferStatus::Paused;
        info!("Multipath transfer {} paused", self.transfer_id);
    }

    /// Manually resumes the transfer.
    pub async fn resume(&self) {
        let mut st = self.status.lock().await;
        *st = TransferStatus::InProgress;
        self.pause_notify.notify_waiters();
        info!("Multipath transfer {} resumed", self.transfer_id);
    }

    /// Manually cancels the transfer.
    pub async fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        let mut st = self.status.lock().await;
        *st = TransferStatus::Cancelled;
        self.pause_notify.notify_waiters();
        info!("Multipath transfer {} cancelled", self.transfer_id);
    }

    /// Runs the sender multipath scheduler loop until all chunks are completed or cancelled.
    pub async fn run_sender(&self) -> Result<(), TransportError> {
        info!(
            "Starting Multipath Scheduler sender loop: transfer_id={}, total_chunks={}",
            self.transfer_id, self.total_chunks_count
        );

        let _buffer_pool = BufferPool::new(self.config.buffer_count, self.chunk_size);
        let plan = calculate_chunk_plan(self.file_size, self.chunk_size as u32);

        while !self.cancelled.load(Ordering::Relaxed) {
            // Check if transfer is completed
            if self.completed_chunks.load(Ordering::Relaxed) >= self.total_chunks_count {
                let mut st = self.status.lock().await;
                *st = TransferStatus::Completed;
                info!("Multipath transfer {} completed successfully!", self.transfer_id);
                return Ok(());
            }

            // Check if paused
            {
                let st = *self.status.lock().await;
                if st == TransferStatus::Paused {
                    debug!("Transfer is paused -> waiting for resume signal");
                    self.pause_notify.notified().await;
                    continue;
                }
            }

            // Get active transports
            let active_transports = {
                let list = self.transports.lock().await;
                list.clone()
            };

            if active_transports.is_empty() {
                warn!("No active transports available -> moving transfer to Paused state");
                let mut st = self.status.lock().await;
                *st = TransferStatus::Paused;
                if let Some(actor) = &self.manifest_actor {
                    actor.pause().await;
                }
                self.pause_notify.notified().await;
                continue;
            }

            // Iterate over transports and dispatch chunks when under in-flight capacity (§10.1)
            let mut dispatched_any = false;

            for transport_arc in &active_transports {
                let mut transport_guard = transport_arc.lock().await;
                let kind = transport_guard.kind();

                if !transport_guard.is_connected() {
                    continue;
                }

                // Check in-flight count for this transport
                let in_flight_for_transport = {
                    let map = self.in_flight.lock().await;
                    map.values().filter(|(t, _)| *t == kind).count()
                };

                if in_flight_for_transport >= self.config.max_in_flight_per_transport {
                    continue;
                }

                // Pull next pending chunk from FIFO queue
                let next_chunk_id = {
                    let mut queue = self.pending_chunks.lock().await;
                    queue.pop_front()
                };

                let chunk_id = match next_chunk_id {
                    Some(id) => id,
                    None => break, // No more pending chunks
                };

                // Track chunk as in-flight
                {
                    let mut map = self.in_flight.lock().await;
                    map.insert(chunk_id, (kind, Instant::now()));
                }

                // Read chunk payload asynchronously from disk buffer pool (§10.2)
                let entry = &plan[chunk_id as usize];
                let payload = match read_chunk_at(&self.file_path, entry.file_offset, entry.payload_length) {
                    Ok(data) => data,
                    Err(e) => {
                        error!("Failed to read chunk #{} from disk: {} -> requeueing", chunk_id, e);
                        let mut map = self.in_flight.lock().await;
                        map.remove(&chunk_id);
                        let mut queue = self.pending_chunks.lock().await;
                        queue.push_front(chunk_id);
                        continue;
                    }
                };

                let checksum = compute_xxhash64(&payload);
                let chunk_msg = Message::ChunkData(ChunkDataPayload {
                    transfer_id: self.transfer_id,
                    file_id: self.file_id,
                    chunk_id,
                    file_offset: entry.file_offset,
                    payload_length: entry.payload_length,
                    checksum,
                    payload: payload.to_vec(),
                });

                // Send frame over transport
                match transport_guard.send_frame(&chunk_msg).await {
                    Ok(_) => {
                        debug!("Dispatched chunk #{} on {}", chunk_id, kind);
                        dispatched_any = true;
                    }
                    Err(e) => {
                        warn!("Transport {} send failed on chunk #{}: {} -> requeueing in-flight", kind, chunk_id, e);
                        self.metrics.record_retry(kind);

                        // Requeue failed chunk
                        {
                            let mut map = self.in_flight.lock().await;
                            map.remove(&chunk_id);
                            let mut queue = self.pending_chunks.lock().await;
                            queue.push_front(chunk_id);
                        }

                        // Requeue all other in-flight chunks on this dropped transport (§10.5)
                        self.requeue_transport_in_flight(kind).await;
                    }
                }
            }

            // Yield / poll ACKs
            if !dispatched_any {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }

        Ok(())
    }

    /// Handles an incoming ACK for a completed chunk.
    pub async fn handle_chunk_ack(&self, ack: &ChunkAckData, kind: TransportKind, chunk_len: u64) {
        {
            let mut map = self.in_flight.lock().await;
            map.remove(&ack.chunk_id);
        }

        let is_new = {
            let mut set = self.completed_set.lock().await;
            set.insert(ack.chunk_id)
        };

        if is_new {
            self.completed_chunks.fetch_add(1, Ordering::Relaxed);
            self.metrics.record_transport_bytes(kind, chunk_len);

            if let Some(actor) = &self.manifest_actor {
                let transport_type = match kind {
                    TransportKind::Usb => TransportType::Usb,
                    _ => TransportType::WifiDirect,
                };
                actor.send_chunk_completed(ack.chunk_id, transport_type, chunk_len).await;
            }

            debug!("Chunk ACK #{} recorded on {}", ack.chunk_id, kind);
        } else {
            debug!("Duplicate Chunk ACK #{} ignored as idempotent no-op", ack.chunk_id);
        }
    }

    /// Handles an incoming NACK for a corrupted chunk (§10.3).
    pub async fn handle_chunk_nack(&self, nack: &ChunkNackData, kind: TransportKind) {
        warn!("Chunk NACK #{} received: {} -> requeueing for retry", nack.chunk_id, nack.reason);
        self.metrics.record_retry(kind);

        if let Some(actor) = &self.manifest_actor {
            let transport_type = match kind {
                TransportKind::Usb => TransportType::Usb,
                _ => TransportType::WifiDirect,
            };
            actor.send_chunk_failed(nack.chunk_id, transport_type).await;
        }

        {
            let mut map = self.in_flight.lock().await;
            map.remove(&nack.chunk_id);
        }

        {
            let mut queue = self.pending_chunks.lock().await;
            queue.push_front(nack.chunk_id);
        }
    }

    /// Requeues all in-flight chunks belonging to a disconnected transport back to the shared pending queue (§10.5).
    pub async fn requeue_transport_in_flight(&self, kind: TransportKind) {
        let mut map = self.in_flight.lock().await;
        let mut queue = self.pending_chunks.lock().await;

        let failed_chunks: Vec<u32> = map
            .iter()
            .filter(|(_, (t, _))| *t == kind)
            .map(|(cid, _)| *cid)
            .collect();

        for cid in failed_chunks {
            map.remove(&cid);
            queue.push_front(cid);
            debug!("Requeued unacked in-flight chunk #{} from {} back to pending queue", cid, kind);
        }
    }
}


d:\MyDocuments\Programming\android\Aug26\TurboTransfer\core\src\scheduler\model.rs

//! Channel performance modeling, sample-aware EWMAs, variance tracking, and completion time prediction.

use std::collections::HashMap;
use std::time::Instant;
use super::tracker::{AckSample, ChannelState, ChannelTracker};

/// Record for evaluating scheduler prediction accuracy.
#[derive(Debug, Clone, Copy)]
pub struct PredictionRecord {
    pub chunk_id: u32,
    pub predicted_completion_us: u64,
    pub actual_completion_us: u64,
    pub error_us: u64,
    pub error_pct: f64,
}

/// Statistical performance model for an individual transport channel.
pub struct ChannelPerformanceModel {
    pub channel_name: String,
    pub initial_capacity_mbps: f64,

    // EWMA and variance values
    pub throughput_ewma_mbps: f64,
    pub throughput_variance: f64,
    pub ack_turnaround_ewma_us: f64,
    pub ack_turnaround_variance: f64,
    pub socket_duration_ewma_us: f64,
    pub socket_duration_variance: f64,
    pub estimated_capacity_mbps: f64,

    // Smoothing constants (§10)
    alpha_throughput: f64,
    alpha_ack: f64,
    alpha_socket: f64,

    // Prediction tracking
    pending_predictions: HashMap<u32, (u64, Instant)>,
    prediction_history: Vec<PredictionRecord>,
}

impl ChannelPerformanceModel {
    pub fn new(channel_name: String, initial_capacity_mbps: f64) -> Self {
        Self {
            channel_name,
            initial_capacity_mbps,
            throughput_ewma_mbps: 0.0,
            throughput_variance: 0.0,
            ack_turnaround_ewma_us: 0.0,
            ack_turnaround_variance: 0.0,
            socket_duration_ewma_us: 0.0,
            socket_duration_variance: 0.0,
            estimated_capacity_mbps: initial_capacity_mbps,
            alpha_throughput: 0.20,
            alpha_ack: 0.15,
            alpha_socket: 0.15,
            pending_predictions: HashMap::new(),
            prediction_history: Vec::with_capacity(64),
        }
    }

    /// Ingests a new ACK sample from ChannelTracker and updates EWMAs and variances.
    pub fn update_from_sample(&mut self, sample: &AckSample) {
        let sample_sec = (sample.ack_turnaround_us as f64) / 1_000_000.0;
        let sample_mbps = if sample_sec > 0.00001 {
            ((sample.bytes as f64) / (1024.0 * 1024.0)) / sample_sec
        } else {
            0.0
        };

        // 1. Throughput EWMA & Variance
        if self.throughput_ewma_mbps == 0.0 {
            self.throughput_ewma_mbps = sample_mbps;
            self.throughput_variance = 0.0;
        } else {
            let diff = sample_mbps - self.throughput_ewma_mbps;
            self.throughput_ewma_mbps += self.alpha_throughput * diff;
            self.throughput_variance = (1.0 - self.alpha_throughput) * self.throughput_variance
                + self.alpha_throughput * diff * diff;
        }

        // 2. ACK Turnaround EWMA & Variance
        let ack_us = sample.ack_turnaround_us as f64;
        if self.ack_turnaround_ewma_us == 0.0 {
            self.ack_turnaround_ewma_us = ack_us;
            self.ack_turnaround_variance = 0.0;
        } else {
            let diff = ack_us - self.ack_turnaround_ewma_us;
            self.ack_turnaround_ewma_us += self.alpha_ack * diff;
            self.ack_turnaround_variance = (1.0 - self.alpha_ack) * self.ack_turnaround_variance
                + self.alpha_ack * diff * diff;
        }

        // 3. Socket Send Duration EWMA & Variance
        let sock_us = sample.socket_send_duration_us as f64;
        if self.socket_duration_ewma_us == 0.0 {
            self.socket_duration_ewma_us = sock_us;
            self.socket_duration_variance = 0.0;
        } else {
            let diff = sock_us - self.socket_duration_ewma_us;
            self.socket_duration_ewma_us += self.alpha_socket * diff;
            self.socket_duration_variance = (1.0 - self.alpha_socket) * self.socket_duration_variance
                + self.alpha_socket * diff * diff;
        }

        // 4. Update Capacity Estimate (peak observed rolling vs baseline)
        if sample_mbps > self.estimated_capacity_mbps {
            self.estimated_capacity_mbps = sample_mbps;
        } else {
            self.estimated_capacity_mbps = self.estimated_capacity_mbps * 0.98 + sample_mbps * 0.02;
        }

        // 5. Complete pending prediction if present
        if let Some((pred_us, start_time)) = self.pending_predictions.remove(&sample.chunk_id) {
            let actual_us = start_time.elapsed().as_micros() as u64;
            let error_us = if actual_us > pred_us { actual_us - pred_us } else { pred_us - actual_us };
            let error_pct = if pred_us > 0 { (error_us as f64) / (pred_us as f64) * 100.0 } else { 0.0 };

            if self.prediction_history.len() == 64 {
                self.prediction_history.remove(0);
            }
            self.prediction_history.push(PredictionRecord {
                chunk_id: sample.chunk_id,
                predicted_completion_us: pred_us,
                actual_completion_us: actual_us,
                error_us,
                error_pct,
            });
        }
    }

    /// Predicts completion time in microseconds for a new chunk of given size on this channel.
    pub fn estimate_completion_time_us(&self, tracker: &ChannelTracker, chunk_size: usize) -> u64 {
        let total_work_bytes = tracker.in_flight_bytes + (chunk_size as u64);
        let work_mb = (total_work_bytes as f64) / (1024.0 * 1024.0);

        let effective_mbps = match tracker.state {
            ChannelState::Unknown => self.initial_capacity_mbps.max(1.0),
            ChannelState::WarmingUp => (self.initial_capacity_mbps * 0.5).max(self.throughput_ewma_mbps).max(1.0),
            ChannelState::Active => self.throughput_ewma_mbps.max(1.0),
            ChannelState::Degraded => (self.throughput_ewma_mbps * 0.2).max(0.5),
            ChannelState::Probing => (self.throughput_ewma_mbps * 0.6).max(1.0),
        };

        let est_seconds = work_mb / effective_mbps;
        (est_seconds * 1_000_000.0) as u64
    }

    /// Registers a scheduling prediction before chunk send.
    pub fn record_prediction(&mut self, chunk_id: u32, predicted_us: u64) {
        self.pending_predictions.insert(chunk_id, (predicted_us, Instant::now()));
    }

    /// Returns (P50 error %, P95 error %, MAE in microseconds).
    pub fn prediction_error_stats(&self) -> (f64, f64, f64) {
        if self.prediction_history.is_empty() {
            return (0.0, 0.0, 0.0);
        }

        let mut errors_pct: Vec<f64> = self.prediction_history.iter().map(|r| r.error_pct).collect();
        errors_pct.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let p50_idx = (errors_pct.len() as f64 * 0.50) as usize;
        let p95_idx = (errors_pct.len() as f64 * 0.95).min((errors_pct.len() - 1) as f64) as usize;

        let p50 = errors_pct[p50_idx];
        let p95 = errors_pct[p95_idx];

        let sum_err: u64 = self.prediction_history.iter().map(|r| r.error_us).sum();
        let mae = (sum_err as f64) / (self.prediction_history.len() as f64);

        (p50, p95, mae)
    }
}


d:\MyDocuments\Programming\android\Aug26\TurboTransfer\core\src\scheduler\tracker.rs

//! Ground-truth channel state tracking, in-flight accounting, and utilization.

use std::collections::HashSet;
use std::time::Instant;

/// Lifecycle state of a transport channel (§10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelState {
    /// Channel just registered; no performance data available.
    Unknown,
    /// Channel undergoing initial probing to establish baseline.
    WarmingUp,
    /// Channel actively transferring data with stable performance.
    Active,
    /// Channel has experienced performance collapse or severe latency spike.
    Degraded,
    /// Channel is being sent limited traffic to test if performance has recovered.
    Probing,
}

/// Raw sample recorded upon chunk acknowledgment.
#[derive(Debug, Clone, Copy)]
pub struct AckSample {
    pub chunk_id: u32,
    pub bytes: u64,
    pub ack_turnaround_us: u64,
    pub socket_send_duration_us: u64,
    pub receiver_verify_us: Option<u32>,
    pub ack_residual_us: u64,
    pub timestamp: Instant,
}

/// Ground-truth tracker for an individual transport channel.
pub struct ChannelTracker {
    pub name: String,
    pub state: ChannelState,
    pub session_start: Instant,
    pub in_flight_chunks: HashSet<u32>,
    pub in_flight_bytes: u64,
    pub max_in_flight_observed: usize,
    pub total_chunks_sent: u64,
    pub total_chunks_acked: u64,
    pub total_chunks_nacked: u64,
    pub total_bytes_sent: u64,
    pub total_bytes_acked: u64,
    pub disconnect_count: u32,

    // Utilization tracking (time with >= 1 chunk in-flight)
    busy_time_us: u64,
    last_busy_start: Option<Instant>,

    // Hysteresis counters
    consecutive_severe_samples: usize,
    consecutive_healthy_samples: usize,
    last_degraded_time: Option<Instant>,

    // Recent samples ring buffer (capacity 32)
    recent_samples: Vec<AckSample>,
}

impl ChannelTracker {
    pub fn new(name: String) -> Self {
        let now = Instant::now();
        Self {
            name,
            state: ChannelState::Unknown,
            session_start: now,
            in_flight_chunks: HashSet::new(),
            in_flight_bytes: 0,
            max_in_flight_observed: 0,
            total_chunks_sent: 0,
            total_chunks_acked: 0,
            total_chunks_nacked: 0,
            total_bytes_sent: 0,
            total_bytes_acked: 0,
            disconnect_count: 0,
            busy_time_us: 0,
            last_busy_start: None,
            consecutive_severe_samples: 0,
            consecutive_healthy_samples: 0,
            last_degraded_time: None,
            recent_samples: Vec::with_capacity(32),
        }
    }

    /// Records when a chunk is dispatched on this channel.
    pub fn record_chunk_sent(&mut self, chunk_id: u32, bytes: u64) {
        let now = Instant::now();
        if self.in_flight_chunks.is_empty() && self.last_busy_start.is_none() {
            self.last_busy_start = Some(now);
        }

        self.in_flight_chunks.insert(chunk_id);
        self.in_flight_bytes += bytes;
        self.total_chunks_sent += 1;
        self.total_bytes_sent += bytes;

        let cur_len = self.in_flight_chunks.len();
        if cur_len > self.max_in_flight_observed {
            self.max_in_flight_observed = cur_len;
        }

        if self.state == ChannelState::Unknown {
            self.state = ChannelState::WarmingUp;
        }
    }

    /// Records when an ACK is received for a chunk.
    pub fn record_chunk_ack(
        &mut self,
        chunk_id: u32,
        bytes: u64,
        ack_turnaround_us: u64,
        socket_send_duration_us: u64,
        receiver_verify_us: Option<u32>,
    ) -> Option<AckSample> {
        let was_in_flight = self.in_flight_chunks.remove(&chunk_id);
        if was_in_flight {
            self.in_flight_bytes = self.in_flight_bytes.saturating_sub(bytes);
        }

        let now = Instant::now();
        if self.in_flight_chunks.is_empty() {
            if let Some(start) = self.last_busy_start.take() {
                self.busy_time_us += now.duration_since(start).as_micros() as u64;
            }
        }

        self.total_chunks_acked += 1;
        self.total_bytes_acked += bytes;

        let verify_val = receiver_verify_us.unwrap_or(0) as u64;
        let ack_residual_us = ack_turnaround_us.saturating_sub(verify_val);

        let sample = AckSample {
            chunk_id,
            bytes,
            ack_turnaround_us,
            socket_send_duration_us,
            receiver_verify_us,
            ack_residual_us,
            timestamp: now,
        };

        if self.recent_samples.len() == 32 {
            self.recent_samples.remove(0);
        }
        self.recent_samples.push(sample);

        self.update_state_machine_on_sample(&sample);

        Some(sample)
    }

    /// Records when a NACK is received.
    pub fn record_chunk_nack(&mut self, chunk_id: u32, _reason: &str) {
        if self.in_flight_chunks.remove(&chunk_id) {
            // In-flight chunk removed
        }
        self.total_chunks_nacked += 1;
        self.consecutive_severe_samples += 1;
        if self.consecutive_severe_samples >= 4 && self.state == ChannelState::Active {
            self.state = ChannelState::Degraded;
            self.last_degraded_time = Some(Instant::now());
        }
    }

    /// Records a transport disconnect / drop.
    pub fn record_disconnect(&mut self, _reason: &str) {
        self.disconnect_count += 1;
        self.in_flight_chunks.clear();
        self.in_flight_bytes = 0;
        self.state = ChannelState::Degraded;
        self.last_degraded_time = Some(Instant::now());
    }

    /// Updates the channel state machine with hysteresis rules (§10).
    fn update_state_machine_on_sample(&mut self, sample: &AckSample) {
        let is_severe = sample.socket_send_duration_us > 100_000 || sample.ack_turnaround_us > 1_500_000;
        let is_healthy = sample.socket_send_duration_us < 10_000 && sample.ack_turnaround_us < 500_000;

        if is_severe {
            self.consecutive_severe_samples += 1;
            self.consecutive_healthy_samples = 0;
        } else if is_healthy {
            self.consecutive_healthy_samples += 1;
            self.consecutive_severe_samples = 0;
        } else {
            self.consecutive_severe_samples = 0;
            self.consecutive_healthy_samples = 0;
        }

        match self.state {
            ChannelState::Unknown => {
                self.state = ChannelState::WarmingUp;
            }
            ChannelState::WarmingUp => {
                if self.total_chunks_acked >= 3 {
                    self.state = ChannelState::Active;
                }
            }
            ChannelState::Active => {
                if self.consecutive_severe_samples >= 4 {
                    self.state = ChannelState::Degraded;
                    self.last_degraded_time = Some(Instant::now());
                }
            }
            ChannelState::Degraded => {
                // After 1s cooldown, transition to Probing
                if let Some(t_deg) = self.last_degraded_time {
                    if t_deg.elapsed().as_secs_f64() >= 1.0 {
                        self.state = ChannelState::Probing;
                        self.consecutive_healthy_samples = 0;
                    }
                }
            }
            ChannelState::Probing => {
                if self.consecutive_healthy_samples >= 6 {
                    self.state = ChannelState::Active;
                } else if self.consecutive_severe_samples >= 2 {
                    self.state = ChannelState::Degraded;
                    self.last_degraded_time = Some(Instant::now());
                }
            }
        }
    }

    /// Computes inflight utilization: percentage of session time with >= 1 chunk outstanding.
    pub fn inflight_utilization_pct(&self) -> f64 {
        let now = Instant::now();
        let total_busy = if let Some(start) = self.last_busy_start {
            self.busy_time_us + now.duration_since(start).as_micros() as u64
        } else {
            self.busy_time_us
        };

        let elapsed_us = now.duration_since(self.session_start).as_micros() as u64;
        if elapsed_us == 0 {
            0.0
        } else {
            ((total_busy as f64) / (elapsed_us as f64) * 100.0).clamp(0.0, 100.0)
        }
    }

    /// Returns fraction of recent socket sends taking >10ms (blocking ratio).
    pub fn socket_blocking_ratio(&self) -> f64 {
        if self.recent_samples.is_empty() {
            return 0.0;
        }
        let blocked = self.recent_samples.iter().filter(|s| s.socket_send_duration_us > 10_000).count();
        (blocked as f64) / (self.recent_samples.len() as f64)
    }

    pub fn recent_samples(&self) -> &[AckSample] {
        &self.recent_samples
    }
}


d:\MyDocuments\Programming\android\Aug26\TurboTransfer\core\src\scheduler\window.rs

//! AIMD Concurrency & Congestion Window Controller (§10).

use std::time::{Duration, Instant};
use super::model::ChannelPerformanceModel;
use super::tracker::ChannelTracker;

/// Configuration and controller for channel concurrency window sizing.
#[derive(Debug, Clone)]
pub struct WindowController {
    pub min_window: usize,
    pub max_window: usize,
    pub current_window: usize,

    chunks_since_adjust: usize,
    last_adjust_time: Instant,
    throughput_at_last_adjust: f64,
    increase_cooldown_chunks: usize,
    decrease_cooldown: Duration,
}

impl WindowController {
    pub fn new(min_window: usize, max_window: usize, initial_window: usize) -> Self {
        Self {
            min_window: min_window.max(1),
            max_window: max_window.max(1),
            current_window: initial_window.clamp(min_window, max_window),
            chunks_since_adjust: 0,
            last_adjust_time: Instant::now(),
            throughput_at_last_adjust: 0.0,
            increase_cooldown_chunks: 5,
            decrease_cooldown: Duration::from_millis(500),
        }
    }

    /// Evaluates recent channel performance and updates the allowable in-flight window.
    pub fn evaluate_and_adjust(&mut self, tracker: &ChannelTracker, model: &ChannelPerformanceModel) -> usize {
        self.chunks_since_adjust += 1;

        let cur_throughput = model.throughput_ewma_mbps;
        let throughput_gain_pct = if self.throughput_at_last_adjust > 0.0 {
            ((cur_throughput - self.throughput_at_last_adjust) / self.throughput_at_last_adjust) * 100.0
        } else {
            0.0
        };

        // 1. Check Multiplicative Decrease on Corroborated Backpressure
        let is_sustained_backpressure = model.socket_duration_ewma_us > 100_000.0
            && tracker.socket_blocking_ratio() > 0.30
            && throughput_gain_pct <= 0.0;

        if is_sustained_backpressure && self.last_adjust_time.elapsed() >= self.decrease_cooldown {
            let new_win = (self.current_window / 2).max(self.min_window);
            if new_win != self.current_window {
                self.current_window = new_win;
                self.chunks_since_adjust = 0;
                self.last_adjust_time = Instant::now();
                self.throughput_at_last_adjust = cur_throughput;
                return self.current_window;
            }
        }

        // 2. Check Additive Increase on Healthy Throughput Gain
        let is_healthy_gain = self.chunks_since_adjust >= self.increase_cooldown_chunks
            && self.current_window < self.max_window
            && (throughput_gain_pct >= 3.0 || self.throughput_at_last_adjust == 0.0)
            && tracker.socket_blocking_ratio() < 0.10
            && model.socket_duration_ewma_us < 20_000.0;

        if is_healthy_gain {
            self.current_window += 1;
            self.chunks_since_adjust = 0;
            self.last_adjust_time = Instant::now();
            self.throughput_at_last_adjust = cur_throughput;
        }

        self.current_window
    }
}


d:\MyDocuments\Programming\android\Aug26\TurboTransfer\core\src\scheduler\metrics.rs

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::transport::TransportKind;

/// Sample of bytes transferred at a specific timestamp.
#[derive(Debug, Clone, Copy)]
struct ByteSample {
    timestamp: Instant,
    bytes: u64,
}

/// Rolling 2-second window throughput calculator (§10.1, §12).
pub struct RollingWindowTracker {
    window_duration: Duration,
    samples: VecDeque<ByteSample>,
    total_bytes_in_window: u64,
}

impl RollingWindowTracker {
    pub fn new(window_duration: Duration) -> Self {
        Self {
            window_duration,
            samples: VecDeque::new(),
            total_bytes_in_window: 0,
        }
    }

    /// Records transferred bytes at the current instant.
    pub fn record_bytes(&mut self, bytes: u64) {
        let now = Instant::now();
        self.samples.push_back(ByteSample {
            timestamp: now,
            bytes,
        });
        self.total_bytes_in_window += bytes;
        self.prune(now);
    }

    /// Prunes samples older than `window_duration`.
    fn prune(&mut self, now: Instant) {
        while let Some(front) = self.samples.front() {
            if now.duration_since(front.timestamp) > self.window_duration {
                self.total_bytes_in_window -= front.bytes;
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// Calculates current rolling throughput in Bytes per second.
    pub fn current_throughput_bps(&mut self) -> f64 {
        let now = Instant::now();
        self.prune(now);

        if self.samples.is_empty() {
            return 0.0;
        }

        let elapsed = if self.samples.len() > 1 {
            let earliest = self.samples.front().unwrap().timestamp;
            now.duration_since(earliest).as_secs_f64().max(0.001)
        } else {
            self.window_duration.as_secs_f64()
        };

        (self.total_bytes_in_window as f64) / elapsed
    }
}

/// Consolidated metrics and rolling throughput tracker for multipath transfers.
pub struct ThroughputTracker {
    usb_tracker: Mutex<RollingWindowTracker>,
    wifi_tracker: Mutex<RollingWindowTracker>,
    tcp_tracker: Mutex<RollingWindowTracker>,
    total_usb_bytes: Mutex<u64>,
    total_wifi_bytes: Mutex<u64>,
    total_retries: Mutex<u64>,
    usb_errors: Mutex<u64>,
    wifi_errors: Mutex<u64>,
}

impl Default for ThroughputTracker {
    fn default() -> Self {
        Self::new(Duration::from_secs(2))
    }
}

impl ThroughputTracker {
    pub fn new(window: Duration) -> Self {
        Self {
            usb_tracker: Mutex::new(RollingWindowTracker::new(window)),
            wifi_tracker: Mutex::new(RollingWindowTracker::new(window)),
            tcp_tracker: Mutex::new(RollingWindowTracker::new(window)),
            total_usb_bytes: Mutex::new(0),
            total_wifi_bytes: Mutex::new(0),
            total_retries: Mutex::new(0),
            usb_errors: Mutex::new(0),
            wifi_errors: Mutex::new(0),
        }
    }

    /// Records bytes completed on a specific transport.
    pub fn record_transport_bytes(&self, kind: TransportKind, bytes: u64) {
        match kind {
            TransportKind::Usb => {
                let mut tracker = self.usb_tracker.lock().unwrap();
                tracker.record_bytes(bytes);
                *self.total_usb_bytes.lock().unwrap() += bytes;
            }
            TransportKind::WifiDirect => {
                let mut tracker = self.wifi_tracker.lock().unwrap();
                tracker.record_bytes(bytes);
                *self.total_wifi_bytes.lock().unwrap() += bytes;
            }
            TransportKind::Tcp => {
                let mut tracker = self.tcp_tracker.lock().unwrap();
                tracker.record_bytes(bytes);
            }
        }
    }

    /// Increments retry count for a transport.
    pub fn record_retry(&self, kind: TransportKind) {
        *self.total_retries.lock().unwrap() += 1;
        match kind {
            TransportKind::Usb => *self.usb_errors.lock().unwrap() += 1,
            TransportKind::WifiDirect => *self.wifi_errors.lock().unwrap() += 1,
            _ => {}
        }
    }

    /// Returns snapshot of throughput metrics: (usb_bps, wifi_bps, aggregate_bps).
    pub fn throughput_snapshot(&self) -> (f64, f64, f64) {
        let usb_bps = self.usb_tracker.lock().unwrap().current_throughput_bps();
        let wifi_bps = self.wifi_tracker.lock().unwrap().current_throughput_bps();
        let tcp_bps = self.tcp_tracker.lock().unwrap().current_throughput_bps();
        let agg_bps = usb_bps + wifi_bps + tcp_bps;
        (usb_bps, wifi_bps, agg_bps)
    }

    /// Returns total stats: (usb_bytes, wifi_bytes, retries, usb_errors, wifi_errors).
    pub fn stats_snapshot(&self) -> (u64, u64, u64, u64, u64) {
        (
            *self.total_usb_bytes.lock().unwrap(),
            *self.total_wifi_bytes.lock().unwrap(),
            *self.total_retries.lock().unwrap(),
            *self.usb_errors.lock().unwrap(),
            *self.wifi_errors.lock().unwrap(),
        )
    }
}


d:\MyDocuments\Programming\android\Aug26\TurboTransfer\core\src\scheduler\buffer_pool.rs

use bytes::{Bytes, BytesMut};
use std::sync::{Arc, Mutex};
use tokio::sync::Semaphore;

/// Bounded memory buffer pool ensuring strict memory caps (§10.2).
///
/// Pre-allocates and recycles chunk memory buffers up to `max_buffers`
/// without allocating arbitrary amounts of RAM or repeatedly allocating/freeing on the heap.
#[derive(Clone)]
pub struct BufferPool {
    chunk_size: usize,
    semaphore: Arc<Semaphore>,
    pool: Arc<Mutex<Vec<BytesMut>>>,
}

pub struct PooledBuffer {
    buffer: Option<BytesMut>,
    pool: Option<Arc<Mutex<Vec<BytesMut>>>>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}

impl PooledBuffer {
    /// Returns a mutable slice of exact length `len` to write chunk bytes into.
    pub fn get_mut_slice(&mut self, len: usize) -> &mut [u8] {
        let b = self.buffer.as_mut().expect("PooledBuffer already consumed");
        if b.capacity() < len {
            b.reserve(len - b.capacity());
        }
        if b.len() < len {
            b.resize(len, 0);
        } else {
            b.truncate(len);
        }
        &mut b[..len]
    }

    /// Returns a mutable reference to the underlying buffer.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.buffer.as_mut().expect("PooledBuffer already consumed")
    }

    /// Returns a slice view of the initialized buffer bytes.
    pub fn as_slice(&self) -> &[u8] {
        self.buffer.as_ref().expect("PooledBuffer already consumed")
    }

    /// Freezes the buffer into immutable `Bytes` suitable for frame transport.
    /// Note: This consumes the buffer from the pool recycling path.
    pub fn freeze(mut self) -> Bytes {
        let buf = self.buffer.take().expect("PooledBuffer already consumed");
        buf.freeze()
    }
}

impl Drop for PooledBuffer {
    fn drop(&mut self) {
        if let Some(mut buf) = self.buffer.take() {
            buf.clear();
            if let Some(pool) = &self.pool {
                if let Ok(mut lock) = pool.lock() {
                    lock.push(buf);
                }
            }
        }
    }
}

impl BufferPool {
    /// Creates a new buffer pool with the given capacity (default 8 buffers * chunk_size).
    pub fn new(max_buffers: usize, chunk_size: usize) -> Self {
        let mut preallocated = Vec::with_capacity(max_buffers);
        for _ in 0..max_buffers {
            preallocated.push(BytesMut::with_capacity(chunk_size));
        }

        Self {
            chunk_size,
            semaphore: Arc::new(Semaphore::new(max_buffers)),
            pool: Arc::new(Mutex::new(preallocated)),
        }
    }

    /// Acquires a buffer from the pool, waiting asynchronously if all buffers are currently in flight.
    pub async fn acquire(&self) -> PooledBuffer {
        let permit = self.semaphore.clone().acquire_owned().await.unwrap();
        let buf = {
            let mut lock = self.pool.lock().unwrap();
            lock.pop()
        }
        .unwrap_or_else(|| BytesMut::with_capacity(self.chunk_size));

        PooledBuffer {
            buffer: Some(buf),
            pool: Some(Arc::clone(&self.pool)),
            _permit: permit,
        }
    }

    /// Returns the configured chunk size for this pool.
    pub fn chunk_size(&self) -> usize {
        self.chunk_size
    }

    /// Returns the number of currently available buffer slots.
    pub fn available_slots(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// Returns the number of recycled buffers currently sitting idle in the pool.
    pub fn idle_count(&self) -> usize {
        self.pool.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_buffer_pool_bounded_allocation() {
        let pool = BufferPool::new(2, 1024);
        assert_eq!(pool.available_slots(), 2);
        assert_eq!(pool.idle_count(), 2);

        let buf1 = pool.acquire().await;
        assert_eq!(pool.available_slots(), 1);
        assert_eq!(pool.idle_count(), 1);

        let buf2 = pool.acquire().await;
        assert_eq!(pool.available_slots(), 0);
        assert_eq!(pool.idle_count(), 0);

        drop(buf1);
        assert_eq!(pool.available_slots(), 1);
        assert_eq!(pool.idle_count(), 1);

        drop(buf2);
        assert_eq!(pool.available_slots(), 2);
        assert_eq!(pool.idle_count(), 2);
    }

    #[tokio::test]
    async fn test_buffer_pool_slice_and_recycling() {
        let pool = BufferPool::new(1, 1024);
        {
            let mut buf = pool.acquire().await;
            let slice = buf.get_mut_slice(512);
            slice[0] = 42;
            slice[511] = 99;
            assert_eq!(buf.as_slice().len(), 512);
        }
        // Dropped -> returned to pool
        assert_eq!(pool.available_slots(), 1);
        assert_eq!(pool.idle_count(), 1);

        {
            let mut buf = pool.acquire().await;
            let slice = buf.get_mut_slice(256);
            assert_eq!(slice.len(), 256);
        }
    }
}

Listed directory chunk
Viewed mod.rs:1-105


### [`core/src/chunk/mod.rs`](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/core/src/chunk/mod.rs)

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use uuid::Uuid;

use crate::checksum::compute_xxhash64;

/// Represents a single stateless data-plane chunk (§5.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chunk {
    pub transfer_id: Uuid,
    pub file_id: Uuid,
    /// Sequence index, 0-based
    pub chunk_id: u32,
    pub file_offset: u64,
    pub payload_length: u32,
    /// xxHash64 of payload
    pub checksum: u64,
    pub payload: Bytes,
}

/// Represents the planned offset and size for a chunk without reading file contents into memory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkPlanEntry {
    pub chunk_id: u32,
    pub file_offset: u64,
    pub payload_length: u32,
}

/// Calculates the total number of chunks needed for a given file size and chunk size.
pub fn total_chunks(file_size: u64, chunk_size: u32) -> u32 {
    if file_size == 0 || chunk_size == 0 {
        return 0;
    }
    ((file_size + chunk_size as u64 - 1) / chunk_size as u64) as u32
}

/// Generates the chunk plan for a file given its size and chunk size.
pub fn calculate_chunk_plan(file_size: u64, chunk_size: u32) -> Vec<ChunkPlanEntry> {
    let num_chunks = total_chunks(file_size, chunk_size);
    let mut plan = Vec::with_capacity(num_chunks as usize);

    for i in 0..num_chunks {
        let file_offset = i as u64 * chunk_size as u64;
        let remaining = file_size.saturating_sub(file_offset);
        let payload_length = remaining.min(chunk_size as u64) as u32;

        plan.push(ChunkPlanEntry {
            chunk_id: i,
            file_offset,
            payload_length,
        });
    }

    plan
}

/// Reads a specific chunk payload from disk at the given offset and length.
pub fn read_chunk_at<P: AsRef<Path>>(
    path: P,
    offset: u64,
    length: u32,
) -> Result<Bytes, std::io::Error> {
    let mut file = File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;

    let mut buf = vec![0u8; length as usize];
    file.read_exact(&mut buf)?;
    Ok(Bytes::from(buf))
}

/// Reads a chunk from an open file handle directly into a destination buffer slice at the given offset.
pub fn read_chunk_into_slice(
    file: &mut File,
    offset: u64,
    dest: &mut [u8],
) -> Result<(), std::io::Error> {
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(dest)?;
    Ok(())
}

/// Constructs a full `Chunk` struct from a `ChunkPlanEntry` and file on disk.
pub fn create_chunk<P: AsRef<Path>>(
    transfer_id: Uuid,
    file_id: Uuid,
    entry: &ChunkPlanEntry,
    file_path: P,
) -> Result<Chunk, std::io::Error> {
    let payload = read_chunk_at(file_path, entry.file_offset, entry.payload_length)?;
    let checksum = compute_xxhash64(&payload);

    Ok(Chunk {
        transfer_id,
        file_id,
        chunk_id: entry.chunk_id,
        file_offset: entry.file_offset,
        payload_length: entry.payload_length,
        checksum,
        payload,
    })
}

d:\MyDocuments\Programming\android\Aug26\TurboTransfer\core\src\protocol\messages.rs

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::ProtocolError;

/// Message type code for `Hello` (0x01)
pub const MSG_TYPE_HELLO: u8 = 0x01;
/// Message type code for `TransferOffer` (0x02)
pub const MSG_TYPE_TRANSFER_OFFER: u8 = 0x02;
/// Message type code for `TransferAccept` (0x03)
pub const MSG_TYPE_TRANSFER_ACCEPT: u8 = 0x03;
/// Message type code for `TransferReject` (0x04)
pub const MSG_TYPE_TRANSFER_REJECT: u8 = 0x04;
/// Message type code for `ChunkData` (0x05)
pub const MSG_TYPE_CHUNK_DATA: u8 = 0x05;
/// Message type code for `ChunkAck` (0x06)
pub const MSG_TYPE_CHUNK_ACK: u8 = 0x06;
/// Message type code for `ChunkNack` (0x07)
pub const MSG_TYPE_CHUNK_NACK: u8 = 0x07;
/// Message type code for `Pause` (0x08)
pub const MSG_TYPE_PAUSE: u8 = 0x08;
/// Message type code for `Resume` (0x09)
pub const MSG_TYPE_RESUME: u8 = 0x09;
/// Message type code for `Cancel` (0x0A)
pub const MSG_TYPE_CANCEL: u8 = 0x0A;
/// Message type code for `Complete` (0x0B)
pub const MSG_TYPE_COMPLETE: u8 = 0x0B;
/// Message type code for `Heartbeat` (0x0C)
pub const MSG_TYPE_HEARTBEAT: u8 = 0x0C;
/// Message type code for `BatchChunkAck` (0x0D)
pub const MSG_TYPE_BATCH_CHUNK_ACK: u8 = 0x0D;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloData {
    pub device_id: Uuid,
    pub device_name: String,
    pub protocol_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferOfferData {
    pub transfer_id: Uuid,
    pub file_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub chunk_size: u32,
    pub total_chunks: u32,
    pub checksum_algo: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferAcceptData {
    pub transfer_id: Uuid,
    /// Completed range list stored as inclusive [start, end] chunk-id pairs.
    pub resume_from: Option<Vec<(u32, u32)>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferRejectData {
    pub transfer_id: Uuid,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkDataPayload {
    pub transfer_id: Uuid,
    pub file_id: Uuid,
    pub chunk_id: u32,
    pub file_offset: u64,
    pub payload_length: u32,
    pub checksum: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkAckData {
    pub transfer_id: Uuid,
    pub chunk_id: u32,
    #[serde(default)]
    pub receiver_verify_us: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkNackData {
    pub transfer_id: Uuid,
    pub chunk_id: u32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PauseData {
    pub transfer_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeData {
    pub transfer_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelData {
    pub transfer_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteData {
    pub transfer_id: Uuid,
    pub file_checksum: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatData {
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchChunkAckData {
    pub transfer_id: Uuid,
    pub chunk_ids: Vec<u32>,
    #[serde(default)]
    pub sum_receiver_verify_us: Option<u32>,
}

/// The 13 wire-protocol message types for TurboTransfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    Hello(HelloData),
    TransferOffer(TransferOfferData),
    TransferAccept(TransferAcceptData),
    TransferReject(TransferRejectData),
    ChunkData(ChunkDataPayload),
    ChunkAck(ChunkAckData),
    ChunkNack(ChunkNackData),
    Pause(PauseData),
    Resume(ResumeData),
    Cancel(CancelData),
    Complete(CompleteData),
    Heartbeat(HeartbeatData),
    BatchChunkAck(BatchChunkAckData),
}

impl Message {
    /// Returns the 1-byte message type code for this message.
    pub fn message_type(&self) -> u8 {
        match self {
            Message::Hello(_) => MSG_TYPE_HELLO,
            Message::TransferOffer(_) => MSG_TYPE_TRANSFER_OFFER,
            Message::TransferAccept(_) => MSG_TYPE_TRANSFER_ACCEPT,
            Message::TransferReject(_) => MSG_TYPE_TRANSFER_REJECT,
            Message::ChunkData(_) => MSG_TYPE_CHUNK_DATA,
            Message::ChunkAck(_) => MSG_TYPE_CHUNK_ACK,
            Message::ChunkNack(_) => MSG_TYPE_CHUNK_NACK,
            Message::Pause(_) => MSG_TYPE_PAUSE,
            Message::Resume(_) => MSG_TYPE_RESUME,
            Message::Cancel(_) => MSG_TYPE_CANCEL,
            Message::Complete(_) => MSG_TYPE_COMPLETE,
            Message::Heartbeat(_) => MSG_TYPE_HEARTBEAT,
            Message::BatchChunkAck(_) => MSG_TYPE_BATCH_CHUNK_ACK,
        }
    }

    /// Deserializes a message given its type code and bincode payload.
    pub fn decode_payload(type_code: u8, payload: &[u8]) -> Result<Self, ProtocolError> {
        let msg = match type_code {
            MSG_TYPE_HELLO => {
                let data: HelloData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::Hello(data)
            }
            MSG_TYPE_TRANSFER_OFFER => {
                let data: TransferOfferData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::TransferOffer(data)
            }
            MSG_TYPE_TRANSFER_ACCEPT => {
                let data: TransferAcceptData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::TransferAccept(data)
            }
            MSG_TYPE_TRANSFER_REJECT => {
                let data: TransferRejectData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::TransferReject(data)
            }
            MSG_TYPE_CHUNK_DATA => {
                let data: ChunkDataPayload = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::ChunkData(data)
            }
            MSG_TYPE_CHUNK_ACK => {
                let data: ChunkAckData = if payload.len() == 20 {
                    // Legacy 20-byte payload: transfer_id (16) + chunk_id (4)
                    let t_id = bincode::deserialize(&payload[0..16])
                        .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                    let c_id = bincode::deserialize(&payload[16..20])
                        .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                    ChunkAckData {
                        transfer_id: t_id,
                        chunk_id: c_id,
                        receiver_verify_us: None,
                    }
                } else {
                    bincode::deserialize(payload)
                        .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?
                };
                Message::ChunkAck(data)
            }
            MSG_TYPE_CHUNK_NACK => {
                let data: ChunkNackData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::ChunkNack(data)
            }
            MSG_TYPE_PAUSE => {
                let data: PauseData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::Pause(data)
            }
            MSG_TYPE_RESUME => {
                let data: ResumeData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::Resume(data)
            }
            MSG_TYPE_CANCEL => {
                let data: CancelData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::Cancel(data)
            }
            MSG_TYPE_COMPLETE => {
                let data: CompleteData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::Complete(data)
            }
            MSG_TYPE_HEARTBEAT => {
                let data: HeartbeatData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::Heartbeat(data)
            }
            MSG_TYPE_BATCH_CHUNK_ACK => {
                let data: BatchChunkAckData = match bincode::deserialize(payload) {
                    Ok(d) => d,
                    Err(_) => {
                        #[derive(Deserialize)]
                        struct LegacyBatchChunkAckData {
                            transfer_id: Uuid,
                            chunk_ids: Vec<u32>,
                        }
                        let leg: LegacyBatchChunkAckData = bincode::deserialize(payload)
                            .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                        BatchChunkAckData {
                            transfer_id: leg.transfer_id,
                            chunk_ids: leg.chunk_ids,
                            sum_receiver_verify_us: None,
                        }
                    }
                };
                Message::BatchChunkAck(data)
            }
            other => return Err(ProtocolError::InvalidMessageType(other)),
        };
        Ok(msg)
    }

    /// Serializes the inner message payload to bincode.
    pub fn encode_payload(&self) -> Result<Vec<u8>, ProtocolError> {
        match self {
            Message::Hello(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::TransferOffer(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::TransferAccept(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::TransferReject(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::ChunkData(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::ChunkAck(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::ChunkNack(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::Pause(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::Resume(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::Cancel(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::Complete(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::Heartbeat(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::BatchChunkAck(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
        }
    }
}


d:\MyDocuments\Programming\android\Aug26\TurboTransfer\core\src\transport\mod.rs

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::protocol::{Message, ProtocolError};

pub mod stream;
pub mod tcp;
pub mod usb;
pub mod vectored;
pub mod wifi_direct;

pub use stream::StreamTransport;
pub use tcp::{TcpListenerTransport, TcpTransport};
pub use usb::{AdbDeviceInfo, UsbTransport, UsbTransportConfig};
pub use vectored::write_all_vectored;
pub use wifi_direct::{WifiDirectConfig, WifiDirectTransport};

/// Discriminator for active physical or virtual transport types (§2, §8, §9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransportKind {
    Tcp,
    Usb,
    WifiDirect,
}

impl std::fmt::Display for TransportKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportKind::Tcp => write!(f, "TCP"),
            TransportKind::Usb => write!(f, "USB"),
            TransportKind::WifiDirect => write!(f, "Wi-Fi Direct"),
        }
    }
}

/// Operational connectivity status for a transport channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportStatus {
    Connected,
    Connecting,
    Disconnected,
    Failed,
}

impl std::fmt::Display for TransportStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportStatus::Connected => write!(f, "Connected"),
            TransportStatus::Connecting => write!(f, "Connecting"),
            TransportStatus::Disconnected => write!(f, "Disconnected"),
            TransportStatus::Failed => write!(f, "Failed"),
        }
    }
}

/// Errors surfaced by a transport layer, suitable for scheduler retry logic (§10.3).
#[derive(Error, Debug)]
pub enum TransportError {
    #[error("Transport disconnected: {0}")]
    Disconnected(String),

    #[error("Transport connection timeout: {0}")]
    Timeout(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("Transport error: {0}")]
    Other(String),
}

/// Shared abstraction implemented by all TurboTransfer transport backends (§8, §9).
///
/// Both Wi-Fi Direct and USB transports describe themselves as "a plain TCP socket once
/// the tunnel/group is up, same framing as §6.1." This trait captures that shared contract.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Returns the physical or virtual category of this transport.
    fn kind(&self) -> TransportKind;

    /// Returns the current connectivity status.
    fn status(&self) -> TransportStatus;

    /// Helper indicating whether the transport is currently ready for frame transmission.
    fn is_connected(&self) -> bool {
        self.status() == TransportStatus::Connected
    }

    /// Total payload and framing bytes sent through this transport.
    fn bytes_sent(&self) -> u64;

    /// Total payload and framing bytes received through this transport.
    fn bytes_received(&self) -> u64;

    /// Transmits a protocol frame over the transport channel.
    async fn send_frame(&mut self, msg: &Message) -> Result<(), TransportError>;

    /// Receives the next protocol frame from the transport channel.
    /// Returns `Ok(None)` if the connection reached EOF cleanly.
    async fn receive_frame(&mut self) -> Result<Option<Message>, TransportError>;

    /// Gracefully closes or shuts down the transport channel.
    async fn close(&mut self) -> Result<(), TransportError>;
}

#[async_trait]
impl Transport for Box<dyn Transport> {
    fn kind(&self) -> TransportKind {
        (**self).kind()
    }

    fn status(&self) -> TransportStatus {
        (**self).status()
    }

    fn bytes_sent(&self) -> u64 {
        (**self).bytes_sent()
    }

    fn bytes_received(&self) -> u64 {
        (**self).bytes_received()
    }

    async fn send_frame(&mut self, msg: &Message) -> Result<(), TransportError> {
        (**self).send_frame(msg).await
    }

    async fn receive_frame(&mut self) -> Result<Option<Message>, TransportError> {
        (**self).receive_frame().await
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        (**self).close().await
    }
}


d:\MyDocuments\Programming\android\Aug26\TurboTransfer\core\src\transport\stream.rs

use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf};

use super::{Transport, TransportError, TransportKind, TransportStatus};
use crate::protocol::{encode_frame_parts, FrameReader, Message};

/// Adapter that turns any bidirectional asynchronous byte stream into a `Transport` implementation.
pub struct StreamTransport<S> {
    reader: FrameReader<ReadHalf<S>>,
    writer: WriteHalf<S>,
    kind: TransportKind,
    status: TransportStatus,
    bytes_sent: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
}

impl<S> StreamTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
{
    /// Creates a new `StreamTransport` wrapping the given stream.
    pub fn new(stream: S, kind: TransportKind) -> Self {
        let (read_half, write_half) = tokio::io::split(stream);
        Self {
            reader: FrameReader::new(read_half),
            writer: write_half,
            kind,
            status: TransportStatus::Connected,
            bytes_sent: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
        }
    }
}

#[async_trait]
impl<S> Transport for StreamTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
{
    fn kind(&self) -> TransportKind {
        self.kind
    }

    fn status(&self) -> TransportStatus {
        self.status
    }

    fn bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }

    fn bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }

    async fn send_frame(&mut self, msg: &Message) -> Result<(), TransportError> {
        if self.status != TransportStatus::Connected {
            return Err(TransportError::Disconnected(
                "Cannot send frame on disconnected stream transport".into(),
            ));
        }

        let (header, maybe_payload) = encode_frame_parts(msg)?;
        let payload = maybe_payload.unwrap_or(&[]);
        let frame_len = (header.len() + payload.len()) as u64;

        if let Err(e) = super::vectored::write_all_vectored(&mut self.writer, &header, payload).await {
            self.status = TransportStatus::Disconnected;
            return Err(TransportError::Disconnected(format!(
                "Stream write error: {}",
                e
            )));
        }

        if let Err(e) = self.writer.flush().await {
            self.status = TransportStatus::Disconnected;
            return Err(TransportError::Disconnected(format!(
                "Stream flush error: {}",
                e
            )));
        }

        self.bytes_sent.fetch_add(frame_len, Ordering::Relaxed);
        Ok(())
    }

    async fn receive_frame(&mut self) -> Result<Option<Message>, TransportError> {
        if self.status != TransportStatus::Connected {
            return Err(TransportError::Disconnected(
                "Cannot receive frame on disconnected stream transport".into(),
            ));
        }

        match self.reader.read_frame().await {
            Ok(Some(msg)) => {
                self.bytes_received.fetch_add(64, Ordering::Relaxed);
                Ok(Some(msg))
            }
            Ok(None) => {
                self.status = TransportStatus::Disconnected;
                Ok(None)
            }
            Err(e) => {
                self.status = TransportStatus::Disconnected;
                Err(TransportError::from(e))
            }
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.status = TransportStatus::Disconnected;
        let _ = self.writer.shutdown().await;
        Ok(())
    }
}


d:\MyDocuments\Programming\android\Aug26\TurboTransfer\core\src\transport\vectored.rs

use std::io::{Error, ErrorKind, IoSlice};
use tokio::io::{AsyncWrite, AsyncWriteExt};

/// Asynchronously writes both `header` and `payload` slices to `writer` using OS-level vectored I/O (`writev` / `WSASend`).
///
/// Handles partial writes in a loop until all bytes across both slices have been completely transferred.
pub async fn write_all_vectored<W: AsyncWrite + Unpin>(
    writer: &mut W,
    header: &[u8],
    payload: &[u8],
) -> Result<(), Error> {
    if payload.is_empty() {
        return writer.write_all(header).await;
    }
    if header.is_empty() {
        return writer.write_all(payload).await;
    }

    let mut header_offset = 0;
    let mut payload_offset = 0;

    while header_offset < header.len() || payload_offset < payload.len() {
        if header_offset < header.len() {
            let mut slices = [
                IoSlice::new(&header[header_offset..]),
                IoSlice::new(&payload[payload_offset..]),
            ];
            let n = writer.write_vectored(&mut slices).await?;
            if n == 0 {
                return Err(Error::new(
                    ErrorKind::WriteZero,
                    "failed to write whole frame (zero bytes written)",
                ));
            }
            if header_offset + n <= header.len() {
                header_offset += n;
            } else {
                let header_rem = header.len() - header_offset;
                header_offset = header.len();
                payload_offset += n - header_rem;
            }
        } else {
            // Header is completely written, finish writing remaining payload
            writer.write_all(&payload[payload_offset..]).await?;
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_write_all_vectored() {
        let header = b"HEADER_1234";
        let payload = b"PAYLOAD_DATA_XYZ_56789";

        let mut output = Vec::new();
        write_all_vectored(&mut output, header, payload)
            .await
            .expect("Vectored write must succeed");

        let mut expected = Vec::new();
        expected.extend_from_slice(header);
        expected.extend_from_slice(payload);

        assert_eq!(output, expected);
    }

    #[tokio::test]
    async fn test_write_all_vectored_empty_payload() {
        let header = b"CONTROL_HEADER";
        let payload = b"";

        let mut output = Vec::new();
        write_all_vectored(&mut output, header, payload)
            .await
            .expect("Vectored write must succeed");

        assert_eq!(output, header);
    }
}


d:\MyDocuments\Programming\android\Aug26\TurboTransfer\transport\usb\src\lib.rs

//! USB / ADB tunnel transport implementation (§8).

pub use turbotransfer_core::transport::usb::*;


d:\MyDocuments\Programming\android\Aug26\TurboTransfer\transport\wifi_direct\src\lib.rs

//! Wi-Fi Direct transport implementation.


d:\MyDocuments\Programming\android\Aug26\TurboTransfer\core\src\util\telemetry.rs

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use parking_lot::Mutex;
use uuid::Uuid;

use crate::manifest::TransferRole;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferStage {
    Init,
    Discovery,
    Connection,
    Handshake,
    DiskRead,
    Checksum,
    NetQueue,
    NetSend,
    NetRecv,
    NetAck,
    DiskQueue,
    DiskWrite,
    Finalize,
    Control,
}

impl std::fmt::Display for TransferStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransferStage::Init => write!(f, "INIT"),
            TransferStage::Discovery => write!(f, "DISCOVERY"),
            TransferStage::Connection => write!(f, "CONNECTION"),
            TransferStage::Handshake => write!(f, "HANDSHAKE"),
            TransferStage::DiskRead => write!(f, "DISK_READ"),
            TransferStage::Checksum => write!(f, "CHECKSUM"),
            TransferStage::NetQueue => write!(f, "NET_QUEUE"),
            TransferStage::NetSend => write!(f, "NET_SEND"),
            TransferStage::NetRecv => write!(f, "NET_RECV"),
            TransferStage::NetAck => write!(f, "NET_ACK"),
            TransferStage::DiskQueue => write!(f, "DISK_QUEUE"),
            TransferStage::DiskWrite => write!(f, "DISK_WRITE"),
            TransferStage::Finalize => write!(f, "FINALIZE"),
            TransferStage::Control => write!(f, "CONTROL"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for EventLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventLevel::Debug => write!(f, "DEBUG"),
            EventLevel::Info => write!(f, "INFO"),
            EventLevel::Warn => write!(f, "WARN"),
            EventLevel::Error => write!(f, "ERROR"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferEvent {
    pub timestamp_us: u64,
    pub relative_ms: u64,
    pub stage: TransferStage,
    pub level: EventLevel,
    pub channel: String,
    pub chunk_id: Option<u32>,
    pub duration_us: Option<u64>,
    pub bytes: Option<u64>,
    pub message: String,
    pub details: HashMap<String, String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ChannelMetric {
    pub channel_name: String,
    pub bytes_transferred: u64,
    pub chunks_transferred: u32,
    pub throughput_mbps: f64,
    pub max_in_flight: u32,
    pub avg_socket_write_us: f64,
    pub avg_rtt_ms: f64,
    pub p95_rtt_ms: f64,
    pub nack_count: u64,
    pub disconnect_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BottleneckReport {
    pub transfer_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub role: String,
    pub total_duration_ms: u64,
    pub avg_throughput_mbps: f64,
    pub peak_throughput_mbps: f64,
    pub sender_disk_read_mbps: f64,
    pub sender_disk_read_avg_us: f64,
    pub sender_disk_read_p95_us: f64,
    pub sender_checksum_mbps: f64,
    pub sender_checksum_avg_us: f64,
    pub receiver_disk_write_mbps: f64,
    pub receiver_disk_write_avg_us: f64,
    pub receiver_disk_write_p95_us: f64,
    pub receiver_max_queue_depth: u32,
    pub receiver_finalize_ms: u64,
    pub channels: Vec<ChannelMetric>,
    pub stage_durations_pct: HashMap<String, f64>,
    pub primary_bottleneck: String,
    pub recommendations: Vec<String>,
}

struct TelemetryChannelTracker {
    bytes: u64,
    chunks: u32,
    current_in_flight: u32,
    max_in_flight: u32,
    socket_write_durations_us: Vec<u64>,
    rtt_samples_ms: Vec<f64>,
    nacks: u64,
    disconnects: u64,
}

impl TelemetryChannelTracker {
    fn new() -> Self {
        Self {
            bytes: 0,
            chunks: 0,
            current_in_flight: 0,
            max_in_flight: 0,
            socket_write_durations_us: Vec::with_capacity(1024),
            rtt_samples_ms: Vec::with_capacity(1024),
            nacks: 0,
            disconnects: 0,
        }
    }
}

pub struct TransferTelemetry {
    pub transfer_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub role: TransferRole,
    pub start_time: Instant,
    pub start_utc: DateTime<Utc>,
    pub end_time: Mutex<Option<Instant>>,
    pub events: Mutex<Vec<TransferEvent>>,

    // Sender read & hash stats
    read_durations_us: Mutex<Vec<u64>>,
    read_bytes_total: AtomicU64,
    hash_durations_us: Mutex<Vec<u64>>,
    hash_bytes_total: AtomicU64,

    // Receiver stats
    write_durations_us: Mutex<Vec<u64>>,
    write_bytes_total: AtomicU64,
    recv_verify_durations_us: Mutex<Vec<u64>>,
    max_queue_depth: AtomicU32,
    finalize_duration_ms: AtomicU64,
    duplicate_chunks: AtomicU32,

    // Per-channel stats
    channels: Mutex<HashMap<String, TelemetryChannelTracker>>,
    peak_throughput_mbps: Mutex<f64>,
    throughput_sampler: Mutex<(Instant, u64)>,
}

impl TransferTelemetry {
    pub fn new(transfer_id: Uuid, file_name: String, file_size: u64, role: TransferRole) -> Self {
        let now = Instant::now();
        Self {
            transfer_id,
            file_name,
            file_size,
            role,
            start_time: now,
            start_utc: Utc::now(),
            end_time: Mutex::new(None),
            events: Mutex::new(Vec::with_capacity(2048)),
            read_durations_us: Mutex::new(Vec::with_capacity(1024)),
            read_bytes_total: AtomicU64::new(0),
            hash_durations_us: Mutex::new(Vec::with_capacity(1024)),
            hash_bytes_total: AtomicU64::new(0),
            write_durations_us: Mutex::new(Vec::with_capacity(1024)),
            write_bytes_total: AtomicU64::new(0),
            recv_verify_durations_us: Mutex::new(Vec::with_capacity(1024)),
            max_queue_depth: AtomicU32::new(0),
            finalize_duration_ms: AtomicU64::new(0),
            duplicate_chunks: AtomicU32::new(0),
            channels: Mutex::new(HashMap::new()),
            peak_throughput_mbps: Mutex::new(0.0),
            throughput_sampler: Mutex::new((now, 0)),
        }
    }

    pub fn sample_throughput(&self, added_bytes: u64) {
        let mut sampler = self.throughput_sampler.lock();
        sampler.1 += added_bytes;
        let now = Instant::now();
        let elapsed = now.duration_since(sampler.0).as_secs_f64();
        if elapsed >= 0.25 {
            let mbps = (sampler.1 as f64 / (1024.0 * 1024.0)) / elapsed;
            let mut peak = self.peak_throughput_mbps.lock();
            if mbps > *peak {
                *peak = mbps;
            }
            sampler.0 = now;
            sampler.1 = 0;
        }
    }

    pub fn record_event(
        &self,
        stage: TransferStage,
        level: EventLevel,
        channel: &str,
        chunk_id: Option<u32>,
        duration_us: Option<u64>,
        bytes: Option<u64>,
        message: impl Into<String>,
        details: Option<HashMap<String, String>>,
    ) {
        let now = Instant::now();
        let relative_ms = now.duration_since(self.start_time).as_millis() as u64;
        let timestamp_us = self.start_utc.timestamp_micros() as u64 + (relative_ms * 1000);
        let msg = message.into();

        // Also log to standard Rust log for Logcat / console visibility
        match level {
            EventLevel::Debug => log::debug!("[{}] [{}] [{}] {}", self.transfer_id, stage, channel, msg),
            EventLevel::Info => log::info!("[{}] [{}] [{}] {}", self.transfer_id, stage, channel, msg),
            EventLevel::Warn => log::warn!("[{}] [{}] [{}] {}", self.transfer_id, stage, channel, msg),
            EventLevel::Error => log::error!("[{}] [{}] [{}] {}", self.transfer_id, stage, channel, msg),
        }

        let event = TransferEvent {
            timestamp_us,
            relative_ms,
            stage,
            level,
            channel: channel.to_string(),
            chunk_id,
            duration_us,
            bytes,
            message: msg,
            details: details.unwrap_or_default(),
        };

        let mut events = self.events.lock();
        if events.len() < 50_000 {
            events.push(event);
        }
    }

    pub fn record_chunk_read(&self, chunk_id: u32, bytes: u64, read_us: u64, hash_us: u64) {
        self.read_bytes_total.fetch_add(bytes, Ordering::Relaxed);
        self.hash_bytes_total.fetch_add(bytes, Ordering::Relaxed);

        {
            let mut reads = self.read_durations_us.lock();
            if reads.len() < 100_000 {
                reads.push(read_us);
            }
        }
        {
            let mut hashes = self.hash_durations_us.lock();
            if hashes.len() < 100_000 {
                hashes.push(hash_us);
            }
        }

        if chunk_id % 32 == 0 || chunk_id == 0 {
            self.record_event(
                TransferStage::DiskRead,
                EventLevel::Debug,
                "DiskReader",
                Some(chunk_id),
                Some(read_us),
                Some(bytes),
                format!("Read chunk #{} ({} bytes) in {} us (hash: {} us)", chunk_id, bytes, read_us, hash_us),
                None,
            );
        }
    }

    pub fn record_chunk_sent(&self, channel_name: &str, chunk_id: u32, bytes: u64, socket_write_us: u64) {
        let mut channels = self.channels.lock();
        let tracker = channels.entry(channel_name.to_string()).or_insert_with(TelemetryChannelTracker::new);
        tracker.bytes += bytes;
        tracker.chunks += 1;
        tracker.current_in_flight += 1;
        if tracker.current_in_flight > tracker.max_in_flight {
            tracker.max_in_flight = tracker.current_in_flight;
        }
        if tracker.socket_write_durations_us.len() < 50_000 {
            tracker.socket_write_durations_us.push(socket_write_us);
        }

        if chunk_id % 32 == 0 || chunk_id == 0 {
            drop(channels);
            self.record_event(
                TransferStage::NetSend,
                EventLevel::Debug,
                channel_name,
                Some(chunk_id),
                Some(socket_write_us),
                Some(bytes),
                format!("Sent chunk #{} on {} in {} us", chunk_id, channel_name, socket_write_us),
                None,
            );
        }
    }

    pub fn record_chunk_ack(&self, channel_name: &str, chunk_id: u32, ack_latency_ms: f64, bytes: u64) {
        {
            let mut channels = self.channels.lock();
            let tracker = channels.entry(channel_name.to_string()).or_insert_with(TelemetryChannelTracker::new);
            tracker.current_in_flight = tracker.current_in_flight.saturating_sub(1);
            if tracker.rtt_samples_ms.len() < 50_000 {
                tracker.rtt_samples_ms.push(ack_latency_ms);
            }
        }
        self.sample_throughput(bytes);

        if chunk_id % 32 == 0 || chunk_id == 0 {
            self.record_event(
                TransferStage::NetAck,
                EventLevel::Debug,
                channel_name,
                Some(chunk_id),
                Some((ack_latency_ms * 1000.0) as u64),
                Some(bytes),
                format!("Received ACK for chunk #{} on {} (Chunk ACK Latency: {:.2} ms)", chunk_id, channel_name, ack_latency_ms),
                None,
            );
        }
    }

    pub fn record_chunk_nack(&self, channel_name: &str, chunk_id: u32, reason: &str) {
        {
            let mut channels = self.channels.lock();
            let tracker = channels.entry(channel_name.to_string()).or_insert_with(TelemetryChannelTracker::new);
            tracker.nacks += 1;
        }
        self.record_event(
            TransferStage::NetAck,
            EventLevel::Warn,
            channel_name,
            Some(chunk_id),
            None,
            None,
            format!("Received NACK for chunk #{}: {}", chunk_id, reason),
            None,
        );
    }

    pub fn record_channel_disconnect(&self, channel_name: &str, reason: &str) {
        {
            let mut channels = self.channels.lock();
            let tracker = channels.entry(channel_name.to_string()).or_insert_with(TelemetryChannelTracker::new);
            tracker.disconnects += 1;
        }
        self.record_event(
            TransferStage::Connection,
            EventLevel::Warn,
            channel_name,
            None,
            None,
            None,
            format!("Transport channel disconnected: {}", reason),
            None,
        );
    }

    pub fn record_chunk_recv(&self, channel_name: &str, chunk_id: u32, bytes: u64, recv_us: u64, verify_us: u64) {
        {
            let mut verify_list = self.recv_verify_durations_us.lock();
            if verify_list.len() < 50_000 {
                verify_list.push(verify_us);
            }
        }
        let mut channels = self.channels.lock();
        let tracker = channels.entry(channel_name.to_string()).or_insert_with(TelemetryChannelTracker::new);
        tracker.bytes += bytes;
        tracker.chunks += 1;

        if chunk_id % 32 == 0 || chunk_id == 0 {
            drop(channels);
            self.record_event(
                TransferStage::NetRecv,
                EventLevel::Debug,
                channel_name,
                Some(chunk_id),
                Some(recv_us),
                Some(bytes),
                format!("Received chunk #{} ({} bytes) in {} us (verify: {} us)", chunk_id, bytes, recv_us, verify_us),
                None,
            );
        }
    }

    pub fn record_duplicate_chunk(&self, chunk_id: u32) {
        self.duplicate_chunks.fetch_add(1, Ordering::Relaxed);
        self.record_event(
            TransferStage::NetRecv,
            EventLevel::Debug,
            "Receiver",
            Some(chunk_id),
            None,
            None,
            format!("Dropped duplicate chunk #{}", chunk_id),
            None,
        );
    }

    pub fn record_disk_write(&self, chunk_id: u32, bytes: u64, write_us: u64, queue_depth: u32) {
        self.write_bytes_total.fetch_add(bytes, Ordering::Relaxed);
        let mut cur_max = self.max_queue_depth.load(Ordering::Relaxed);
        while queue_depth > cur_max {
            match self.max_queue_depth.compare_exchange_weak(cur_max, queue_depth, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(actual) => cur_max = actual,
            }
        }

        {
            let mut writes = self.write_durations_us.lock();
            if writes.len() < 100_000 {
                writes.push(write_us);
            }
        }

        if chunk_id % 32 == 0 || chunk_id == 0 || queue_depth > 64 {
            let lvl = if queue_depth > 64 { EventLevel::Warn } else { EventLevel::Debug };
            self.record_event(
                TransferStage::DiskWrite,
                lvl,
                "DiskWriter",
                Some(chunk_id),
                Some(write_us),
                Some(bytes),
                format!("Wrote chunk #{} ({} bytes) to disk in {} us [queue depth: {}]", chunk_id, bytes, write_us, queue_depth),
                None,
            );
        }
    }

    pub fn record_finalize(&self, duration_ms: u64, crc_instant: bool) {
        self.finalize_duration_ms.store(duration_ms, Ordering::Relaxed);
        self.record_event(
            TransferStage::Finalize,
            EventLevel::Info,
            "Finalizer",
            None,
            Some(duration_ms * 1000),
            None,
            format!("Transfer finalized in {} ms (in-flight CRC combine: {})", duration_ms, crc_instant),
            None,
        );
    }

    pub fn update_peak_throughput(&self, mbps: f64) {
        let mut peak = self.peak_throughput_mbps.lock();
        if mbps > *peak {
            *peak = mbps;
        }
    }

    pub fn mark_completed(&self) {
        let now = Instant::now();
        let mut end = self.end_time.lock();
        if end.is_none() {
            *end = Some(now);
        }
        let elapsed_ms = now.duration_since(self.start_time).as_millis() as u64;
        let avg_mbps = if elapsed_ms > 0 {
            (self.file_size as f64 / (1024.0 * 1024.0)) / (elapsed_ms as f64 / 1000.0)
        } else {
            0.0
        };

        self.record_event(
            TransferStage::Finalize,
            EventLevel::Info,
            "Session",
            None,
            Some(elapsed_ms * 1000),
            Some(self.file_size),
            format!("Transfer completed: {} bytes in {} ms ({:.2} MB/s avg)", self.file_size, elapsed_ms, avg_mbps),
            None,
        );
    }

    pub fn mark_failed(&self, error: &str) {
        let now = Instant::now();
        let mut end = self.end_time.lock();
        if end.is_none() {
            *end = Some(now);
        }
        let elapsed_ms = now.duration_since(self.start_time).as_millis() as u64;

        self.record_event(
            TransferStage::Finalize,
            EventLevel::Error,
            "Session",
            None,
            Some(elapsed_ms * 1000),
            None,
            format!("Transfer failed after {} ms: {}", elapsed_ms, error),
            None,
        );
    }

    pub fn get_events(&self, max_count: Option<usize>) -> Vec<TransferEvent> {
        let events = self.events.lock();
        match max_count {
            Some(n) if events.len() > n => events[events.len() - n..].to_vec(),
            _ => events.clone(),
        }
    }

    pub fn generate_report(&self) -> BottleneckReport {
        self.generate_bottleneck_report()
    }

    pub fn generate_bottleneck_report(&self) -> BottleneckReport {
        let end_instant = self.end_time.lock().unwrap_or_else(Instant::now);
        let total_duration_ms = end_instant.duration_since(self.start_time).as_millis().max(1) as u64;
        let total_secs = total_duration_ms as f64 / 1000.0;

        let total_bytes = self.file_size;
        let avg_throughput_mbps = (total_bytes as f64 / (1024.0 * 1024.0)) / total_secs;
        let peak_throughput_mbps = (*self.peak_throughput_mbps.lock()).max(avg_throughput_mbps);

        // Sender Disk Read stats
        let read_list = self.read_durations_us.lock().clone();
        let (read_avg_us, read_p95_us) = calc_avg_p95(&read_list);
        let read_bytes = self.read_bytes_total.load(Ordering::Relaxed);
        let read_total_secs = (read_list.iter().sum::<u64>() as f64) / 1_000_000.0;
        let sender_disk_read_mbps = if read_total_secs > 0.0 {
            (read_bytes as f64 / (1024.0 * 1024.0)) / read_total_secs
        } else {
            0.0
        };

        // Sender Checksum stats
        let hash_list = self.hash_durations_us.lock().clone();
        let (hash_avg_us, _) = calc_avg_p95(&hash_list);
        let hash_bytes = self.hash_bytes_total.load(Ordering::Relaxed);
        let hash_total_secs = (hash_list.iter().sum::<u64>() as f64) / 1_000_000.0;
        let sender_checksum_mbps = if hash_total_secs > 0.0 {
            (hash_bytes as f64 / (1024.0 * 1024.0)) / hash_total_secs
        } else {
            0.0
        };

        // Receiver Disk Write stats
        let write_list = self.write_durations_us.lock().clone();
        let (write_avg_us, write_p95_us) = calc_avg_p95(&write_list);
        let write_bytes = self.write_bytes_total.load(Ordering::Relaxed);
        let write_total_secs = (write_list.iter().sum::<u64>() as f64) / 1_000_000.0;
        let receiver_disk_write_mbps = if write_total_secs > 0.0 {
            (write_bytes as f64 / (1024.0 * 1024.0)) / write_total_secs
        } else {
            0.0
        };
        let receiver_max_queue_depth = self.max_queue_depth.load(Ordering::Relaxed);
        let receiver_finalize_ms = self.finalize_duration_ms.load(Ordering::Relaxed);

        // Channels
        let mut channel_metrics = Vec::new();
        let channels_guard = self.channels.lock();
        for (name, tracker) in channels_guard.iter() {
            let (write_avg, _) = calc_avg_p95(&tracker.socket_write_durations_us);
            let (rtt_avg, rtt_p95) = calc_avg_p95_f64(&tracker.rtt_samples_ms);
            let ch_throughput = if total_secs > 0.0 {
                (tracker.bytes as f64 / (1024.0 * 1024.0)) / total_secs
            } else {
                0.0
            };
            channel_metrics.push(ChannelMetric {
                channel_name: name.clone(),
                bytes_transferred: tracker.bytes,
                chunks_transferred: tracker.chunks,
                throughput_mbps: ch_throughput,
                max_in_flight: tracker.max_in_flight,
                avg_socket_write_us: write_avg,
                avg_rtt_ms: rtt_avg,
                p95_rtt_ms: rtt_p95,
                nack_count: tracker.nacks,
                disconnect_count: tracker.disconnects,
            });
        }
        drop(channels_guard);

        // Stage duration breakdown percentages
        let mut stage_durations_pct = HashMap::new();
        let total_active_us = (total_duration_ms * 1000) as f64;
        let read_sum_us: u64 = read_list.iter().sum();
        let write_sum_us: u64 = write_list.iter().sum();
        let hash_sum_us: u64 = hash_list.iter().sum();
        let finalize_us = receiver_finalize_ms * 1000;

        if total_active_us > 0.0 {
            stage_durations_pct.insert("DiskRead".to_string(), (read_sum_us as f64 / total_active_us) * 100.0);
            stage_durations_pct.insert("CpuChecksum".to_string(), (hash_sum_us as f64 / total_active_us) * 100.0);
            stage_durations_pct.insert("DiskWrite".to_string(), (write_sum_us as f64 / total_active_us) * 100.0);
            stage_durations_pct.insert("Finalize".to_string(), (finalize_us as f64 / total_active_us) * 100.0);
        }

        // Bottleneck Diagnosis
        let mut recommendations = Vec::new();
        let primary_bottleneck;

        if self.role == TransferRole::Receiver && (receiver_max_queue_depth >= 96 || write_p95_us > 50_000.0) {
            primary_bottleneck = "RECEIVER_DISK_WRITE_BOTTLENECK".to_string();
            recommendations.push(format!(
                "Receiver storage write latency averaged {:.1} ms (P95: {:.1} ms) and disk queue reached {}/128 chunks. The receiving drive/flash storage is the primary constraint.",
                write_avg_us / 1000.0, write_p95_us / 1000.0, receiver_max_queue_depth
            ));
        } else if self.role == TransferRole::Sender && read_p95_us > 40_000.0 && sender_disk_read_mbps < (avg_throughput_mbps * 1.1) {
            primary_bottleneck = "SENDER_DISK_READ_BOTTLENECK".to_string();
            recommendations.push(format!(
                "Sender disk read throughput ({:.1} MB/s) was slower than network capacity. Reading chunks took an average of {:.1} ms per chunk.",
                sender_disk_read_mbps, read_avg_us / 1000.0
            ));
        } else {
            let total_disconnects: u64 = channel_metrics.iter().map(|c| c.disconnect_count).sum();
            let total_nacks: u64 = channel_metrics.iter().map(|c| c.nack_count).sum();

            if total_disconnects > 0 || total_nacks > 3 {
                primary_bottleneck = "NETWORK_PACKET_CORRUPTION_OR_DROP".to_string();
                recommendations.push(format!(
                    "Network packet corruption or disconnect detected (NACKs: {}, Disconnects: {}). Check physical USB connection or 5GHz Wi-Fi line-of-sight.",
                    total_nacks, total_disconnects
                ));
            } else if hash_avg_us > 25_000.0 && sender_checksum_mbps < 200.0 {
                primary_bottleneck = "CPU_CHECKSUM_BOTTLENECK".to_string();
                recommendations.push(format!(
                    "xxHash64 / CRC32C computation took {:.1} ms per chunk ({:.1} MB/s). CPU computation throttled the transfer pipeline.",
                    hash_avg_us / 1000.0, sender_checksum_mbps
                ));
            } else if avg_throughput_mbps >= 150.0 {
                primary_bottleneck = "BALANCED_WIRE_SPEED".to_string();
                recommendations.push(format!(
                    "Optimal wire-speed performance achieved ({:.1} MB/s average, peak {:.1} MB/s). Pipeline stages operated without stalls.",
                    avg_throughput_mbps, peak_throughput_mbps
                ));
            } else {
                primary_bottleneck = "NETWORK_BANDWIDTH_LIMIT".to_string();
                recommendations.push(format!(
                    "Transfer was network bandwidth-limited at {:.1} MB/s across {} active channel(s) (Peak: {:.1} MB/s). Disk I/O and CPU checksums operated faster than the physical wireless link.",
                    avg_throughput_mbps, channel_metrics.len(), peak_throughput_mbps
                ));
            }
        }

        BottleneckReport {
            transfer_id: self.transfer_id.to_string(),
            file_name: self.file_name.clone(),
            file_size: self.file_size,
            role: format!("{:?}", self.role),
            total_duration_ms,
            avg_throughput_mbps,
            peak_throughput_mbps,
            sender_disk_read_mbps,
            sender_disk_read_avg_us: read_avg_us,
            sender_disk_read_p95_us: read_p95_us,
            sender_checksum_mbps,
            sender_checksum_avg_us: hash_avg_us,
            receiver_disk_write_mbps,
            receiver_disk_write_avg_us: write_avg_us,
            receiver_disk_write_p95_us: write_p95_us,
            receiver_max_queue_depth,
            receiver_finalize_ms,
            channels: channel_metrics,
            stage_durations_pct,
            primary_bottleneck,
            recommendations,
        }
    }

    /// Exports structured `.json` and human-readable `.log` files to `<data_dir>/logs/`.
    pub fn export_log_files(&self, data_dir: &Path) -> Result<(PathBuf, PathBuf), std::io::Error> {
        let logs_dir = data_dir.join("logs");
        std::fs::create_dir_all(&logs_dir)?;

        let id_str = self.transfer_id.to_string();
        let json_path = logs_dir.join(format!("{}.json", id_str));
        let log_path = logs_dir.join(format!("{}.log", id_str));

        let report = self.generate_bottleneck_report();
        let events = self.get_events(None);

        #[derive(Serialize)]
        struct FullExport<'a> {
            report: &'a BottleneckReport,
            events: &'a [TransferEvent],
        }

        // Write JSON file
        let full_export = FullExport {
            report: &report,
            events: &events,
        };
        let json_str = serde_json::to_string_pretty(&full_export)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&json_path, json_str)?;

        // Write Human-Readable .log file
        let mut log_content = String::new();
        log_content.push_str("================================================================================\n");
        log_content.push_str(&format!(" TurboTransfer Session Log: {}\n", id_str));
        log_content.push_str("================================================================================\n");
        log_content.push_str(&format!("File Name       : {}\n", self.file_name));
        log_content.push_str(&format!("File Size       : {} bytes ({:.2} MB)\n", self.file_size, self.file_size as f64 / (1024.0 * 1024.0)));
        log_content.push_str(&format!("Role            : {:?}\n", self.role));
        log_content.push_str(&format!("Start Time (UTC): {}\n", self.start_utc.to_rfc3339()));
        log_content.push_str(&format!("Duration        : {} ms ({:.2} s)\n", report.total_duration_ms, report.total_duration_ms as f64 / 1000.0));
        log_content.push_str(&format!("Average Speed   : {:.2} MB/s\n", report.avg_throughput_mbps));
        log_content.push_str(&format!("Peak Speed      : {:.2} MB/s\n", report.peak_throughput_mbps));
        log_content.push_str(&format!("Bottleneck      : {}\n", report.primary_bottleneck));
        for rec in &report.recommendations {
            log_content.push_str(&format!("  * {}\n", rec));
        }
        log_content.push_str("\n--- Channels Breakdown ---\n");
        for ch in &report.channels {
            log_content.push_str(&format!(
                "  [{}] Chunks: {}, Bytes: {} ({:.2} MB/s), Socket Write: {:.1} us, Avg ACK Latency: {:.1} ms (P95: {:.1} ms), Max In-Flight: {}, NACKs: {}, Disconnects: {}\n",
                ch.channel_name, ch.chunks_transferred, ch.bytes_transferred, ch.throughput_mbps, ch.avg_socket_write_us, ch.avg_rtt_ms, ch.p95_rtt_ms, ch.max_in_flight, ch.nack_count, ch.disconnect_count
            ));
        }
        log_content.push_str("\n--- Stage Latencies ---\n");
        if self.role == TransferRole::Sender {
            log_content.push_str(&format!("  Sender Disk Read    : {:.1} MB/s (avg {:.1} us, p95 {:.1} us)\n", report.sender_disk_read_mbps, report.sender_disk_read_avg_us, report.sender_disk_read_p95_us));
            log_content.push_str(&format!("  Sender CPU Checksum : {:.1} MB/s (avg {:.1} us)\n", report.sender_checksum_mbps, report.sender_checksum_avg_us));
            log_content.push_str("  Receiver Disk Write : N/A (Sender Role Session)\n");
        } else {
            log_content.push_str("  Sender Disk Read    : N/A (Receiver Role Session)\n");
            log_content.push_str("  Sender CPU Checksum : N/A (Receiver Role Session)\n");
            log_content.push_str(&format!("  Receiver Disk Write : {:.1} MB/s (avg {:.1} us, p95 {:.1} us, max queue {})\n", report.receiver_disk_write_mbps, report.receiver_disk_write_avg_us, report.receiver_disk_write_p95_us, report.receiver_max_queue_depth));
        }
        log_content.push_str(&format!("  Receiver Finalize   : {} ms\n", report.receiver_finalize_ms));

        log_content.push_str("\n================================================================================\n");
        log_content.push_str(" Detailed Event Timeline\n");
        log_content.push_str("================================================================================\n");
        log_content.push_str(" REL_MS | LEVEL | STAGE       | CHANNEL        | MSG\n");
        log_content.push_str("--------------------------------------------------------------------------------\n");

        for ev in events {
            log_content.push_str(&format!(
                "{:>7} | {:<5} | {:<11} | {:<14} | {}\n",
                ev.relative_ms, ev.level, ev.stage, ev.channel, ev.message
            ));
        }

        std::fs::write(&log_path, log_content)?;

        #[cfg(target_os = "android")]
        {
            let public_dirs = [
                std::path::PathBuf::from("/storage/emulated/0/Download/TurboTransfer/logs"),
                std::path::PathBuf::from("/sdcard/Download/TurboTransfer/logs"),
            ];
            for pdir in &public_dirs {
                if std::fs::create_dir_all(pdir).is_ok() {
                    let _ = std::fs::copy(&json_path, pdir.join(format!("{}.json", id_str)));
                    let _ = std::fs::copy(&log_path, pdir.join(format!("{}.log", id_str)));
                    break;
                }
            }
        }

        Ok((json_path, log_path))
    }
}

fn calc_avg_p95(values: &[u64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let sum: u64 = values.iter().sum();
    let avg = sum as f64 / values.len() as f64;

    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let p95_idx = ((sorted.len() as f64 * 0.95).floor() as usize).min(sorted.len() - 1);
    let p95 = sorted[p95_idx] as f64;

    (avg, p95)
}

fn calc_avg_p95_f64(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let sum: f64 = values.iter().sum();
    let avg = sum / values.len() as f64;

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95_idx = ((sorted.len() as f64 * 0.95).floor() as usize).min(sorted.len() - 1);
    let p95 = sorted[p95_idx];

    (avg, p95)
}

// ---------------------------------------------------------------------------
// Global Telemetry Registry
// ---------------------------------------------------------------------------

struct GlobalTelemetryRegistry {
    sessions: Mutex<HashMap<Uuid, Arc<TransferTelemetry>>>,
}

static TELEMETRY_REGISTRY: std::sync::OnceLock<GlobalTelemetryRegistry> = std::sync::OnceLock::new();

fn get_telemetry_registry() -> &'static GlobalTelemetryRegistry {
    TELEMETRY_REGISTRY.get_or_init(|| GlobalTelemetryRegistry {
        sessions: Mutex::new(HashMap::new()),
    })
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

pub fn export_and_clean_telemetry(transfer_id: Uuid, data_dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let reg = get_telemetry_registry();
    let telemetry = {
        let mut map = reg.sessions.lock();
        map.remove(&transfer_id)
    }?;

    match telemetry.export_log_files(data_dir) {
        Ok(paths) => {
            log::info!("[Telemetry] Exported transfer {} logs to {:?}", transfer_id, paths);
            Some(paths)
        }
        Err(e) => {
            log::error!("[Telemetry] Failed to export transfer {} logs: {}", transfer_id, e);
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
                fn __android_log_write(prio: i32, tag: *const std::os::raw::c_char, text: *const std::os::raw::c_char) -> i32;
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

static LOGGER_INIT: std::sync::Once = std::sync::Once::new();

pub fn init_telemetry_logger() {
    LOGGER_INIT.call_once(|| {
        let logger = Box::leak(Box::new(TurboLogger));
        let _ = log::set_logger(logger);
        log::set_max_level(log::LevelFilter::Debug);
        log::info!("TurboTransfer logging and structured telemetry initialized");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_telemetry_event_recording_and_metrics() {
        let transfer_id = Uuid::new_v4();
        let telemetry = TransferTelemetry::new(transfer_id, "test_movie.mp4", 100 * 1024 * 1024, TransferRole::Sender);

        telemetry.record_event(TransferStage::Handshake, EventLevel::Info, "Control", None, None, None, "Handshake started", None);
        telemetry.record_chunk_read(0, 2 * 1024 * 1024, 2500, 1200);
        telemetry.record_chunk_sent("Wi-Fi", 0, 2 * 1024 * 1024, 3000);
        telemetry.record_chunk_ack("Wi-Fi", 0, 4.5);
        telemetry.record_finalize(12, true);
        telemetry.mark_completed();

        let report = telemetry.generate_report();
        assert_eq!(report.transfer_id, transfer_id);
        assert_eq!(report.file_name, "test_movie.mp4");
        assert_eq!(report.channels.len(), 1);
        assert_eq!(report.channels[0].channel_name, "Wi-Fi");
        assert_eq!(report.channels[0].chunks_transferred, 1);
        assert_eq!(report.channels[0].bytes_transferred, 2 * 1024 * 1024);
        assert!(report.sender_disk_read_avg_us > 0.0);
        assert!(report.sender_checksum_avg_us > 0.0);
    }

    #[test]
    fn test_receiver_disk_write_bottleneck_diagnosis() {
        let transfer_id = Uuid::new_v4();
        let telemetry = TransferTelemetry::new(transfer_id, "large.iso", 100 * 1024 * 1024, TransferRole::Receiver);

        // Simulate high disk write latency and deep queue
        for cid in 0..20 {
            telemetry.record_chunk_recv("Wi-Fi", cid, 2 * 1024 * 1024, 1500, 500);
            telemetry.record_disk_write(cid, 2 * 1024 * 1024, 85_000, 48); // 85ms write latency per chunk
        }
        telemetry.mark_completed();

        let report = telemetry.generate_report();
        assert_eq!(report.primary_bottleneck, "RECEIVER_DISK_WRITE_BOTTLENECK");
        assert!(report.recommendations.iter().any(|r| r.contains("flash write")));
    }

    #[test]
    fn test_sender_disk_read_bottleneck_diagnosis() {
        let transfer_id = Uuid::new_v4();
        let telemetry = TransferTelemetry::new(transfer_id, "data.bin", 50 * 1024 * 1024, TransferRole::Sender);

        // Simulate slow disk read (e.g. 5 MB/s) but fast network
        for cid in 0..10 {
            telemetry.record_chunk_read(cid, 2 * 1024 * 1024, 150_000, 1000); // 150ms read per chunk
            telemetry.record_chunk_sent("Wi-Fi", cid, 2 * 1024 * 1024, 2000);
            telemetry.record_chunk_ack("Wi-Fi", cid, 3.0);
        }
        telemetry.mark_completed();

        let report = telemetry.generate_report();
        assert_eq!(report.primary_bottleneck, "SENDER_DISK_READ_BOTTLENECK");
    }

    #[test]
    fn test_network_latency_jitter_diagnosis() {
        let transfer_id = Uuid::new_v4();
        let telemetry = TransferTelemetry::new(transfer_id, "network_test.bin", 50 * 1024 * 1024, TransferRole::Sender);

        // Fast disk read and hash, but huge RTT (120ms) and NACKs
        for cid in 0..10 {
            telemetry.record_chunk_read(cid, 2 * 1024 * 1024, 1000, 500);
            telemetry.record_chunk_sent("Wi-Fi", cid, 2 * 1024 * 1024, 2000);
            telemetry.record_chunk_ack("Wi-Fi", cid, 120.0);
        }
        telemetry.record_chunk_nack("Wi-Fi", 5, "packet drop");
        telemetry.mark_completed();

        let report = telemetry.generate_report();
        assert_eq!(report.primary_bottleneck, "NETWORK_LATENCY_JITTER");
    }

    #[test]
    fn test_export_log_files_to_disk() {
        let dir = tempdir().expect("tempdir");
        let transfer_id = Uuid::new_v4();
        let telemetry = TransferTelemetry::new(transfer_id, "file.zip", 1024 * 1024, TransferRole::Sender);

        telemetry.record_event(TransferStage::Init, EventLevel::Info, "Main", None, None, None, "Transfer session initialized", None);
        telemetry.record_chunk_read(0, 1024 * 1024, 500, 300);
        telemetry.record_chunk_sent("Wi-Fi", 0, 1024 * 1024, 1000);
        telemetry.record_chunk_ack("Wi-Fi", 0, 2.5);
        telemetry.mark_completed();

        let (json_path, log_path) = telemetry.export_log_files(dir.path()).expect("export");
        assert!(json_path.exists());
        assert!(log_path.exists());

        let json_str = std::fs::read_to_string(&json_path).expect("read json");
        assert!(json_str.contains("file.zip"));
        assert!(json_str.contains(&transfer_id.to_string()));

        let log_str = std::fs::read_to_string(&log_path).expect("read log");
        assert!(log_str.contains("Transfer session initialized"));
        assert!(log_str.contains("BOTTLENECK DIAGNOSTIC SUMMARY"));
    }
}

d:\MyDocuments\Programming\android\Aug26\TurboTransfer\core\src\util\storage.rs

use std::fs::File;
use std::path::Path;

/// Issues OS-level kernel readahead advisories to optimize sequential reading of large files into page cache.
///
/// On Linux/Android, issues `posix_fadvise(..., POSIX_FADV_SEQUENTIAL | POSIX_FADV_WILLNEED)` to trigger
/// asynchronous page-cache population ahead of chunk reader threads.
pub fn advise_sequential_read(file: &File, file_size: u64) {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = file.as_raw_fd();
        unsafe {
            libc::posix_fadvise(fd, 0, file_size as libc::off_t, libc::POSIX_FADV_SEQUENTIAL);
            libc::posix_fadvise(fd, 0, file_size as libc::off_t, libc::POSIX_FADV_WILLNEED);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (file, file_size);
    }
}

/// Opens a file for high-throughput sequential reading with platform-optimized kernel flags.
///
/// On Windows, sets `FILE_FLAG_SEQUENTIAL_SCAN` (0x08000000).
/// On Linux/Android, opens the file and applies `POSIX_FADV_SEQUENTIAL`.
pub fn open_sequential_read<P: AsRef<Path>>(path: P) -> std::io::Result<File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_SEQUENTIAL_SCAN: u32 = 0x08000000;
        std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_SEQUENTIAL_SCAN)
            .open(path.as_ref())
    }
    #[cfg(unix)]
    {
        let file = File::open(path.as_ref())?;
        if let Ok(meta) = file.metadata() {
            advise_sequential_read(&file, meta.len());
        }
        Ok(file)
    }
    #[cfg(not(any(unix, windows)))]
    {
        File::open(path.as_ref())
    }
}

pub fn preallocate_file(file: &File, file_size: u64) -> std::io::Result<()> {
    // Always set logical size first — required for read_exact / file cursor operations to work
    file.set_len(file_size)?;

    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        let fd = file.as_raw_fd();
        // Advisory: pre-allocate contiguous blocks to avoid fragmentation and random write stalls
        unsafe {
            libc::posix_fallocate(fd, 0, file_size as libc::off_t);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_open_sequential_read() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test_sequential.bin");
        {
            let mut f = File::create(&file_path).unwrap();
            f.write_all(b"SEQUENTIAL_DATA_TEST_12345").unwrap();
        }

        let file = open_sequential_read(&file_path).expect("Should open sequential file");
        let meta = file.metadata().unwrap();
        advise_sequential_read(&file, meta.len());
        assert_eq!(meta.len(), 26);
    }
}


d:\MyDocuments\Programming\android\Aug26\TurboTransfer\core\src\manifest\schema.rs

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransferRole {
    Sender,
    Receiver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransferStatus {
    InProgress,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportStats {
    pub bytes: u64,
    pub errors: u64,
    pub retries: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransportStatsMap {
    pub usb: TransportStats,
    pub wifi_direct: TransportStats,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferMeta {
    pub transfer_id: Uuid,
    pub file_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub chunk_size: u32,
    pub total_chunks: u32,
    pub role: TransferRole,
    pub peer_device_id: Uuid,
    pub status: TransferStatus,
    pub completed_ranges: Vec<(u32, u32)>,
    pub created_at: String,
    pub updated_at: String,
    pub transport_stats: TransportStatsMap,
}

impl TransferMeta {
    pub fn new(
        transfer_id: Uuid,
        file_id: Uuid,
        file_name: String,
        file_size: u64,
        chunk_size: u32,
        total_chunks: u32,
        role: TransferRole,
        peer_device_id: Uuid,
    ) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            transfer_id,
            file_id,
            file_name,
            file_size,
            chunk_size,
            total_chunks,
            role,
            peer_device_id,
            status: TransferStatus::InProgress,
            completed_ranges: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
            transport_stats: TransportStatsMap::default(),
        }
    }
}

/// Coalesces an in-memory `HashSet<u32>` of completed chunk IDs into a minimal,
/// sorted list of non-overlapping, non-adjacent inclusive `[start, end]` ranges.
pub fn coalesce_ranges(completed_chunks: &HashSet<u32>) -> Vec<(u32, u32)> {
    if completed_chunks.is_empty() {
        return Vec::new();
    }

    let mut chunk_ids: Vec<u32> = completed_chunks.iter().copied().collect();
    chunk_ids.sort_unstable();

    let mut ranges = Vec::new();
    let mut start = chunk_ids[0];
    let mut prev = start;

    for &id in &chunk_ids[1..] {
        if id == prev + 1 {
            prev = id;
        } else if id > prev + 1 {
            ranges.push((start, prev));
            start = id;
            prev = id;
        }
    }
    ranges.push((start, prev));

    ranges
}

/// Expands a list of inclusive `[start, end]` ranges back into an in-memory `HashSet<u32>`.
pub fn expand_ranges(ranges: &[(u32, u32)]) -> HashSet<u32> {
    let mut set = HashSet::new();
    for &(start, end) in ranges {
        for id in start..=end {
            set.insert(id);
        }
    }
    set
}

/// Represents file metadata and chunk count generated for a transfer offer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileManifest {
    pub file_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub chunk_size: u32,
    pub total_chunks: u32,
}

/// Generates a `FileManifest` for the file at `file_path`.
pub fn generate_manifest<P: AsRef<std::path::Path>>(
    file_path: P,
    chunk_size: u32,
) -> Result<FileManifest, std::io::Error> {
    generate_manifest_with_name(file_path, chunk_size, None)
}

/// Generates a `FileManifest` for the file at `file_path` with an optional custom file name.
pub fn generate_manifest_with_name<P: AsRef<std::path::Path>>(
    file_path: P,
    chunk_size: u32,
    custom_file_name: Option<&str>,
) -> Result<FileManifest, std::io::Error> {
    use std::io::Seek;
    let path = file_path.as_ref();
    let mut file = std::fs::File::open(path)?;
    let file_size = match file.metadata() {
        Ok(m) if m.len() > 0 => m.len(),
        _ => file.seek(std::io::SeekFrom::End(0))?,
    };
    let file_name = match custom_file_name {
        Some(name) => name.to_string(),
        None => {
            let resolved_path = std::fs::read_link(path).unwrap_or_else(|_| path.to_path_buf());
            resolved_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
        }
    };

    let num_chunks = crate::chunk::total_chunks(file_size, chunk_size);
    let file_id = Uuid::new_v4();

    Ok(FileManifest {
        file_id,
        file_name,
        file_size,
        chunk_size,
        total_chunks: num_chunks,
    })
}


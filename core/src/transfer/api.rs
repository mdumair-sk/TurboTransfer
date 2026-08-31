use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

use log::debug;

use super::session::{send_file_session_multipath, TransferSessionError};
use super::tracker::{ChunkTracker, InMemoryChunkTracker};
use crate::checksum::{compute_file_crc32c, compute_xxhash64};
use crate::manifest::{MetaActor, MetaActorHandle, TransferMeta, TransferRole, TransferStatus, TransportType};
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
    let mut map = registry.transfers.lock().unwrap();
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
            *existing.last_sample_bytes.lock().unwrap() = initial_bytes;
        }
        *existing.status.lock().unwrap() = TransferStatus::InProgress;
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

/// Updates the MetaActor handle of an active transfer.
pub fn set_transfer_actor_handle(transfer_id: Uuid, handle: MetaActorHandle) {
    let registry = get_registry();
    let mut map = registry.transfers.lock().unwrap();
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
    let map = registry.transfers.lock().unwrap();
    map.get(&transfer_id).and_then(|record| record.actor_handle.clone())
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

fn get_windows_hotspot_probe_ips() -> Vec<String> {
    let mut ips = Vec::new();
    #[cfg(target_os = "windows")]
    {
        for gw in WifiDirectTransport::resolve_windows_all_gateways() {
            let addr = format!("{}:9876", gw);
            if !ips.contains(&addr) {
                ips.push(addr);
            }
        }
    }
    for static_ip in &[
        "10.18.163.1:9876",
        "10.18.163.2:9876",
        "10.18.163.130:9876",
        "10.78.112.40:9876",
        "192.168.43.1:9876",
        "192.168.43.2:9876",
        "192.168.137.1:9876",
        "192.168.1.19:9876",
    ] {
        let addr = static_ip.to_string();
        if !ips.contains(&addr) {
            ips.push(addr);
        }
    }
    ips
}

/// Connects all available transport channels according to the preference and network environment.
pub async fn resolve_and_connect_transports(
    transport_pref: TransportPreference,
    address: Option<&str>,
) -> Result<(Vec<(Box<dyn Transport>, bool)>, Vec<String>), TransferSessionError> {
    let addr_default = DEFAULT_LOOPBACK_ADDR.to_string();
    let addr = address.unwrap_or(&addr_default);
    let mut transports: Vec<(Box<dyn Transport>, bool)> = Vec::new();
    let mut transport_names: Vec<String> = Vec::new();

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
            if let Some(explicit_addr) = address {
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
            if let Some(explicit_addr) = address {
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
                let probe_ips = get_windows_hotspot_probe_ips();
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

            if transports.is_empty() {
                return Err(TransferSessionError::Transport(crate::transport::TransportError::Disconnected(
                    "Failed to connect over either USB or Wi-Fi Direct".into(),
                )));
            }
        }
        TransportPreference::Automatic => {
            if let Some(explicit_addr) = address {
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
                    let probe_ips = get_windows_hotspot_probe_ips();
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
            }

            if transports.is_empty() {
                if let Some(explicit_addr) = address {
                    let fallback_addrs: Vec<&str> = explicit_addr.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
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
                } else {
                    // Fallback to loopback only if running in test environment or loopback server is active
                    if let Ok(Ok(t)) = tokio::time::timeout(
                        tokio::time::Duration::from_millis(400),
                        TcpTransport::connect(DEFAULT_LOOPBACK_ADDR),
                    ).await {
                        transports.push((Box::new(t), true));
                        transport_names.push("Loopback TCP".to_string());
                    }
                }
            }
        }
    }

    if transports.is_empty() {
        return Err(TransferSessionError::Transport(
            crate::transport::TransportError::Disconnected("Failed to establish connection on any transport".into()),
        ));
    }

    Ok((transports, transport_names))
}

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
    let is_high_speed = transport_pref == TransportPreference::UsbOnly || transport_pref == TransportPreference::Combined;
    let chunk_size = crate::chunk::select_optimal_chunk_size(file_size, is_high_speed);
    let plan = crate::chunk::calculate_chunk_plan(file_size, chunk_size);
    let total_chunks = plan.len().max(1) as u32;

    // Register active transfer in registry immediately so UI/API can track progress from initiation
    register_active_transfer_with_path(
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
        Some(file_path.clone()),
        0,
        0,
    );

    // Create initial TransferMeta and spawn MetaActor so resumable meta.json exists on disk immediately
    let mut initial_meta = TransferMeta::new(
        transfer_id,
        Uuid::new_v4(),
        file_name.clone(),
        file_size,
        chunk_size,
        total_chunks,
        TransferRole::Sender,
        _target_device_id,
    );
    initial_meta.source_file_path = Some(file_path.to_string_lossy().to_string());
    let meta_path = default_data_dir().join(format!("{}.meta.json", transfer_id));
    let (actor_handle, _actor_join) = MetaActor::spawn(meta_path, initial_meta, 100);
    set_transfer_actor_handle(transfer_id, actor_handle);

    let (transports, transport_names) = match resolve_and_connect_transports(transport_pref, address.as_deref()).await {
        Ok(res) => res,
        Err(e) => {
            log::error!("start_transfer connection failure: {}", e);
            if let Some(telemetry) = crate::util::telemetry::get_telemetry(transfer_id) {
                telemetry.mark_failed(&e.to_string());
                let data_dir = default_data_dir();
                export_and_clean_telemetry(transfer_id, &data_dir);
            }
            set_transfer_status(transfer_id, TransferStatus::Failed, Some(e.to_string()));
            return Err(e);
        }
    };

    let transport_name = if transport_names.len() > 1 {
        format!("{} (Multipath Active)", transport_names.join(" + "))
    } else {
        transport_names.into_iter().next().unwrap_or_else(|| "TCP Transport".to_string())
    };
    log::info!("start_transfer connected successfully via {}", transport_name);

    update_transfer_transport_name(transfer_id, transport_name);

    crate::util::runtime::spawn_task(async move {
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
            crate::util::runtime::spawn_task(async {
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

    let handle = crate::util::runtime::spawn_task(async move {
        loop {
            tokio::select! {
                accept_res = listener.accept() => {
                    match accept_res {
                        Ok((transport, peer_addr)) => {
                            let is_usb = peer_addr.ip().is_loopback();
                            let tx = completion_tx.clone();
                            let ddir = dest_dir.clone();
                            crate::util::runtime::spawn_task(async move {
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
    pub disk_error: Arc<parking_lot::Mutex<Option<String>>>,
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
            let (part_path, final_path) = crate::util::storage::resolve_secure_paths(&dest_dir, &offer.file_name)?;
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

            if !is_sender_in_same_process {
                let meta_path = default_data_dir().join(format!("{}.meta.json", offer.transfer_id));
                let initial_meta = TransferMeta::new(
                    offer.transfer_id,
                    offer.file_id,
                    offer.file_name.clone(),
                    offer.file_size,
                    offer.chunk_size,
                    offer.total_chunks,
                    TransferRole::Receiver,
                    Uuid::nil(),
                );
                let (actor_handle, _actor_join) = MetaActor::spawn(meta_path, initial_meta, 100);
                set_transfer_actor_handle(offer.transfer_id, actor_handle);
            }

            let disk_error = Arc::new(parking_lot::Mutex::new(None));
            let disk_error_clone = disk_error.clone();

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
                                *disk_error_clone.lock() = Some(e.to_string());
                            }
                            let write_us = t_w0.elapsed().as_micros() as u64;
                            tel_for_disk.record_disk_write(chunk_id, len, write_us, queue_depth);
                        }
                        DiskWriteCmd::Flush(reply_tx) => {
                            let res = writer_file.flush();
                            let _ = reply_tx.send(res);
                        }
                        DiskWriteCmd::Close(reply_tx) => {
                            let flush_res = writer_file.flush();
                            let sync_res = writer_file.sync_all();
                            drop(writer_file);
                            let final_res = if let Some(ref err_msg) = *disk_error_clone.lock() {
                                Err(std::io::Error::new(std::io::ErrorKind::Other, err_msg.clone()))
                            } else {
                                flush_res.and(sync_res)
                            };
                            let _ = reply_tx.send(final_res);
                            break;
                        }
                    }
                }
            });

            let tracker = if let Some((_, meta)) = find_resumable_transfer(Some(offer.transfer_id)) {
                if !meta.completed_ranges.is_empty() {
                    InMemoryChunkTracker::from_ranges(offer.transfer_id, offer.file_id, &meta.completed_ranges)
                } else {
                    InMemoryChunkTracker::new()
                }
            } else {
                InMemoryChunkTracker::new()
            };

            let new_session = Arc::new(ActiveReceiveSession {
                file_path: final_path,
                part_path,
                disk_tx,
                disk_error,
                tracker: Arc::new(parking_lot::Mutex::new(tracker)),
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
    let actor_handle = get_transfer_actor_handle(offer.transfer_id);

    // 4. Send TransferAccept
    let resume_from = session.tracker.lock().get_completed_ranges();
    let accept = Message::TransferAccept(TransferAcceptData {
        transfer_id: offer.transfer_id,
        resume_from,
    });
    transport.send_frame(&accept).await?;

    // 5. Data Plane Receive Loop
    loop {
        let ch_name = if is_usb { "USB" } else { "Wi-Fi" };
        let frame_res = transport.receive_frame().await;
        let frame = match frame_res {
            Ok(Some(f)) => f,
            Ok(None) => {
                session.telemetry.record_channel_disconnect(ch_name, "Peer disconnected / EOF");
                break;
            }
            Err(e) => {
                session.telemetry.record_channel_disconnect(ch_name, &e.to_string());
                return Err(TransferSessionError::Transport(e));
            }
        };

        match frame {
            Message::ChunkData(chunk_data) => {
                let t_v0 = std::time::Instant::now();
                let computed_checksum = compute_xxhash64(&chunk_data.payload);
                let verify_us = t_v0.elapsed().as_micros() as u64;

                if computed_checksum != chunk_data.checksum {
                    session.telemetry.record_chunk_nack(ch_name, chunk_data.chunk_id, "xxHash64 payload mismatch");
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

                let is_already_done = {
                    let tracker = session.tracker.lock();
                    tracker.is_chunk_completed(
                        chunk_data.transfer_id,
                        chunk_data.file_id,
                        chunk_data.chunk_id,
                        chunk_data.checksum,
                    )
                };

                if is_already_done {
                    session.telemetry.record_duplicate_chunk(chunk_data.chunk_id);
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
                    if let Some(actor) = actor_handle.as_ref() {
                        let t_type = if is_usb { TransportType::Usb } else { TransportType::WifiDirect };
                        actor.try_send_chunk_completed(chunk_data.chunk_id, t_type, chunk_data.payload_length as u64);
                    }
                }

                if let Some(ref err) = *session.disk_error.lock() {
                    return Err(TransferSessionError::Io(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Receiver disk write failed: {}", err),
                    )));
                }

                // Queue disk write before sending ACK
                let q_depth = (128 - session.disk_tx.capacity()) as u32;
                if session.disk_tx.send(DiskWriteCmd::Write {
                    chunk_id: chunk_data.chunk_id,
                    file_offset: chunk_data.file_offset,
                    payload: chunk_data.payload,
                    queue_depth: q_depth,
                }).await.is_err() {
                    return Err(TransferSessionError::Io(std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "Disk writer task terminated unexpectedly",
                    )));
                }

                // Send immediate ChunkAck for 100% universal sender compatibility
                let ack = Message::ChunkAck(ChunkAckData {
                    transfer_id: chunk_data.transfer_id,
                    chunk_id: chunk_data.chunk_id,
                    receiver_verify_us: Some(verify_us as u32),
                });
                transport.send_frame(&ack).await?;
            }
            Message::Complete(complete_data) => {
                let was_completed = session.is_completed.swap(true, Ordering::SeqCst);
                if !was_completed {
                    let t_fin0 = std::time::Instant::now();
                    // Close background disk writer and flush/fsync before checking file checksum
                    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                    let _ = session.disk_tx.send(DiskWriteCmd::Close(reply_tx)).await;
                    if let Ok(res) = reply_rx.await {
                        if let Err(e) = res {
                            session.telemetry.mark_failed(&format!("Disk flush/fsync error: {}", e));
                            let data_dir = default_data_dir();
                            export_and_clean_telemetry(complete_data.transfer_id, &data_dir);
                            set_transfer_status(
                                complete_data.transfer_id,
                                TransferStatus::Failed,
                                Some(format!("Disk write error: {}", e)),
                            );
                            return Err(TransferSessionError::Io(e));
                        }
                    }
                    if let Some(ref err) = *session.disk_error.lock() {
                        session.telemetry.mark_failed(&format!("Disk write error: {}", err));
                        let data_dir = default_data_dir();
                        export_and_clean_telemetry(complete_data.transfer_id, &data_dir);
                        set_transfer_status(
                            complete_data.transfer_id,
                            TransferStatus::Failed,
                            Some(format!("Disk write error: {}", err)),
                        );
                        return Err(TransferSessionError::Io(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("Disk write error: {}", err),
                        )));
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

    // Clean up active receive session if transfer is not completed and this was the last active transport channel
    if !session.is_completed.load(Ordering::SeqCst) {
        let should_cleanup = {
            let map = get_active_receive_sessions().lock().unwrap();
            map.get(&offer.transfer_id).map_or(true, |s| Arc::strong_count(s) <= 2)
        };
        if should_cleanup {
            let session_opt = {
                let mut map = get_active_receive_sessions().lock().unwrap();
                map.remove(&offer.transfer_id)
            };
            if let Some(s) = session_opt {
                let (reply_tx, _) = tokio::sync::oneshot::channel();
                let _ = s.disk_tx.send(DiskWriteCmd::Close(reply_tx)).await;
            }
            let current_status = transfer_control_status(offer.transfer_id);
            if current_status != Some(TransferStatus::Paused) && current_status != Some(TransferStatus::Cancelled) && current_status != Some(TransferStatus::Completed) {
                set_transfer_status(offer.transfer_id, TransferStatus::Failed, Some("Transport connection closed unexpectedly".to_string()));
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
            crate::util::runtime::spawn_task(async move {
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

    let mut candidate: Option<(PathBuf, crate::manifest::TransferMeta, String)> = None;

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
                        let updated = meta.updated_at.clone();
                        let is_newer = match &candidate {
                            Some((_, _, prev_updated)) => updated > *prev_updated,
                            None => true,
                        };
                        if is_newer {
                            candidate = Some((path, meta, updated));
                        }
                    }
                }
            }
        }
    }

    candidate.map(|(path, meta, _)| (path, meta))
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
    let chunk_size = meta.chunk_size;

    // Calculate initial completed chunks and bytes from meta.completed_ranges
    let mut initial_chunks_done = 0u32;
    let mut initial_bytes_done = 0u64;
    for &(start, end) in &meta.completed_ranges {
        let count = end.saturating_sub(start) + 1;
        initial_chunks_done += count;
        for cid in start..=end {
            let chunk_len = if cid == total_chunks.saturating_sub(1) {
                let remainder = (file_size % chunk_size as u64) as u32;
                if remainder > 0 { remainder } else { chunk_size }
            } else {
                chunk_size
            };
            initial_bytes_done += chunk_len as u64;
        }
    }

    if role == TransferRole::Receiver {
        set_transfer_status(tid, TransferStatus::InProgress, None);
        update_transfer_progress(tid, initial_bytes_done, initial_chunks_done);
        return Ok(TransferHandle { transfer_id: tid });
    }

    // Sender: resolve source file path
    let registry = get_registry();
    let resolved_path = {
        let map = registry.transfers.lock().unwrap();
        map.get(&tid).and_then(|r| r.source_file_path.clone())
    }
    .or_else(|| meta.source_file_path.as_ref().map(PathBuf::from))
    .or_else(|| {
        let direct = PathBuf::from(&meta.file_name);
        if direct.exists() {
            Some(direct)
        } else {
            let cand1 = default_data_dir().join(&meta.file_name);
            if cand1.exists() {
                Some(cand1)
            } else {
                #[cfg(target_os = "android")]
                {
                    let cand2 = PathBuf::from("/storage/emulated/0/Download").join(&meta.file_name);
                    if cand2.exists() {
                        return Some(cand2);
                    }
                    let cand3 = PathBuf::from("/sdcard/Download").join(&meta.file_name);
                    if cand3.exists() {
                        return Some(cand3);
                    }
                }
                None
            }
        }
    })
    .unwrap_or_else(|| PathBuf::from(&meta.file_name));

    if !resolved_path.exists() {
        return Err(TransferSessionError::Rejected(format!(
            "Source file not found for resume: {:?}. Original file name: {}",
            resolved_path, meta.file_name
        )));
    }

    // Start MetaActor loading from existing meta_path
    let (actor_handle, _join) = MetaActor::spawn(meta_path, meta.clone(), 100);

    // Register / update active transfer preserving completed progress
    register_active_transfer_with_path(
        tid,
        file_name.clone(),
        file_size,
        role,
        total_chunks,
        "Resuming...".to_string(),
        Some(resolved_path.clone()),
        initial_bytes_done,
        initial_chunks_done,
    );
    set_transfer_actor_handle(tid, actor_handle);

    // Connect transports with full multi-channel resolution
    let (transports, transport_names) = resolve_and_connect_transports(transport_pref, address.as_deref()).await?;
    let transport_name = if transport_names.len() > 1 {
        format!("{} (Multipath Active)", transport_names.join(" + "))
    } else {
        transport_names.into_iter().next().unwrap_or_else(|| "TCP Transport".to_string())
    };
    update_transfer_transport_name(tid, transport_name);

    let sender_id = Uuid::new_v4();
    let file_path = resolved_path;
    let custom_name = meta.file_name.clone();

    crate::util::runtime::spawn_task(async move {
        let res = send_file_session_multipath(
            sender_id,
            "TurboSender",
            &file_path,
            chunk_size,
            tid,
            transports,
            Some(&custom_name),
        )
        .await;

        match res {
            Ok(()) => {
                set_transfer_status(tid, TransferStatus::Completed, None);
            }
            Err(TransferSessionError::Paused | TransferSessionError::Cancelled) => {}
            Err(e) => {
                set_transfer_status(tid, TransferStatus::Failed, Some(e.to_string()));
            }
        }
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
            crate::util::runtime::spawn_task(async move {
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

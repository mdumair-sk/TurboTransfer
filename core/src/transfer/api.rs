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
        },
    );
}

/// Default loopback TCP address for Milestone 5 / 6 transfers.
pub const DEFAULT_LOOPBACK_ADDR: &str = "127.0.0.1:9876";

/// Default listen address for Milestone 6 real network transfers.
pub const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0:9876";

/// Starts a file transfer to a peer over `TcpTransport` or `UsbTransport` (§6, §7, §8).
pub async fn start_transfer(
    file_path: PathBuf,
    device_id: Option<Uuid>,
    transport_pref: TransportPreference,
    address: Option<String>,
) -> Result<TransferHandle, TransferSessionError> {
    let addr = address.as_deref().unwrap_or(DEFAULT_LOOPBACK_ADDR);
    let mut transports: Vec<Box<dyn Transport>> = Vec::new();
    let mut transport_names: Vec<String> = Vec::new();

    match transport_pref {
        TransportPreference::UsbOnly => {
            if let Ok(t) = TcpTransport::connect(addr).await {
                transports.push(Box::new(t));
                transport_names.push("USB 3.0 (ADB Tunnel)".to_string());
            } else {
                let config = UsbTransportConfig::new(9876, 9876);
                let t = UsbTransport::connect(config).await?;
                transports.push(Box::new(t));
                transport_names.push("USB 3.0 (ADB Tunnel)".to_string());
            }
        }
        TransportPreference::WifiDirectOnly => {
            if let Some(explicit_addr) = address {
                let transport = TcpTransport::connect(&explicit_addr).await?;
                transports.push(Box::new(transport));
                transport_names.push("5 GHz Wi-Fi Direct".to_string());
            } else {
                let config = WifiDirectTransport::discover_android_hotspot(None)
                    .await
                    .ok_or_else(|| TransferSessionError::Rejected(
                        "No Android Local-Only Hotspot was discovered over USB control channel".into(),
                    ))?;
                let transport = WifiDirectTransport::connect(config).await?;
                transports.push(Box::new(transport));
                transport_names.push("5 GHz Local-Only Hotspot".to_string());
            }
        }
        TransportPreference::Combined => {
            if let Some(ref explicit_addr) = address {
                if explicit_addr.contains(',') {
                    for single_addr in explicit_addr.split(',') {
                        let trimmed = single_addr.trim();
                        if !trimmed.is_empty() {
                            if let Ok(t) = TcpTransport::connect(trimmed).await {
                                let is_usb = trimmed.contains("127.0.0.1") || trimmed.contains("localhost") || trimmed.contains("usb");
                                let name = if is_usb {
                                    "USB 3.0 (ADB Tunnel)".to_string()
                                } else {
                                    format!("5 GHz Wi-Fi Direct ({})", trimmed)
                                };
                                transports.push(Box::new(t));
                                transport_names.push(name);
                            }
                        }
                    }
                } else if let Ok(t) = TcpTransport::connect(explicit_addr).await {
                    let is_usb = explicit_addr.contains("127.0.0.1") || explicit_addr.contains("localhost") || explicit_addr.contains("usb");
                    let name = if is_usb {
                        "USB 3.0 (ADB Tunnel)".to_string()
                    } else {
                        format!("5 GHz Wi-Fi Direct ({})", explicit_addr)
                    };
                    transports.push(Box::new(t));
                    transport_names.push(name);
                }
            }

            if transports.is_empty() {
                // 1. Connect USB channel
                if let Ok(t) = TcpTransport::connect(DEFAULT_LOOPBACK_ADDR).await {
                    transports.push(Box::new(t));
                    transport_names.push("USB 3.0 (ADB Tunnel)".to_string());
                } else {
                    let usb_config = UsbTransportConfig::new(9876, 9876);
                    if let Ok(t) = UsbTransport::connect(usb_config).await {
                        transports.push(Box::new(t));
                        transport_names.push("USB 3.0 (ADB Tunnel)".to_string());
                    }
                }

                // 2. Connect Wi-Fi Direct channel
                for hotspot_ip in &["192.168.43.2:9876", "192.168.43.1:9876", "192.168.1.19:9876", "10.18.163.130:9876", "10.18.163.1:9876"] {
                    if let Ok(t) = tokio::time::timeout(tokio::time::Duration::from_millis(600), TcpTransport::connect(hotspot_ip)).await {
                        if let Ok(transport) = t {
                            transports.push(Box::new(transport));
                            transport_names.push("5 GHz Wi-Fi Direct".to_string());
                            break;
                        }
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
                            if let Ok(t) = TcpTransport::connect(trimmed).await {
                                let is_usb = trimmed.contains("127.0.0.1") || trimmed.contains("localhost") || trimmed.contains("usb");
                                let name = if is_usb {
                                    "USB 3.0 (ADB Tunnel)".to_string()
                                } else {
                                    format!("5 GHz Wi-Fi Direct ({})", trimmed)
                                };
                                transports.push(Box::new(t));
                                transport_names.push(name);
                            }
                        }
                    }
                } else if let Ok(t) = TcpTransport::connect(explicit_addr).await {
                    let is_usb = explicit_addr.contains("127.0.0.1") || explicit_addr.contains("localhost") || explicit_addr.contains("usb");
                    let name = if is_usb {
                        "USB 3.0 (ADB Tunnel)".to_string()
                    } else {
                        format!("5 GHz Wi-Fi Direct ({})", explicit_addr)
                    };
                    transports.push(Box::new(t));
                    transport_names.push(name);
                }
            }

            if transports.is_empty() {
                #[cfg(target_os = "android")]
                {
                    // Probe USB reverse tunnel
                    if let Ok(t) = tokio::time::timeout(tokio::time::Duration::from_millis(800), TcpTransport::connect(addr)).await {
                        if let Ok(transport) = t {
                            transports.push(Box::new(transport));
                            transport_names.push("USB ADB Reverse Tunnel".to_string());
                        }
                    }
                    // Probe Wi-Fi Direct / Hotspot gateway
                    for hotspot_ip in &["192.168.43.2:9876", "192.168.43.1:9876"] {
                        if let Ok(t) = tokio::time::timeout(tokio::time::Duration::from_millis(500), TcpTransport::connect(hotspot_ip)).await {
                            if let Ok(transport) = t {
                                transports.push(Box::new(transport));
                                transport_names.push("5 GHz Wi-Fi Direct".to_string());
                                break;
                            }
                        }
                    }
                }

                #[cfg(not(target_os = "android"))]
                {
                    let usb_config = UsbTransportConfig::new(9876, 9876);
                    if let Ok(t) = UsbTransport::connect(usb_config).await {
                        transports.push(Box::new(t));
                        transport_names.push("USB 3.0 (ADB Tunnel)".to_string());
                    } else if let Ok(t) = TcpTransport::connect(addr).await {
                        transports.push(Box::new(t));
                        transport_names.push("USB Tunnel".to_string());
                    }

                    if let Some(config) = WifiDirectTransport::discover_android_hotspot(None).await {
                        if let Ok(t) = WifiDirectTransport::connect(config).await {
                            transports.push(Box::new(t));
                            transport_names.push("5 GHz Local-Only Hotspot".to_string());
                        }
                    }
                }
            }

            if transports.is_empty() {
                let t = TcpTransport::connect(addr).await?;
                transports.push(Box::new(t));
                transport_names.push("TCP Transport".to_string());
            }
        }
    }

    let transport_name = if transport_names.len() > 1 {
        format!("{} (Multipath Active)", transport_names.join(" + "))
    } else {
        transport_names.into_iter().next().unwrap_or_else(|| "TCP Transport".to_string())
    };

    let sender_id = Uuid::new_v4();
    let _target_device_id = device_id.unwrap_or_else(Uuid::new_v4);
    let transfer_id = Uuid::new_v4();
    let chunk_size = 4 * 1024 * 1024; // 4 MiB chunks for high throughput and granular real-time progress updates

    let resolved_path = std::fs::read_link(&file_path).unwrap_or_else(|_| file_path.clone());
    let file_name = resolved_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    let file_size = std::fs::metadata(&file_path)?.len();
    let plan = crate::chunk::calculate_chunk_plan(file_size, chunk_size);
    let total_chunks = plan.len().max(1) as u32;

    // Register active transfer in registry
    register_active_transfer(
        transfer_id,
        file_name,
        file_size,
        TransferRole::Sender,
        total_chunks,
        transport_name,
    );

    tokio::spawn(async move {
        let res = send_file_session_multipath(
            sender_id,
            "TurboSender",
            &file_path,
            chunk_size,
            transfer_id,
            transports,
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

enum DiskWriteCmd {
    Write { file_offset: u64, payload: Vec<u8> },
    Flush(tokio::sync::oneshot::Sender<std::io::Result<()>>),
}

struct ActiveReceiveSession {
    pub file_path: PathBuf,
    pub part_path: PathBuf,
    pub disk_tx: tokio::sync::mpsc::Sender<DiskWriteCmd>,
    pub tracker: Arc<tokio::sync::Mutex<InMemoryChunkTracker>>,
    pub bytes_recv_total: Arc<AtomicU64>,
    pub completed_chunks_count: Arc<AtomicU32>,
    pub is_completed: Arc<std::sync::atomic::AtomicBool>,
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
            file.set_len(offer.file_size)?;

            register_active_transfer(
                offer.transfer_id,
                offer.file_name.clone(),
                offer.file_size,
                TransferRole::Receiver,
                offer.total_chunks,
                "Multi-Channel Ingestion".to_string(),
            );

            let (disk_tx, mut disk_rx) = tokio::sync::mpsc::channel::<DiskWriteCmd>(32);
            let mut writer_file = file;
            tokio::task::spawn_blocking(move || {
                use std::io::{Seek, SeekFrom, Write};
                while let Some(cmd) = disk_rx.blocking_recv() {
                    match cmd {
                        DiskWriteCmd::Write { file_offset, payload } => {
                            if let Err(e) = writer_file.seek(SeekFrom::Start(file_offset)).and_then(|_| writer_file.write_all(&payload)) {
                                log::error!("Background disk write error: {}", e);
                            }
                        }
                        DiskWriteCmd::Flush(reply_tx) => {
                            let res = writer_file.flush();
                            let _ = reply_tx.send(res);
                        }
                    }
                }
                let _ = writer_file.flush();
            });

            let new_session = Arc::new(ActiveReceiveSession {
                file_path: final_path,
                part_path,
                disk_tx,
                tracker: Arc::new(tokio::sync::Mutex::new(InMemoryChunkTracker::new())),
                bytes_recv_total: Arc::new(AtomicU64::new(0)),
                completed_chunks_count: Arc::new(AtomicU32::new(0)),
                is_completed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
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
                let computed_checksum = compute_xxhash64(&chunk_data.payload);
                if computed_checksum != chunk_data.checksum {
                    let nack = Message::ChunkNack(ChunkNackData {
                        transfer_id: chunk_data.transfer_id,
                        chunk_id: chunk_data.chunk_id,
                        reason: "xxHash64 payload mismatch".to_string(),
                    });
                    transport.send_frame(&nack).await?;
                    continue;
                }

                let is_duplicate = {
                    let tracker = session.tracker.lock().await;
                    tracker.is_chunk_completed(
                        chunk_data.transfer_id,
                        chunk_data.file_id,
                        chunk_data.chunk_id,
                        chunk_data.checksum,
                    )
                };

                if is_duplicate {
                    let ack = Message::ChunkAck(ChunkAckData {
                        transfer_id: chunk_data.transfer_id,
                        chunk_id: chunk_data.chunk_id,
                    });
                    transport.send_frame(&ack).await?;
                    continue;
                }

                {
                    let mut tracker = session.tracker.lock().await;
                    tracker.mark_chunk_completed(
                        chunk_data.transfer_id,
                        chunk_data.file_id,
                        chunk_data.chunk_id,
                        chunk_data.checksum,
                    );
                }

                let total_b = session
                    .bytes_recv_total
                    .fetch_add(chunk_data.payload_length as u64, Ordering::Relaxed)
                    + chunk_data.payload_length as u64;
                let total_c = session.completed_chunks_count.fetch_add(1, Ordering::Relaxed) + 1;
                update_transfer_progress(chunk_data.transfer_id, total_b, total_c);
                record_channel_bytes(chunk_data.transfer_id, is_usb, chunk_data.payload_length as u64);

                // Send ACK immediately to sustain wire-speed TCP throughput
                let ack = Message::ChunkAck(ChunkAckData {
                    transfer_id: chunk_data.transfer_id,
                    chunk_id: chunk_data.chunk_id,
                });
                transport.send_frame(&ack).await?;

                // Queue disk write asynchronously
                let _ = session.disk_tx.send(DiskWriteCmd::Write {
                    file_offset: chunk_data.file_offset,
                    payload: chunk_data.payload,
                }).await;
            }
            Message::Complete(complete_data) => {
                let was_completed = session.is_completed.swap(true, Ordering::SeqCst);
                if !was_completed {
                    // Flush background disk writes before checking file checksum
                    let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
                    let _ = session.disk_tx.send(DiskWriteCmd::Flush(reply_tx)).await;
                    if let Ok(res) = reply_rx.await {
                        res?;
                    }

                    let file_crc = compute_file_crc32c(&session.part_path)?;
                    if file_crc != complete_data.file_checksum {
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

                    let _ = completion_tx.send(session.file_path.clone());
                    get_active_receive_sessions()
                        .lock()
                        .unwrap()
                        .remove(&complete_data.transfer_id);
                }

                let ack = Message::ChunkAck(ChunkAckData {
                    transfer_id: complete_data.transfer_id,
                    chunk_id: u32::MAX,
                });
                transport.send_frame(&ack).await?;
                return Ok(());
            }
            _ => {}
        }
    }

    Ok(())
}

/// Stops a named listener, or every listener when no address is supplied.
pub fn leave_receive_mode(address: Option<&str>) -> bool {
    let mut listeners = get_receive_listeners().lock().unwrap();
    if let Some(addr) = address {
        if let Some(listener) = listeners.remove(addr) {
            listener.abort.abort();
            return true;
        }
        return false;
    }

    let active = std::mem::take(&mut *listeners);
    let stopped = !active.is_empty();
    for listener in active.into_values() {
        listener.abort.abort();
    }
    stopped
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

/// Returns the default metadata storage directory (§12).
pub fn default_data_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = PathBuf::from(appdata).join("turbotransfer");
        let _ = std::fs::create_dir_all(&p);
        p
    } else {
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
    let chunk_size = meta.chunk_size;

    tokio::spawn(async move {
        let _ = send_file_session(
            sender_id,
            "TurboSender",
            &file_path,
            chunk_size,
            tid,
            transport,
        )
        .await;
    });

    Ok(TransferHandle { transfer_id: tid })
}

/// Cancels an active transfer (§7).
pub fn cancel_transfer(transfer_id: Uuid) {
    set_transfer_status(transfer_id, TransferStatus::Cancelled, None);
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

    let delta_t = now.duration_since(*last_time).as_secs_f64();
    if delta_t >= 0.20 {
        let delta_b = bytes.saturating_sub(*last_bytes) as f64;
        let delta_usb = usb_bytes.saturating_sub(*last_usb_bytes) as f64;
        let delta_wifi = wifi_bytes.saturating_sub(*last_wifi_bytes) as f64;

        let inst_bps = delta_b / delta_t;
        let inst_usb_bps = delta_usb / delta_t;
        let inst_wifi_bps = delta_wifi / delta_t;

        *rolling = if *rolling == 0.0 { inst_bps } else { *rolling * 0.35 + inst_bps * 0.65 };
        *rolling_usb = if *rolling_usb == 0.0 { inst_usb_bps } else { *rolling_usb * 0.35 + inst_usb_bps * 0.65 };
        *rolling_wifi = if *rolling_wifi == 0.0 { inst_wifi_bps } else { *rolling_wifi * 0.35 + inst_wifi_bps * 0.65 };

        *last_time = now;
        *last_bytes = bytes;
        *last_usb_bytes = usb_bytes;
        *last_wifi_bytes = wifi_bytes;
    }

    let (throughput, usb_speed, wifi_speed) = if status == TransferStatus::Completed {
        let elapsed = record.start_time.elapsed().as_secs_f64();
        if elapsed > 0.05 {
            (
                (bytes as f64) / elapsed,
                (usb_bytes as f64) / elapsed,
                (wifi_bytes as f64) / elapsed,
            )
        } else {
            (0.0, 0.0, 0.0)
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
    let eta = if throughput > 0.0 && bytes < record.file_size {
        Some(((record.file_size - bytes) as f64 / throughput).ceil() as u64)
    } else {
        None
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

    // 1. Enumerate connected ADB devices that are actively in Receive Mode
    if let Ok(adb_devs) = UsbTransport::list_adb_devices() {
        for d in adb_devs {
            if d.state == "device" {
                if UsbTransport::is_receiver_listening(&d.serial, 9876) {
                    let name = if let Some(model) = &d.model {
                        format!("Android Phone: {} ({})", model, d.serial)
                    } else if let Some(prod) = &d.product {
                        format!("Android Device: {} ({})", prod, d.serial)
                    } else {
                        format!("Android ADB Device ({})", d.serial)
                    };
                    devices.push(DeviceInfo {
                        device_id: Uuid::from_u128(crate::checksum::compute_xxhash64(d.serial.as_bytes()) as u128),
                        device_name: name,
                        transport: "USB (Ready to Receive)".to_string(),
                        is_connected: true,
                    });
                }
            }
        }
    }

    // 2. LAN / Wi-Fi Active Receivers Probe
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

    // 2. Scan disk metadata files
    let dir = default_data_dir();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json")
                || path.file_name().and_then(|n| n.to_str()).map_or(false, |n| n.ends_with(".meta.json"))
            {
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

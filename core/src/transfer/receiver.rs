use log::debug;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use uuid::Uuid;

use super::registry::{
    get_registry, get_transfer_actor_handle, record_channel_bytes, register_active_transfer,
    set_transfer_actor_handle, set_transfer_status, transfer_control_status,
    update_transfer_progress,
};
use super::sender::{default_data_dir, find_resumable_transfer, DEFAULT_LISTEN_ADDR};
use super::session::TransferSessionError;
use super::tracker::InMemoryChunkTracker;
use crate::checksum::{compute_file_crc32c, compute_xxhash64};
use crate::manifest::{MetaActor, TransferMeta, TransferRole, TransferStatus, TransportType};
use crate::protocol::{
    ChunkAckData, ChunkNackData, HelloData, Message, TransferAcceptData,
    CURRENT_PROTOCOL_VERSION, MIN_SUPPORTED_PROTOCOL_VERSION,
};
use crate::transport::{TcpListenerTransport, Transport};
#[cfg(not(target_os = "android"))]
use crate::transport::{UsbTransport, WifiDirectTransport};
use crate::util::telemetry::{
    export_and_clean_telemetry, get_or_create_telemetry, EventLevel, TransferStage,
    TransferTelemetry,
};

/// Receive listeners are process-owned resources, not UI state. Keeping their
/// abort handles here lets every frontend stop the exact listener it started.
pub(crate) struct ReceiveListener {
    pub(crate) abort: tokio::task::AbortHandle,
}

static RECEIVE_LISTENERS: std::sync::LazyLock<Mutex<HashMap<String, ReceiveListener>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn get_receive_listeners() -> &'static Mutex<HashMap<String, ReceiveListener>> {
    &RECEIVE_LISTENERS
}

pub(crate) enum DiskWriteCmd {
    Write {
        chunk_id: u32,
        file_offset: u64,
        payload: Vec<u8>,
        queue_depth: u32,
    },
    #[allow(dead_code)]
    Flush(tokio::sync::oneshot::Sender<std::io::Result<()>>),
    Close(tokio::sync::oneshot::Sender<std::io::Result<()>>),
}

pub(crate) struct ActiveReceiveSession {
    pub file_path: PathBuf,
    pub part_path: PathBuf,
    pub disk_tx: tokio::sync::mpsc::Sender<DiskWriteCmd>,
    pub disk_error: Arc<Mutex<Option<String>>>,
    pub tracker: Arc<Mutex<InMemoryChunkTracker>>,
    pub chunk_crcs: Arc<Mutex<HashMap<u32, (u32, usize)>>>,
    pub total_chunks: u32,
    pub bytes_recv_total: Arc<AtomicU64>,
    pub completed_chunks_count: Arc<AtomicU32>,
    pub is_completed: Arc<AtomicBool>,
    pub is_sender_in_same_process: bool,
    pub is_ephemeral: bool,
    pub telemetry: Arc<TransferTelemetry>,
}

static ACTIVE_RECEIVE_SESSIONS: std::sync::LazyLock<Mutex<HashMap<Uuid, Arc<ActiveReceiveSession>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) fn get_active_receive_sessions() -> &'static Mutex<HashMap<Uuid, Arc<ActiveReceiveSession>>> {
    &ACTIVE_RECEIVE_SESSIONS
}

pub(crate) fn cancel_receive_session(transfer_id: Uuid) {
    if let Some(s) = get_active_receive_sessions().lock().remove(&transfer_id) {
        let (reply_tx, _) = tokio::sync::oneshot::channel();
        let _ = s.disk_tx.try_send(DiskWriteCmd::Close(reply_tx));
    }
}

/// Enters receive mode, listening on a real network interface (e.g. `0.0.0.0:9876`) or loopback via `TcpListenerTransport`.
pub async fn enter_receive_mode(
    address: Option<String>,
    dest_dir: PathBuf,
) -> Result<tokio::task::JoinHandle<Result<PathBuf, TransferSessionError>>, TransferSessionError> {
    let addr = address.as_deref().unwrap_or(DEFAULT_LISTEN_ADDR);

    // 1. Idempotency check: if receive mode is already active on this address or port, reuse existing listener
    {
        let listeners = get_receive_listeners().lock();
        let already_active = listeners.contains_key(addr)
            || listeners.keys().any(|existing| {
                existing == addr
                    || (existing.ends_with(":9876") && addr.ends_with(":9876"))
            });
        if already_active {
            log::info!("Receive mode is already active on {}, returning existing listener handle", addr);
            let handle = crate::util::runtime::spawn_task(async move {
                Ok(dest_dir)
            });
            return Ok(handle);
        }
    }

    // 2. Bind TCP listener with graceful AddrInUse fallback
    let listener = match TcpListenerTransport::bind(addr).await {
        Ok(l) => l,
        Err(crate::transport::TransportError::Io(ref e))
            if e.kind() == std::io::ErrorKind::AddrInUse =>
        {
            let listeners = get_receive_listeners().lock();
            if !listeners.is_empty() {
                log::info!("Receive listener already active on {}", addr);
                let handle = crate::util::runtime::spawn_task(async move { Ok(dest_dir) });
                return Ok(handle);
            }
            return Err(TransferSessionError::Transport(
                crate::transport::TransportError::Io(std::io::Error::new(
                    e.kind(),
                    format!("Failed to bind TCP listener to {}: {}", addr, e),
                )),
            ));
        }
        Err(e) => return Err(TransferSessionError::Transport(e)),
    };

    let bound_addr = listener.local_addr()?.to_string();

    // ADB and Windows WLAN association are desktop responsibilities. Android
    // already has the local listener and must never try to execute `adb`.
    #[cfg(not(target_os = "android"))]
    {
        if address.is_none() || address.as_deref() == Some(DEFAULT_LISTEN_ADDR) {
            crate::util::runtime::spawn_task(async {
                log::info!("[ReceiveMode] Starting auto hotspot trigger and discovery...");
                let mut target_serial = None;
                if let Ok(devices) = UsbTransport::list_adb_devices() {
                    for dev in devices {
                        if dev.state == "device" {
                            let _ = UsbTransport::setup_default_adb_tunnels(&dev.serial);
                            let _ = UsbTransport::trigger_android_hotspot(&dev.serial);
                            target_serial = Some(dev.serial);
                            break;
                        }
                    }
                }

                log::info!("[ReceiveMode] Polling hotspot with target_serial={:?}...", target_serial);
                if let Some(config) = WifiDirectTransport::discover_android_hotspot(target_serial.as_deref()).await {
                    log::info!("[ReceiveMode] Discovered hotspot: SSID='{}', associating Windows WLAN...", config.ssid);
                    #[cfg(target_os = "windows")]
                    {
                        if let Err(e) = WifiDirectTransport::associate_wlan_windows(&config).await {
                            log::warn!("[ReceiveMode] Failed to associate Windows WLAN: {}", e);
                        } else {
                            log::info!("[ReceiveMode] Successfully associated Windows WLAN with '{}'!", config.ssid);
                        }
                    }
                } else {
                    log::warn!("[ReceiveMode] discover_android_hotspot returned None!");
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
                            let ip_str = peer_addr.ip().to_string();
                            let is_usb = peer_addr.ip().is_loopback()
                                || ip_str.starts_with("10.125.")
                                || ip_str.starts_with("10.104.")
                                || ip_str.starts_with("192.168.42.");
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
                            get_receive_listeners().lock().remove(&bound_addr_cleanup);
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

    get_receive_listeners().lock().insert(
        bound_addr,
        ReceiveListener {
            abort: handle.abort_handle(),
        },
    );

    Ok(handle)
}

pub(crate) async fn handle_incoming_receive_transport(
    mut transport: Box<dyn Transport>,
    is_usb: bool,
    dest_dir: PathBuf,
    completion_tx: tokio::sync::mpsc::UnboundedSender<PathBuf>,
) -> Result<(), TransferSessionError> {
    // 1. Handshake: Await Hello
    let peer_hello = match transport.receive_frame().await? {
        Some(Message::Hello(h)) => h,
        Some(other) => {
            return Err(TransferSessionError::UnexpectedMessage(format!(
                "Expected Hello, got {:?}",
                other
            )))
        }
        None => return Ok(()), // EOF / probe
    };
    if peer_hello.protocol_version < MIN_SUPPORTED_PROTOCOL_VERSION {
        return Err(TransferSessionError::Protocol(crate::protocol::ProtocolError::DeserializationError(
            format!("Unsupported protocol version: {}", peer_hello.protocol_version),
        )));
    }

    // Reply Hello
    let receiver_hello = Message::Hello(HelloData {
        device_id: Uuid::new_v4(),
        device_name: "TurboReceiver".to_string(),
        protocol_version: CURRENT_PROTOCOL_VERSION,
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
        let mut map = get_active_receive_sessions().lock();
        if let Some(existing) = map.get(&offer.transfer_id) {
            existing.clone()
        } else {
            let is_ephemeral = offer.purpose != crate::benchmark::TransferPurpose::Normal;
            let effective_dest = if is_ephemeral {
                std::env::temp_dir().join("turbotransfer_bench")
            } else {
                dest_dir.clone()
            };
            std::fs::create_dir_all(&effective_dest)?;
            let (part_path, final_path) = crate::util::storage::resolve_secure_paths(&effective_dest, &offer.file_name)?;
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

            let transport_label = match offer.purpose {
                crate::benchmark::TransferPurpose::Benchmark => "Benchmark (Receiving)".to_string(),
                crate::benchmark::TransferPurpose::Calibration => "Calibration (Receiving)".to_string(),
                crate::benchmark::TransferPurpose::Normal => "Multi-Channel Ingestion".to_string(),
            };
            register_active_transfer(
                offer.transfer_id,
                offer.file_name.clone(),
                offer.file_size,
                TransferRole::Receiver,
                offer.total_chunks,
                transport_label,
            );

            let is_sender_in_same_process = {
                let reg = get_registry();
                let reg_map = reg.transfers.lock();
                reg_map.get(&offer.transfer_id).map_or(false, |r| r.role == TransferRole::Sender)
            };

            if !is_sender_in_same_process && !is_ephemeral {
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

            let disk_error = Arc::new(Mutex::new(None));
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
                            drop(writer_file);
                            let final_res = if let Some(err_msg) = &*disk_error_clone.lock() {
                                Err(std::io::Error::new(std::io::ErrorKind::Other, err_msg.clone()))
                            } else {
                                flush_res
                            };
                            let _ = reply_tx.send(final_res);
                            break;
                        }
                    }
                }
            });

            let tracker = if !is_ephemeral {
                if let Some((_, meta)) = find_resumable_transfer(Some(offer.transfer_id)) {
                    if !meta.completed_ranges.is_empty() {
                        InMemoryChunkTracker::from_ranges(offer.transfer_id, offer.file_id, &meta.completed_ranges)
                    } else {
                        InMemoryChunkTracker::new()
                    }
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
                tracker: Arc::new(Mutex::new(tracker)),
                chunk_crcs: Arc::new(Mutex::new(HashMap::new())),
                total_chunks: offer.total_chunks,
                bytes_recv_total: Arc::new(AtomicU64::new(0)),
                completed_chunks_count: Arc::new(AtomicU32::new(0)),
                is_completed: Arc::new(AtomicBool::new(false)),
                is_sender_in_same_process,
                is_ephemeral,
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
                let is_done = session.completed_chunks_count.load(Ordering::Relaxed) >= offer.total_chunks;
                if is_done {
                    debug!("Channel {} closed gracefully after transfer completion", ch_name);
                } else {
                    session.telemetry.record_channel_disconnect(ch_name, "Peer disconnected / EOF");
                }
                break;
            }
            Err(e) => {
                let is_done = session.completed_chunks_count.load(Ordering::Relaxed) >= offer.total_chunks;
                if is_done {
                    debug!("Channel {} closed after transfer completion: {}", ch_name, e);
                } else {
                    session.telemetry.record_channel_disconnect(ch_name, &e.to_string());
                }
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

                if let Some(err) = &*session.disk_error.lock() {
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
                            let _ = std::fs::remove_file(&session.part_path);
                            get_active_receive_sessions()
                                .lock()
                                .remove(&complete_data.transfer_id);
                            return Err(TransferSessionError::Io(e));
                        }
                    }
                    if let Some(err) = &*session.disk_error.lock() {
                        session.telemetry.mark_failed(&format!("Disk write error: {}", err));
                        let data_dir = default_data_dir();
                        export_and_clean_telemetry(complete_data.transfer_id, &data_dir);
                        set_transfer_status(
                            complete_data.transfer_id,
                            TransferStatus::Failed,
                            Some(format!("Disk write error: {}", err)),
                        );
                        let _ = std::fs::remove_file(&session.part_path);
                        get_active_receive_sessions()
                            .lock()
                            .remove(&complete_data.transfer_id);
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
                        let _ = std::fs::remove_file(&session.part_path);
                        get_active_receive_sessions()
                            .lock()
                            .remove(&complete_data.transfer_id);
                        return Err(TransferSessionError::ChecksumMismatch(format!(
                            "File CRC32C mismatch: expected 0x{:08X}, got 0x{:08X}",
                            complete_data.file_checksum, file_crc
                        )));
                    }
                    if session.is_ephemeral {
                        let _ = std::fs::remove_file(&session.part_path);
                        let _ = std::fs::remove_file(&session.file_path);
                    } else {
                        std::fs::rename(&session.part_path, &session.file_path)?;
                        let sync_path = session.file_path.clone();
                        tokio::task::spawn_blocking(move || {
                            if let Ok(file) = std::fs::OpenOptions::new().write(true).open(&sync_path) {
                                let _ = file.sync_all();
                            }
                        });
                    }
                    set_transfer_status(complete_data.transfer_id, TransferStatus::Completed, None);

                    let fin_ms = t_fin0.elapsed().as_millis() as u64;
                    session.telemetry.record_finalize(fin_ms, true);
                    session.telemetry.mark_completed();
                    let data_dir = default_data_dir();
                    export_and_clean_telemetry(complete_data.transfer_id, &data_dir);

                    let _ = completion_tx.send(session.file_path.clone());
                    get_active_receive_sessions()
                        .lock()
                        .remove(&complete_data.transfer_id);
                    if session.is_ephemeral {
                        let tid = complete_data.transfer_id;
                        crate::util::runtime::spawn_task(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(4)).await;
                            get_registry().transfers.lock().remove(&tid);
                        });
                    }
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
                    .remove(&cancel_data.transfer_id);
                break;
            }
            _ => {}
        }
    }

    // Clean up active receive session if transfer is not completed and this was the last active transport channel
    if !session.is_completed.load(Ordering::SeqCst) {
        let should_cleanup = {
            let map = get_active_receive_sessions().lock();
            map.get(&offer.transfer_id).map_or(true, |s| Arc::strong_count(s) <= 2)
        };
        if should_cleanup {
            let session_opt = {
                let mut map = get_active_receive_sessions().lock();
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
    let mut listeners = get_receive_listeners().lock();
    let had_listeners;
    if let Some(addr) = address {
        if let Some(listener) = listeners.remove(addr) {
            listener.abort.abort();
            had_listeners = true;
        } else {
            let matching_keys: Vec<String> = listeners
                .keys()
                .filter(|k| k.as_str() == addr || (k.ends_with(":9876") && addr.ends_with(":9876")))
                .cloned()
                .collect();
            had_listeners = !matching_keys.is_empty();
            for k in matching_keys {
                if let Some(l) = listeners.remove(&k) {
                    l.abort.abort();
                }
            }
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

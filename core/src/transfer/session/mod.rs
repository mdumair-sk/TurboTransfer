use std::fs::{rename, OpenOptions};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use uuid::Uuid;

pub mod ack;
pub mod reader;

use ack::handle_multipath_ack_frame;
use reader::{spawn_chunk_reader, PreparedChunk};

use super::api::{
    default_data_dir, register_active_transfer, set_transfer_status, transfer_control_status,
    update_transfer_progress,
};
use super::tracker::ChunkTracker;
use crate::benchmark::{TransferPurpose, WindowPreset};
use crate::checksum::{compute_file_crc32c, compute_xxhash64};
use crate::chunk::calculate_chunk_plan;
use crate::manifest::{generate_manifest_with_name, TransferRole, TransferStatus};
use crate::protocol::{
    encode_frame, ChunkAckData, ChunkDataPayload, CompleteData, HelloData, Message,
    ProtocolError, TransferAcceptData, TransferOfferData,
};
use crate::scheduler::{
    ChannelPerformanceModel, ChannelTracker, WindowController, USB_INITIAL_WINDOW,
    WIFI_INITIAL_WINDOW,
};
use crate::transport::{StreamTransport, Transport, TransportError, TransportKind};
use crate::util::telemetry::{
    export_and_clean_telemetry, get_or_create_telemetry, EventLevel, TransferStage,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct SessionOptions {
    pub purpose: TransferPurpose,
    pub wifi_window_preset: Option<WindowPreset>,
}

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

/// Runs the sender side of a transfer session over any generic `Transport` (§6, §7, §8, §9).
/// Implements a high-throughput sliding window pipeline with concurrent in-flight chunks.
pub async fn send_file_session<T>(
    sender_device_id: Uuid,
    sender_device_name: &str,
    file_path: &Path,
    chunk_size: u32,
    transfer_id: Uuid,
    transport: T,
    custom_file_name: Option<&str>,
    is_usb_channel: Option<bool>,
) -> Result<(), TransferSessionError>
where
    T: Transport + 'static,
{
    send_file_session_ext(
        sender_device_id,
        sender_device_name,
        file_path,
        chunk_size,
        transfer_id,
        transport,
        custom_file_name,
        is_usb_channel,
        SessionOptions::default(),
    )
    .await
}

/// Extended sender session supporting custom `SessionOptions`.
/// Transparently dispatches over the unified high-throughput streaming pipeline.
pub async fn send_file_session_ext<T>(
    sender_device_id: Uuid,
    sender_device_name: &str,
    file_path: &Path,
    chunk_size: u32,
    transfer_id: Uuid,
    transport: T,
    custom_file_name: Option<&str>,
    is_usb_channel: Option<bool>,
    options: SessionOptions,
) -> Result<(), TransferSessionError>
where
    T: Transport + 'static,
{
    let is_usb = is_usb_channel.unwrap_or_else(|| transport.kind() == TransportKind::Usb);
    let boxed_transports: Vec<(Box<dyn Transport>, bool)> = vec![(Box::new(transport), is_usb)];
    send_file_session_multipath_ext(
        sender_device_id,
        sender_device_name,
        file_path,
        chunk_size,
        transfer_id,
        boxed_transports,
        custom_file_name,
        options,
    )
    .await
}

/// Runs a multipath sender transfer session over multiple generic `Transport` channels (§10).
/// Chunks are dynamically dispatched across all active channels to aggregate physical bandwidth.
pub async fn send_file_session_multipath(
    sender_device_id: Uuid,
    sender_device_name: &str,
    file_path: &Path,
    chunk_size: u32,
    transfer_id: Uuid,
    transports: Vec<(Box<dyn Transport>, bool)>,
    custom_file_name: Option<&str>,
) -> Result<(), TransferSessionError> {
    send_file_session_multipath_ext(
        sender_device_id,
        sender_device_name,
        file_path,
        chunk_size,
        transfer_id,
        transports,
        custom_file_name,
        SessionOptions::default(),
    )
    .await
}

/// Core high-performance unified streaming pipeline for $N \ge 1$ transport channels.
pub async fn send_file_session_multipath_ext(
    sender_device_id: Uuid,
    sender_device_name: &str,
    file_path: &Path,
    chunk_size: u32,
    transfer_id: Uuid,
    mut transports: Vec<(Box<dyn Transport>, bool)>,
    custom_file_name: Option<&str>,
    options: SessionOptions,
) -> Result<(), TransferSessionError> {
    if transports.is_empty() {
        return Err(TransferSessionError::Transport(TransportError::Disconnected(
            "No active transports provided for transfer".into(),
        )));
    }

    let manifest = generate_manifest_with_name(file_path, chunk_size, custom_file_name)?;
    let telemetry = get_or_create_telemetry(
        transfer_id,
        &manifest.file_name,
        manifest.file_size,
        TransferRole::Sender,
    );
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
        format!(
            "Initiating unified sender session with {} channels for '{}' ({} bytes)",
            transports.len(),
            manifest.file_name,
            manifest.file_size
        ),
        None,
    );

    // 1. Perform Hello and TransferOffer handshakes across all transports
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
                .ok_or_else(|| TransferSessionError::UnexpectedMessage("EOF during Hello".into()))?;
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
                purpose: options.purpose,
            });
            transport.send_frame(&offer).await?;

            let response = transport
                .receive_frame()
                .await?
                .ok_or_else(|| TransferSessionError::UnexpectedMessage("EOF during Offer response".into()))?;

            match response {
                Message::TransferAccept(accept) => Ok(accept.resume_from),
                Message::TransferReject(reject) => Err(TransferSessionError::Rejected(reject.reason)),
                other => Err(TransferSessionError::UnexpectedMessage(format!(
                    "Expected Accept or Reject, got {:?}",
                    other
                ))),
            }
        }
        .await;

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
                    format!(
                        "Channel-{} ({}) handshake failed, skipping: {}",
                        idx + 1,
                        ch_name,
                        e
                    ),
                    None,
                );
                log::warn!(
                    "Channel-{} ({}) handshake failed: {}",
                    idx + 1,
                    ch_name,
                    e
                );
                failed_indices.push(idx);
            }
        }
    }

    for &idx in failed_indices.iter().rev() {
        transports.remove(idx);
    }

    if transports.is_empty() {
        return Err(TransferSessionError::Transport(TransportError::Disconnected(
            "All candidate transport handshakes failed".into(),
        )));
    }

    // 2. Filter already transferred chunks (cold resume)
    let missing_chunks = if !resume_ranges_combined.is_empty() {
        let mut completed_set = std::collections::HashSet::new();
        for &(start, end) in &resume_ranges_combined {
            for cid in start..=end {
                completed_set.insert(cid);
            }
        }
        let missing: std::collections::HashSet<u32> = plan
            .iter()
            .map(|e| e.chunk_id)
            .filter(|cid| !completed_set.contains(cid))
            .collect();
        missing
    } else {
        plan.iter().map(|e| e.chunk_id).collect()
    };

    let mut initial_chunks_to_send = std::collections::VecDeque::new();
    let mut completed_set_init = std::collections::HashSet::new();
    let mut bytes_sent_total_init: u64 = 0;
    let mut completed_chunks_count_init: u32 = 0;

    for entry in &plan {
        let cid = entry.chunk_id;
        if missing_chunks.contains(&cid) {
            initial_chunks_to_send.push_back(entry.clone());
        } else {
            bytes_sent_total_init += entry.payload_length as u64;
            completed_chunks_count_init += 1;
            completed_set_init.insert(cid);
        }
    }

    update_transfer_progress(transfer_id, bytes_sent_total_init, completed_chunks_count_init);

    let total_chunks_needed = initial_chunks_to_send.len();
    if total_chunks_needed == 0 {
        let t_fin0 = std::time::Instant::now();
        let file_checksum = compute_file_crc32c(file_path)?;
        let complete_msg = Message::Complete(CompleteData {
            transfer_id,
            file_checksum,
        });
        transports[0].0.send_frame(&complete_msg).await?;
        let final_frame = transports[0]
            .0
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

    let (prepared_tx, prepared_rx) = async_channel::bounded::<PreparedChunk>(256);
    let (retry_tx, retry_rx) = std::sync::mpsc::channel::<crate::chunk::ChunkPlanEntry>();
    let (recycle_tx, recycle_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let is_cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (running_crc_tx, running_crc_rx) = tokio::sync::oneshot::channel::<u32>();
    let total_plan_chunks = plan.len();

    let reader_file_path = file_path.to_path_buf();
    let reader_cancelled = std::sync::Arc::clone(&is_cancelled);
    let chunk_size_bytes = manifest.chunk_size as usize;
    let resume_ranges_cloned = resume_ranges_combined.clone();
    let plan_map_for_reader = plan_map.clone();
    let tel_reader = telemetry.clone();

    let reader_handle = spawn_chunk_reader(
        reader_file_path,
        chunk_size_bytes,
        initial_chunks_to_send,
        resume_ranges_cloned,
        plan_map_for_reader,
        total_plan_chunks,
        prepared_tx,
        retry_rx,
        recycle_rx,
        reader_cancelled,
        running_crc_tx,
        tel_reader,
    );

    let shared_retry_tx = std::sync::Arc::new(retry_tx);
    let shared_recycle_tx = std::sync::Arc::new(recycle_tx);
    let shared_completed = std::sync::Arc::new(parking_lot::Mutex::new(completed_set_init));
    let shared_completed_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(
        completed_chunks_count_init as usize,
    ));
    let shared_bytes_sent =
        std::sync::Arc::new(std::sync::atomic::AtomicU64::new(bytes_sent_total_init));
    let shared_chunks_done =
        std::sync::Arc::new(std::sync::atomic::AtomicU32::new(completed_chunks_count_init));
    let shared_plan_map = std::sync::Arc::new(plan_map);

    let mut worker_handles = Vec::new();
    let global_notify = std::sync::Arc::new(tokio::sync::Notify::new());

    for (idx, (transport, is_usb)) in transports.into_iter().enumerate() {
        let (writer, reader) = transport.split_boxed()?;
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

        let wifi_window_preset = options.wifi_window_preset;
        let global_notify_worker = global_notify.clone();
        let handle = tokio::spawn(async move {
            let tracker = std::sync::Arc::new(parking_lot::Mutex::new(ChannelTracker::new(
                channel_name.clone(),
            )));
            let init_win = if is_usb {
                USB_INITIAL_WINDOW
            } else if let Some(preset) = wifi_window_preset {
                preset.to_thresholds().2
            } else {
                WIFI_INITIAL_WINDOW
            };
            let current_window = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(init_win));
            let in_flight_times = std::sync::Arc::new(parking_lot::Mutex::new(
                std::collections::HashMap::<u32, std::time::Instant>::new(),
            ));
            let last_socket_send_us =
                std::sync::Arc::new(std::sync::atomic::AtomicU64::new(1_000));
            let channel_notify = std::sync::Arc::new(tokio::sync::Notify::new());
            let channel_disconnected =
                std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let actor_handle = crate::transfer::api::get_transfer_actor_handle(transfer_id);

            // Dedicated async transmit loop
            let tx_writer_task = {
                let mut writer = writer;
                let tracker = tracker.clone();
                let current_window = current_window.clone();
                let in_flight_times = in_flight_times.clone();
                let last_socket_send_us = last_socket_send_us.clone();
                let channel_notify = channel_notify.clone();
                let global_notify = global_notify_worker.clone();
                let channel_disconnected = channel_disconnected.clone();
                let cancelled = cancelled.clone();
                let completed_count = completed_count.clone();
                let retry_tx = retry_tx.clone();
                let recycle_tx = recycle_tx.clone();
                let plan_map = plan_map.clone();
                let telemetry_worker = telemetry_worker.clone();
                let channel_name = channel_name.clone();
                let prepared_rx = prepared_rx.clone();

                tokio::spawn(async move {
                    loop {
                        if cancelled.load(std::sync::atomic::Ordering::Relaxed)
                            || channel_disconnected.load(std::sync::atomic::Ordering::Relaxed)
                            || completed_count.load(std::sync::atomic::Ordering::Relaxed) >= total_chunks
                        {
                            break;
                        }

                        match transfer_control_status(transfer_id) {
                            Some(TransferStatus::Paused) => {
                                cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                                channel_disconnected.store(true, std::sync::atomic::Ordering::Relaxed);
                                prepared_rx.close();
                                channel_notify.notify_waiters();
                                global_notify.notify_waiters();
                                if let Some(ref tel) = telemetry_worker {
                                    tel.record_event(
                                        TransferStage::Control,
                                        EventLevel::Info,
                                        &channel_name,
                                        None,
                                        None,
                                        None,
                                        "Transfer paused by user",
                                        None,
                                    );
                                }
                                return Err(TransferSessionError::Paused);
                            }
                            Some(TransferStatus::Cancelled) => {
                                cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                                channel_disconnected.store(true, std::sync::atomic::Ordering::Relaxed);
                                prepared_rx.close();
                                channel_notify.notify_waiters();
                                global_notify.notify_waiters();
                                if let Some(ref tel) = telemetry_worker {
                                    tel.record_event(
                                        TransferStage::Control,
                                        EventLevel::Info,
                                        &channel_name,
                                        None,
                                        None,
                                        None,
                                        "Transfer cancelled by user",
                                        None,
                                    );
                                }
                                return Err(TransferSessionError::Cancelled);
                            }
                            _ => {}
                        }

                        // Window capacity throttling
                        let win = current_window.load(std::sync::atomic::Ordering::Relaxed);
                        let in_flight = tracker.lock().in_flight_count();
                        if in_flight >= win {
                            tokio::select! {
                                _ = channel_notify.notified() => continue,
                                _ = global_notify.notified() => continue,
                                _ = tokio::time::sleep(tokio::time::Duration::from_millis(50)) => continue,
                            }
                        }

                        tokio::select! {
                            _ = global_notify.notified() => {
                                if cancelled.load(std::sync::atomic::Ordering::Relaxed)
                                    || completed_count.load(std::sync::atomic::Ordering::Relaxed) >= total_chunks
                                    || channel_disconnected.load(std::sync::atomic::Ordering::Relaxed)
                                {
                                    break;
                                }
                                continue;
                            }
                            prepared_res = prepared_rx.recv() => {
                                match prepared_res {
                                    Ok(prepared) => {
                                        let cid = prepared.entry.chunk_id;
                                        let payload_len = prepared.entry.payload_length;
                                        let offset = prepared.entry.file_offset;

                                        let chunk_msg = Message::ChunkData(ChunkDataPayload {
                                            transfer_id,
                                            file_id,
                                            chunk_id: cid,
                                            file_offset: offset,
                                            payload_length: payload_len,
                                            checksum: prepared.checksum,
                                            payload: prepared.payload,
                                        });

                                        let t_send0 = std::time::Instant::now();
                                        tracker.lock().record_chunk_sent(cid, payload_len as u64);
                                        in_flight_times.lock().insert(cid, t_send0);

                                        let send_res = writer.send_frame(&chunk_msg).await;
                                        let send_us = t_send0.elapsed().as_micros() as u64;
                                        last_socket_send_us.store(send_us, std::sync::atomic::Ordering::Relaxed);

                                        // Recycle payload buffer
                                        if let Message::ChunkData(data) = chunk_msg {
                                            let _ = recycle_tx.send(data.payload);
                                        }

                                        if let Err(e) = send_res {
                                            channel_disconnected.store(true, std::sync::atomic::Ordering::Relaxed);
                                            channel_notify.notify_waiters();
                                            global_notify.notify_waiters();
                                            tracker.lock().record_disconnect(&e.to_string());
                                            in_flight_times.lock().remove(&cid);
                                            if let Some(e_entry) = plan_map.get(&cid) {
                                                let _ = retry_tx.send(e_entry.clone());
                                            }
                                            if let Some(ref tel) = telemetry_worker {
                                                tel.record_channel_disconnect(&channel_name, &e.to_string());
                                            }
                                            log::warn!("Multipath writer on {} error -> requeued chunk #{}", channel_name, cid);
                                            return Ok((writer, false));
                                        }

                                        if let Some(ref tel) = telemetry_worker {
                                            tel.record_chunk_sent(&channel_name, cid, payload_len as u64, send_us);
                                        }
                                    }
                                    Err(_) => {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Ok((writer, true))
                })
            };

            // Dedicated async receive loop
            let rx_reader_task = {
                let mut reader = reader;
                let tracker = tracker.clone();
                let current_window = current_window.clone();
                let in_flight_times = in_flight_times.clone();
                let last_socket_send_us = last_socket_send_us.clone();
                let channel_notify = channel_notify.clone();
                let global_notify = global_notify_worker.clone();
                let channel_disconnected = channel_disconnected.clone();
                let cancelled = cancelled.clone();
                let completed = completed.clone();
                let completed_count = completed_count.clone();
                let retry_tx = retry_tx.clone();
                let bytes_sent = bytes_sent.clone();
                let chunks_done = chunks_done.clone();
                let plan_map = plan_map.clone();
                let telemetry_worker = telemetry_worker.clone();
                let channel_name = channel_name.clone();
                let prepared_rx = prepared_rx.clone();

                tokio::spawn(async move {
                    let mut model = ChannelPerformanceModel::new(
                        channel_name.clone(),
                        if is_usb { 45.0 } else { 20.0 },
                    );
                    let mut window = if is_usb {
                        WindowController::for_usb()
                    } else if let Some(preset) = wifi_window_preset {
                        let (min, max, init, bp, rtt) = preset.to_thresholds();
                        WindowController::with_thresholds(min, max, init, bp, rtt)
                    } else {
                        WindowController::for_wifi()
                    };

                    loop {
                        if channel_disconnected.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                        if completed_count.load(std::sync::atomic::Ordering::Relaxed) >= total_chunks {
                            break;
                        }
                        if cancelled.load(std::sync::atomic::Ordering::Relaxed)
                            && tracker.lock().in_flight_count() == 0
                        {
                            break;
                        }

                        tokio::select! {
                            _ = global_notify.notified() => {
                                if channel_disconnected.load(std::sync::atomic::Ordering::Relaxed)
                                    || completed_count.load(std::sync::atomic::Ordering::Relaxed) >= total_chunks
                                    || (cancelled.load(std::sync::atomic::Ordering::Relaxed) && tracker.lock().in_flight_count() == 0)
                                {
                                    break;
                                }
                            }
                            frame_res = reader.receive_frame() => {
                                match frame_res {
                                    Ok(Some(frame)) => {
                                        let mut tr = tracker.lock();
                                        let mut times = in_flight_times.lock();
                                        let socket_dur = last_socket_send_us.load(std::sync::atomic::Ordering::Relaxed);
                                        handle_multipath_ack_frame(
                                            frame,
                                            is_usb,
                                            &mut tr,
                                            &mut model,
                                            &mut window,
                                            &mut times,
                                            &completed,
                                            &completed_count,
                                            &plan_map,
                                            &retry_tx,
                                            transfer_id,
                                            &bytes_sent,
                                            &chunks_done,
                                            telemetry_worker.as_ref(),
                                            &channel_name,
                                            socket_dur,
                                            actor_handle.as_ref(),
                                        )?;
                                        current_window.store(window.current_window, std::sync::atomic::Ordering::Relaxed);
                                        let in_flight_now = tr.in_flight_count();
                                        drop(times);
                                        drop(tr);
                                        channel_notify.notify_waiters();

                                        if completed_count.load(std::sync::atomic::Ordering::Relaxed) >= total_chunks {
                                            cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                                            prepared_rx.close();
                                            channel_notify.notify_waiters();
                                            global_notify.notify_waiters();
                                            break;
                                        }
                                        if cancelled.load(std::sync::atomic::Ordering::Relaxed) && in_flight_now == 0 {
                                            channel_notify.notify_waiters();
                                            global_notify.notify_waiters();
                                            break;
                                        }
                                    }
                                    Ok(None) | Err(_) => {
                                        channel_disconnected.store(true, std::sync::atomic::Ordering::Relaxed);
                                        channel_notify.notify_waiters();
                                        global_notify.notify_waiters();
                                        let in_flight_cids: Vec<u32> = {
                                            let mut tr = tracker.lock();
                                            tr.record_disconnect("Transport disconnected / EOF");
                                            tr.in_flight_chunks.drain().collect()
                                        };
                                        let mut times = in_flight_times.lock();
                                        for cid in in_flight_cids {
                                            times.remove(&cid);
                                            if let Some(e) = plan_map.get(&cid) {
                                                let _ = retry_tx.send(e.clone());
                                            }
                                        }
                                        if let Some(ref tel) = telemetry_worker {
                                            tel.record_channel_disconnect(&channel_name, "Transport disconnected / EOF");
                                        }
                                        log::warn!("Transport ({}) disconnected -> requeued in-flight chunks", channel_name);
                                        return Ok((reader, false));
                                    }
                                }
                            }
                        }
                    }
                    Ok((reader, true))
                })
            };

            let (tx_res, rx_res) = tokio::join!(tx_writer_task, rx_reader_task);

            match (tx_res, rx_res) {
                (Ok(Ok((writer, tx_alive))), Ok(Ok((reader, rx_alive)))) => {
                    let is_alive = tx_alive && rx_alive;
                    Ok((idx, (writer, reader), is_alive))
                }
                (Ok(Err(e)), _) => Err(e),
                (_, Ok(Err(e))) => Err(e),
                _ => Err(TransferSessionError::Transport(TransportError::Disconnected(
                    "Worker task aborted".into(),
                ))),
            }
        });

        worker_handles.push(handle);
    }

    let mut returned_transports = Vec::new();
    for handle in worker_handles {
        if let Ok(Ok((_idx, pair, is_alive))) = handle.await {
            if is_alive {
                returned_transports.push(pair);
            }
        }
    }

    is_cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    prepared_rx.close();
    drop(shared_retry_tx);
    drop(shared_recycle_tx);
    if let Ok(Err(e)) = reader_handle.await {
        telemetry.mark_failed(&format!("Source file read error: {}", e));
        let data_dir = default_data_dir();
        export_and_clean_telemetry(transfer_id, &data_dir);
        return Err(TransferSessionError::Io(e));
    }

    let final_done = {
        let c = shared_completed.lock();
        c.len()
    };

    if final_done < plan.len() {
        if transfer_control_status(transfer_id) == Some(TransferStatus::Paused) {
            return Err(TransferSessionError::Paused);
        }
        if transfer_control_status(transfer_id) == Some(TransferStatus::Cancelled) {
            return Err(TransferSessionError::Cancelled);
        }
        telemetry.mark_failed("All transports disconnected before completing transfer");
        let data_dir = default_data_dir();
        export_and_clean_telemetry(transfer_id, &data_dir);
        return Err(TransferSessionError::Transport(TransportError::Disconnected(
            "All transports disconnected before completing transfer".into(),
        )));
    }

    // 3. Complete transfer on the first surviving transport
    let (mut primary_writer, mut primary_reader) = returned_transports.into_iter().next().ok_or_else(|| {
        telemetry.mark_failed("No surviving transport available to send Complete message");
        let data_dir = default_data_dir();
        export_and_clean_telemetry(transfer_id, &data_dir);
        TransferSessionError::Transport(TransportError::Disconnected(
            "All transports disconnected before completion finalization could be sent".into(),
        ))
    })?;

    let t_fin0 = std::time::Instant::now();
    let file_checksum = match running_crc_rx.await {
        Ok(c) => c,
        Err(_) => compute_file_crc32c(file_path)?,
    };
    let complete_msg = Message::Complete(CompleteData {
        transfer_id,
        file_checksum,
    });
    primary_writer.send_frame(&complete_msg).await?;
    loop {
        let final_frame = primary_reader
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
    let _ = primary_writer.close().await;

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
    send_file_session(
        sender_device_id,
        sender_device_name,
        file_path,
        chunk_size,
        transfer_id,
        StreamTransport::new(stream, TransportKind::Tcp),
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
    let (part_path, final_path) = crate::util::storage::resolve_secure_paths(dest_dir, &offer.file_name)?;

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&part_path)?;
    crate::util::storage::preallocate_file(&file, offer.file_size)?;

    let mut chunk_crcs: std::collections::HashMap<u32, (u32, usize)> = std::collections::HashMap::new();

    // Pre-calculate CRC for resumed chunks from open file handle directly
    if let Some(ranges) = &resume_from {
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

    let mut bytes_received: u64 = 0;
    let mut chunks_completed: u32 = 0;

    // 6. Receive Chunks
    loop {
        let frame_msg = transport.receive_frame().await?;
        let frame = match frame_msg {
            Some(f) => f,
            None => {
                telemetry.record_channel_disconnect("Receiver", "Peer disconnected / EOF");
                break;
            }
        };

        match frame {
            Message::ChunkData(chunk_data) => {
                let t_v0 = std::time::Instant::now();
                let computed = compute_xxhash64(&chunk_data.payload);
                let verify_us = t_v0.elapsed().as_micros() as u64;

                if computed != chunk_data.checksum {
                    telemetry.record_chunk_nack("Receiver", chunk_data.chunk_id, "xxHash64 payload mismatch");
                    let nack = Message::ChunkNack(crate::protocol::ChunkNackData {
                        transfer_id: chunk_data.transfer_id,
                        chunk_id: chunk_data.chunk_id,
                        reason: "xxHash64 checksum mismatch".to_string(),
                    });
                    transport.send_frame(&nack).await?;
                    continue;
                }

                telemetry.record_chunk_recv(
                    "Receiver",
                    chunk_data.chunk_id,
                    chunk_data.payload_length as u64,
                    0,
                    verify_us,
                );

                if tracker.is_chunk_completed(
                    chunk_data.transfer_id,
                    chunk_data.file_id,
                    chunk_data.chunk_id,
                    chunk_data.checksum,
                ) {
                    telemetry.record_duplicate_chunk(chunk_data.chunk_id);
                    let ack = Message::ChunkAck(ChunkAckData {
                        transfer_id: chunk_data.transfer_id,
                        chunk_id: chunk_data.chunk_id,
                        receiver_verify_us: Some(verify_us as u32),
                    });
                    transport.send_frame(&ack).await?;
                    continue;
                }

                use std::io::{Seek, SeekFrom, Write};
                let t_w0 = std::time::Instant::now();
                file.seek(SeekFrom::Start(chunk_data.file_offset))?;
                file.write_all(&chunk_data.payload)?;
                let write_us = t_w0.elapsed().as_micros() as u64;

                telemetry.record_disk_write(
                    chunk_data.chunk_id,
                    chunk_data.payload_length as u64,
                    write_us,
                    0,
                );

                let chunk_crc = crate::checksum::compute_crc32c(&chunk_data.payload);
                chunk_crcs.insert(chunk_data.chunk_id, (chunk_crc, chunk_data.payload.len()));

                tracker.mark_chunk_completed(
                    chunk_data.transfer_id,
                    chunk_data.file_id,
                    chunk_data.chunk_id,
                    chunk_data.checksum,
                );

                bytes_received += chunk_data.payload_length as u64;
                chunks_completed += 1;
                update_transfer_progress(chunk_data.transfer_id, bytes_received, chunks_completed);

                let ack = Message::ChunkAck(ChunkAckData {
                    transfer_id: chunk_data.transfer_id,
                    chunk_id: chunk_data.chunk_id,
                    receiver_verify_us: Some(verify_us as u32),
                });
                transport.send_frame(&ack).await?;
            }
            Message::Complete(complete_data) => {
                let t_fin0 = std::time::Instant::now();
                use std::io::Write;
                file.flush()?;
                drop(file);

                // In-Flight O(1) Checksum calculation via GF(2) matrix CRC32C combination
                let file_crc = if chunk_crcs.len() == offer.total_chunks as usize {
                    let mut acc = crate::checksum::Crc32cAccumulator::new();
                    for cid in 0..offer.total_chunks {
                        if let Some(&(crc, len)) = chunk_crcs.get(&cid) {
                            acc.combine(crc, len);
                        }
                    }
                    acc.finalize()
                } else {
                    compute_file_crc32c(&part_path)?
                };

                if file_crc != complete_data.file_checksum {
                    telemetry.mark_failed(&format!(
                        "CRC32C mismatch: expected 0x{:08X}, got 0x{:08X}",
                        complete_data.file_checksum, file_crc
                    ));
                    let data_dir = default_data_dir();
                    export_and_clean_telemetry(complete_data.transfer_id, &data_dir);
                    set_transfer_status(
                        complete_data.transfer_id,
                        TransferStatus::Failed,
                        Some("CRC32C checksum mismatch".to_string()),
                    );
                    return Err(TransferSessionError::ChecksumMismatch(format!(
                        "CRC32C expected 0x{:08X}, got 0x{:08X}",
                        complete_data.file_checksum, file_crc
                    )));
                }

                rename(&part_path, &final_path)?;
                set_transfer_status(complete_data.transfer_id, TransferStatus::Completed, None);

                let fin_ms = t_fin0.elapsed().as_millis() as u64;
                telemetry.record_finalize(fin_ms, true);
                telemetry.mark_completed();
                let data_dir = default_data_dir();
                export_and_clean_telemetry(complete_data.transfer_id, &data_dir);

                let ack = Message::ChunkAck(ChunkAckData {
                    transfer_id: complete_data.transfer_id,
                    chunk_id: u32::MAX,
                    receiver_verify_us: None,
                });
                transport.send_frame(&ack).await?;
                return Ok(final_path);
            }
            Message::Pause(pause_data) => {
                telemetry.record_event(
                    TransferStage::Control,
                    EventLevel::Info,
                    "Receiver",
                    None,
                    None,
                    None,
                    "Receiver received Pause",
                    None,
                );
                set_transfer_status(pause_data.transfer_id, TransferStatus::Paused, None);
                let ack = Message::ChunkAck(ChunkAckData {
                    transfer_id: pause_data.transfer_id,
                    chunk_id: u32::MAX - 1,
                    receiver_verify_us: None,
                });
                transport.send_frame(&ack).await?;
            }
            Message::Resume(resume_data) => {
                telemetry.record_event(
                    TransferStage::Control,
                    EventLevel::Info,
                    "Receiver",
                    None,
                    None,
                    None,
                    "Receiver received Resume",
                    None,
                );
                set_transfer_status(resume_data.transfer_id, TransferStatus::InProgress, None);
                let ack = Message::ChunkAck(ChunkAckData {
                    transfer_id: resume_data.transfer_id,
                    chunk_id: u32::MAX - 2,
                    receiver_verify_us: None,
                });
                transport.send_frame(&ack).await?;
            }
            Message::Cancel(cancel_data) => {
                telemetry.record_event(
                    TransferStage::Control,
                    EventLevel::Info,
                    "Receiver",
                    None,
                    None,
                    None,
                    "Receiver received Cancel",
                    None,
                );
                telemetry.mark_failed("Transfer cancelled by peer");
                let data_dir = default_data_dir();
                export_and_clean_telemetry(cancel_data.transfer_id, &data_dir);
                set_transfer_status(cancel_data.transfer_id, TransferStatus::Cancelled, None);
                let _ = std::fs::remove_file(&part_path);
                return Err(TransferSessionError::Cancelled);
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

    Ok(final_path)
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
    receive_file_session(
        receiver_device_id,
        receiver_device_name,
        dest_dir,
        tracker,
        StreamTransport::new(stream, TransportKind::Tcp),
    )
    .await
}

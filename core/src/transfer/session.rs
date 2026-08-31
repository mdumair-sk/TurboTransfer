use std::fs::{rename, OpenOptions};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use uuid::Uuid;

use super::api::{
    default_data_dir, register_active_transfer, set_transfer_status, transfer_control_status,
    update_transfer_progress,
};
use super::tracker::{ChunkTracker, InMemoryChunkTracker};
use crate::checksum::{compute_file_crc32c, compute_xxhash64};
use crate::chunk::{calculate_chunk_plan, read_chunk_at};
use crate::manifest::{generate_manifest_with_name, TransferRole, TransferStatus};
use crate::protocol::{
    encode_frame, ChunkAckData, ChunkDataPayload, ChunkNackData, CompleteData,
    HelloData, Message, ProtocolError, TransferAcceptData, TransferOfferData,
};
use crate::scheduler::{ChannelPerformanceModel, ChannelTracker, WindowController};
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
                if let Some(actor) = crate::transfer::api::get_transfer_actor_handle(transfer_id) {
                    let t_type = if is_usb { crate::manifest::TransportType::Usb } else { crate::manifest::TransportType::WifiDirect };
                    actor.try_send_chunk_completed(ack.chunk_id, t_type, bytes_len);
                }
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
                    if let Some(actor) = crate::transfer::api::get_transfer_actor_handle(transfer_id) {
                        let t_type = if is_usb { crate::manifest::TransportType::Usb } else { crate::manifest::TransportType::WifiDirect };
                        actor.try_send_chunk_completed(cid, t_type, bytes_len);
                    }
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
                // Pipeline full -> await ACK from receiver to free slot with 15s timeout
                let frame = tokio::time::timeout(tokio::time::Duration::from_secs(15), transport.receive_frame()).await
                    .map_err(|_| TransferSessionError::Transport(TransportError::Disconnected("Timeout (15s) awaiting ChunkAck from receiver".into())))??
                    .ok_or_else(|| {
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
        if let Ok(Err(e)) = reader_handle.await {
            telemetry.mark_failed(&format!("Source file read error: {}", e));
            return Err(TransferSessionError::Io(e));
        }
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
    tracker: &mut ChannelTracker,
    model: &mut ChannelPerformanceModel,
    window: &mut WindowController,
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
    last_socket_duration_us: u64,
) -> Result<(), TransferSessionError> {
    match frame {
        Message::ChunkAck(ack) => {
            let bytes_len = plan_map.get(&ack.chunk_id).map_or(0, |e| e.payload_length as u64);
            if let Some(t_disp) = worker_in_flight_times.remove(&ack.chunk_id) {
                let rtt_us = t_disp.elapsed().as_micros() as u64;
                let rtt_ms = rtt_us as f64 / 1000.0;
                if let Some(tel) = telemetry {
                    tel.record_chunk_ack(channel_name, ack.chunk_id, rtt_ms, bytes_len);
                }
                if let Some(sample) = tracker.record_chunk_ack(
                    ack.chunk_id,
                    bytes_len,
                    rtt_us,
                    last_socket_duration_us,
                    ack.receiver_verify_us,
                ) {
                    model.update_from_tracker_and_sample(tracker, &sample);
                    window.evaluate_and_adjust(tracker, model);
                }
            }

            let is_new = completed.lock().insert(ack.chunk_id);
            if is_new {
                if let Some(actor) = crate::transfer::api::get_transfer_actor_handle(transfer_id) {
                    let t_type = if is_usb { crate::manifest::TransportType::Usb } else { crate::manifest::TransportType::WifiDirect };
                    actor.try_send_chunk_completed(ack.chunk_id, t_type, bytes_len);
                }
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
                let bytes_len = plan_map.get(&cid).map_or(0, |e| e.payload_length as u64);
                if let Some(t_disp) = worker_in_flight_times.remove(&cid) {
                    let rtt_us = t_disp.elapsed().as_micros() as u64;
                    let rtt_ms = rtt_us as f64 / 1000.0;
                    if let Some(tel) = telemetry {
                        tel.record_chunk_ack(channel_name, cid, rtt_ms, bytes_len);
                    }
                    if let Some(sample) = tracker.record_chunk_ack(
                        cid,
                        bytes_len,
                        rtt_us,
                        last_socket_duration_us,
                        None,
                    ) {
                        model.update_from_tracker_and_sample(tracker, &sample);
                    }
                }

                let is_new = completed.lock().insert(cid);
                if is_new {
                    if let Some(actor) = crate::transfer::api::get_transfer_actor_handle(transfer_id) {
                        let t_type = if is_usb { crate::manifest::TransportType::Usb } else { crate::manifest::TransportType::WifiDirect };
                        actor.try_send_chunk_completed(cid, t_type, bytes_len);
                    }
                    completed_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if let Some(entry) = plan_map.get(&cid) {
                        let total_b = bytes_sent.fetch_add(entry.payload_length as u64, std::sync::atomic::Ordering::Relaxed) + entry.payload_length as u64;
                        let total_c = chunks_done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                        update_transfer_progress(transfer_id, total_b, total_c);
                        crate::transfer::api::record_channel_bytes(transfer_id, is_usb, entry.payload_length as u64);
                    }
                }
            }
            window.evaluate_and_adjust(tracker, model);
        }
        Message::ChunkNack(nack) => {
            tracker.record_chunk_nack(nack.chunk_id, &nack.reason);
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

    // 2. Data Plane: Shared state across all transports using InMemoryChunkTracker
    let chunk_tracker = InMemoryChunkTracker::from_ranges(transfer_id, manifest.file_id, &resume_ranges_combined);
    let missing_chunks: std::collections::HashSet<u32> = chunk_tracker
        .get_missing_chunks(manifest.file_size, manifest.chunk_size)
        .into_iter()
        .collect();

    let mut initial_chunks_to_send = std::collections::VecDeque::new();
    let mut bytes_sent_total_init = 0u64;
    let mut completed_chunks_count_init = 0u32;
    let mut completed_set_init = std::collections::HashSet::new();

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

    let (prepared_tx, prepared_rx) = async_channel::bounded::<PreparedChunk>(64);
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
            let mut tracker = ChannelTracker::new(channel_name.clone());
            let mut model = ChannelPerformanceModel::new(channel_name.clone(), if is_usb { 45.0 } else { 20.0 });
            let mut window = if is_usb { WindowController::for_usb() } else { WindowController::for_wifi() };
            let mut worker_in_flight_times = std::collections::HashMap::new();
            let mut last_socket_send_us: u64 = 1_000;

            loop {
                if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                    if tracker.in_flight_count() == 0 {
                        break;
                    }
                }

                if completed_count.load(std::sync::atomic::Ordering::Relaxed) >= total_chunks {
                    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                    prepared_rx.close();
                    if tracker.in_flight_count() == 0 {
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
                                    &mut tracker,
                                    &mut model,
                                    &mut window,
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
                                    last_socket_send_us,
                                ).await?;
                                if completed_count.load(std::sync::atomic::Ordering::Relaxed) >= total_chunks {
                                    cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                                    prepared_rx.close();
                                    break;
                                }
                            }
                            Ok(None) | Err(_) => {
                                // Transport disconnected
                                tracker.record_disconnect("Transport disconnected / EOF");
                                for cid in tracker.in_flight_chunks.drain() {
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
                    prepared_res = prepared_rx.recv(), if tracker.in_flight_count() < window.current_window && !cancelled.load(std::sync::atomic::Ordering::Relaxed) => {
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
                                    tracker.record_disconnect(&format!("Send error: {}", e));
                                    let _ = retry_tx.send(prepared.entry);
                                    for cid in tracker.in_flight_chunks.drain() {
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
                                last_socket_send_us = send_us;
                                tracker.record_chunk_sent(chunk_id, prepared.entry.payload_length as u64);
                                if let Some(ref tel) = telemetry_worker {
                                    tel.record_chunk_sent(&channel_name, chunk_id, prepared.entry.payload_length as u64, send_us);
                                }
                                worker_in_flight_times.insert(chunk_id, std::time::Instant::now());

                                if let Message::ChunkData(d) = chunk_msg {
                                    let _ = recycle_tx.send(d.payload);
                                }
                            }
                            Err(_) => {
                                // Channel closed (reader finished or encountered error)
                                if tracker.in_flight_count() == 0 {
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
        telemetry.mark_failed("All multipath transports disconnected before completing transfer");
        let data_dir = default_data_dir();
        export_and_clean_telemetry(transfer_id, &data_dir);
        return Err(TransferSessionError::Transport(TransportError::Disconnected(
            "All multipath transports disconnected before completing transfer".into(),
        )));
    }

    // 3. Complete transfer on the first surviving transport
    let mut primary_transport = returned_transports.into_iter().next().ok_or_else(|| {
        telemetry.mark_failed("No surviving transport available to send Complete message");
        let data_dir = default_data_dir();
        export_and_clean_telemetry(transfer_id, &data_dir);
        TransferSessionError::Transport(TransportError::Disconnected(
            "All multipath transports disconnected before completion finalization could be sent".into(),
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
    let (part_path, final_path) = crate::util::storage::resolve_secure_paths(dest_dir, &offer.file_name)?;

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
                if let Some(actor) = crate::transfer::api::get_transfer_actor_handle(chunk_data.transfer_id) {
                    let t_type = if is_usb { crate::manifest::TransportType::Usb } else { crate::manifest::TransportType::WifiDirect };
                    actor.try_send_chunk_completed(chunk_data.chunk_id, t_type, chunk_len);
                }

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

    if transfer_control_status(offer.transfer_id) == Some(TransferStatus::Paused) {
        return Err(TransferSessionError::Paused);
    }
    if transfer_control_status(offer.transfer_id) == Some(TransferStatus::Cancelled) {
        return Err(TransferSessionError::Cancelled);
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

use std::fs::{rename, OpenOptions};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use uuid::Uuid;

use super::api::{
    register_active_transfer, set_transfer_status, transfer_control_status, update_transfer_progress,
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
    completed_set: &mut std::collections::HashSet<u32>,
    plan_map: &std::collections::HashMap<u32, crate::chunk::ChunkPlanEntry>,
    retry_tx: &std::sync::mpsc::Sender<crate::chunk::ChunkPlanEntry>,
    transfer_id: Uuid,
    bytes_sent_total: &mut u64,
    completed_chunks_count: &mut u32,
) -> Result<(), TransferSessionError> {
    match frame {
        Message::ChunkAck(ack) => {
            in_flight.remove(&ack.chunk_id);
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
) -> Result<(), TransferSessionError>
where
    T: Transport,
{
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

    // 3. Send TransferOffer
    let manifest = generate_manifest_with_name(file_path, chunk_size, custom_file_name)?;
    let offer = Message::TransferOffer(TransferOfferData {
        transfer_id,
        file_id: manifest.file_id,
        file_name: manifest.file_name,
        file_size: manifest.file_size,
        chunk_size: manifest.chunk_size,
        total_chunks: manifest.total_chunks,
        checksum_algo: "xxhash64".to_string(),
    });
    transport.send_frame(&offer).await?;

    // 4. Await TransferAccept / TransferReject
    let response = transport
        .receive_frame()
        .await?
        .ok_or(TransferSessionError::UnexpectedMessage(
            "EOF during Offer response".into(),
        ))?;

    let resume_ranges = match response {
        Message::TransferAccept(accept) => accept.resume_from,
        Message::TransferReject(reject) => {
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
    let total_chunks_needed = plan_map.len();
    let is_usb = transport.kind() == crate::transport::TransportKind::Usb || transport.kind() == crate::transport::TransportKind::Tcp;

    if completed_set.len() < total_chunks_needed {
        let (chunk_tx, mut chunk_rx) = tokio::sync::mpsc::channel::<PreparedChunk>(8);
        let (retry_tx, retry_rx) = std::sync::mpsc::channel::<crate::chunk::ChunkPlanEntry>();
        let (recycle_tx, recycle_rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let is_cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let reader_file_path = file_path.to_path_buf();
        let reader_cancelled = std::sync::Arc::clone(&is_cancelled);
        let chunk_size_bytes = manifest.chunk_size as usize;
        let mut pending_reader_chunks = chunks_to_send;

        let reader_handle = tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
            let mut file = std::fs::File::open(&reader_file_path)?;
            let mut buffer_pool: Vec<Vec<u8>> = Vec::with_capacity(16);

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
                file.seek(std::io::SeekFrom::Start(entry.file_offset))?;
                crate::chunk::read_chunk_into_slice(&mut file, entry.file_offset, &mut buf)?;

                let checksum = compute_xxhash64(&buf);
                if chunk_tx.blocking_send(PreparedChunk { entry, payload: buf, checksum }).is_err() {
                    break;
                }
            }
            Ok(())
        });

        while completed_set.len() < total_chunks_needed {
            match transfer_control_status(transfer_id) {
                Some(TransferStatus::Paused) => {
                    is_cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                    return Err(TransferSessionError::Paused);
                }
                Some(TransferStatus::Cancelled) => {
                    is_cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                    return Err(TransferSessionError::Cancelled);
                }
                _ => {}
            }

            if in_flight.len() < PIPELINE_DEPTH {
                tokio::select! {
                    biased;
                    frame_res = transport.receive_frame() => {
                        if let Ok(Some(frame)) = frame_res {
                            handle_ack_frame(
                                frame,
                                is_usb,
                                &mut in_flight,
                                &mut completed_set,
                                &plan_map,
                                &retry_tx,
                                transfer_id,
                                &mut bytes_sent_total,
                                &mut completed_chunks_count,
                            )?;
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

                            transport.send_frame(&chunk_msg).await?;
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
                    TransferSessionError::UnexpectedMessage("EOF while waiting for ChunkAck".into())
                })?;
                handle_ack_frame(
                    frame,
                    is_usb,
                    &mut in_flight,
                    &mut completed_set,
                    &plan_map,
                    &retry_tx,
                    transfer_id,
                    &mut bytes_sent_total,
                    &mut completed_chunks_count,
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
    let file_checksum = compute_file_crc32c(file_path)?;
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

    Ok(())
}

async fn handle_multipath_ack_frame(
    frame: Message,
    is_usb: bool,
    worker_in_flight: &mut std::collections::HashSet<u32>,
    completed: &std::sync::Arc<tokio::sync::Mutex<std::collections::HashSet<u32>>>,
    plan_map: &std::collections::HashMap<u32, crate::chunk::ChunkPlanEntry>,
    retry_tx: &std::sync::mpsc::Sender<crate::chunk::ChunkPlanEntry>,
    transfer_id: Uuid,
    bytes_sent: &std::sync::Arc<std::sync::atomic::AtomicU64>,
    chunks_done: &std::sync::Arc<std::sync::atomic::AtomicU32>,
) -> Result<(), TransferSessionError> {
    match frame {
        Message::ChunkAck(ack) => {
            worker_in_flight.remove(&ack.chunk_id);
            let mut c = completed.lock().await;
            if c.insert(ack.chunk_id) {
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
                let mut c = completed.lock().await;
                if c.insert(cid) {
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
    mut transports: Vec<Box<dyn Transport>>,
    custom_file_name: Option<&str>,
) -> Result<(), TransferSessionError> {
    if transports.is_empty() {
        return Err(TransferSessionError::Transport(TransportError::Disconnected(
            "No active transports provided for multipath transfer".into(),
        )));
    }
    if transports.len() == 1 {
        let transport = transports.pop().unwrap();
        return send_file_session(
            sender_device_id,
            sender_device_name,
            file_path,
            chunk_size,
            transfer_id,
            transport,
            custom_file_name,
        )
        .await;
    }

    let manifest = generate_manifest_with_name(file_path, chunk_size, custom_file_name)?;
    let plan = calculate_chunk_plan(manifest.file_size, manifest.chunk_size);
    let plan_map: std::collections::HashMap<u32, crate::chunk::ChunkPlanEntry> =
        plan.iter().map(|e| (e.chunk_id, e.clone())).collect();

    // 1. Perform Hello and TransferOffer handshakes across all transports
    let mut resume_ranges_combined: Vec<(u32, u32)> = Vec::new();
    for transport in &mut transports {
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
            Message::TransferAccept(accept) => {
                if let Some(ranges) = accept.resume_from {
                    resume_ranges_combined.extend(ranges);
                }
            }
            Message::TransferReject(reject) => {
                return Err(TransferSessionError::Rejected(reject.reason))
            }
            other => {
                return Err(TransferSessionError::UnexpectedMessage(format!(
                    "Expected Accept or Reject, got {:?}",
                    other
                )));
            }
        }
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
        let file_checksum = compute_file_crc32c(file_path)?;
        let complete_msg = Message::Complete(CompleteData {
            transfer_id,
            file_checksum,
        });
        transports[0].send_frame(&complete_msg).await?;
        let final_frame = transports[0]
            .receive_frame()
            .await?
            .ok_or_else(|| TransferSessionError::UnexpectedMessage("EOF waiting for completion ACK".into()))?;
        if !matches!(final_frame, Message::ChunkAck(_)) {
            return Err(TransferSessionError::UnexpectedMessage(format!(
                "Expected final Ack, got {:?}",
                final_frame
            )));
        }
        return Ok(());
    }

    let (prepared_tx, prepared_rx) = tokio::sync::mpsc::channel::<PreparedChunk>(16);
    let (retry_tx, retry_rx) = std::sync::mpsc::channel::<crate::chunk::ChunkPlanEntry>();
    let (recycle_tx, recycle_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    let is_cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let reader_file_path = file_path.to_path_buf();
    let reader_cancelled = std::sync::Arc::clone(&is_cancelled);
    let chunk_size_bytes = manifest.chunk_size as usize;
    let mut pending_reader_chunks = initial_chunks_to_send;

    let reader_handle = tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
        let mut file = std::fs::File::open(&reader_file_path)?;
        let mut buffer_pool: Vec<Vec<u8>> = Vec::with_capacity(32);

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
            file.seek(std::io::SeekFrom::Start(entry.file_offset))?;
            crate::chunk::read_chunk_into_slice(&mut file, entry.file_offset, &mut buf)?;

            let checksum = compute_xxhash64(&buf);
            if prepared_tx.blocking_send(PreparedChunk { entry, payload: buf, checksum }).is_err() {
                break;
            }
        }
        Ok(())
    });

    let shared_prepared_rx = std::sync::Arc::new(tokio::sync::Mutex::new(prepared_rx));
    let shared_retry_tx = std::sync::Arc::new(retry_tx);
    let shared_recycle_tx = std::sync::Arc::new(recycle_tx);
    let shared_completed = std::sync::Arc::new(tokio::sync::Mutex::new(completed_set_init));
    let shared_bytes_sent = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(bytes_sent_total_init));
    let shared_chunks_done = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(completed_chunks_count_init));
    let shared_plan_map = std::sync::Arc::new(plan_map);

    let mut worker_handles = Vec::new();

    for (idx, mut transport) in transports.into_iter().enumerate() {
        let prepared_rx = std::sync::Arc::clone(&shared_prepared_rx);
        let retry_tx = std::sync::Arc::clone(&shared_retry_tx);
        let recycle_tx = std::sync::Arc::clone(&shared_recycle_tx);
        let completed = std::sync::Arc::clone(&shared_completed);
        let bytes_sent = std::sync::Arc::clone(&shared_bytes_sent);
        let chunks_done = std::sync::Arc::clone(&shared_chunks_done);
        let plan_map = std::sync::Arc::clone(&shared_plan_map);
        let cancelled = std::sync::Arc::clone(&is_cancelled);
        let file_id = manifest.file_id;
        let total_chunks = plan.len();
        let is_usb = transport.kind() == crate::transport::TransportKind::Usb || idx == 0;

        let handle = tokio::spawn(async move {
            const WORKER_PIPELINE_DEPTH: usize = 32;
            let mut worker_in_flight = std::collections::HashSet::new();

            loop {
                if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                    break;
                }

                // Check if all chunks completed
                let done_count = {
                    let c = completed.lock().await;
                    c.len()
                };
                if done_count >= total_chunks {
                    break;
                }

                // Check transfer control status
                match transfer_control_status(transfer_id) {
                    Some(TransferStatus::Paused) => {
                        cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                        return Err(TransferSessionError::Paused);
                    }
                    Some(TransferStatus::Cancelled) => {
                        cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                        return Err(TransferSessionError::Cancelled);
                    }
                    _ => {}
                }

                // Try to dispatch next chunk if worker has capacity
                let can_dispatch = worker_in_flight.len() < WORKER_PIPELINE_DEPTH;
                let next_chunk = if can_dispatch {
                    let mut rx = prepared_rx.lock().await;
                    rx.try_recv().ok()
                } else {
                    None
                };

                if let Some(prepared) = next_chunk {
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

                    if let Err(e) = transport.send_frame(&chunk_msg).await {
                        // Transport failed -> return unacked chunks from this worker to retry channel
                        let _ = retry_tx.send(prepared.entry);
                        for cid in worker_in_flight.drain() {
                            if let Some(e) = plan_map.get(&cid) {
                                let _ = retry_tx.send(e.clone());
                            }
                        }
                        log::warn!("Multipath transport #{} send failed: {} -> requeued chunks", idx, e);
                        return Ok((idx, transport, false)); // Transport dropped
                    }

                    if let Message::ChunkData(d) = chunk_msg {
                        let _ = recycle_tx.send(d.payload);
                    }

                    worker_in_flight.insert(chunk_id);

                    // Opportunistically check if ACKs are waiting without blocking
                    tokio::select! {
                        biased;
                        frame_res = transport.receive_frame() => {
                            match frame_res {
                                Ok(Some(frame)) => {
                                    handle_multipath_ack_frame(
                                        frame,
                                        is_usb,
                                        &mut worker_in_flight,
                                        &completed,
                                        &plan_map,
                                        &retry_tx,
                                        transfer_id,
                                        &bytes_sent,
                                        &chunks_done,
                                    ).await?;
                                }
                                Ok(None) | Err(_) => {
                                    // Transport disconnected
                                    for cid in worker_in_flight.drain() {
                                        if let Some(e) = plan_map.get(&cid) {
                                            let _ = retry_tx.send(e.clone());
                                        }
                                    }
                                    log::warn!("Multipath transport #{} EOF/error -> requeued chunks", idx);
                                    return Ok((idx, transport, false));
                                }
                            }
                        }
                        _ = async {} => {}
                    }
                } else if !worker_in_flight.is_empty() {
                    // No new chunks or pipeline full -> await ACK from this transport
                    tokio::select! {
                        frame_res = transport.receive_frame() => {
                            match frame_res {
                                Ok(Some(frame)) => {
                                    handle_multipath_ack_frame(
                                        frame,
                                        is_usb,
                                        &mut worker_in_flight,
                                        &completed,
                                        &plan_map,
                                        &retry_tx,
                                        transfer_id,
                                        &bytes_sent,
                                        &chunks_done,
                                    ).await?;
                                }
                                Ok(None) | Err(_) => {
                                    for cid in worker_in_flight.drain() {
                                        if let Some(e) = plan_map.get(&cid) {
                                            let _ = retry_tx.send(e.clone());
                                        }
                                    }
                                    return Ok((idx, transport, false));
                                }
                            }
                        }
                        _ = tokio::time::sleep(tokio::time::Duration::from_millis(50)) => {}
                    }
                } else {
                    // Pending is empty and no chunks in flight for this worker -> yield until done
                    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
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
    drop(shared_prepared_rx);
    let _ = reader_handle.await;

    let final_done = {
        let c = shared_completed.lock().await;
        c.len()
    };

    if final_done < plan.len() {
        return Err(TransferSessionError::Transport(TransportError::Disconnected(
            "All multipath transports disconnected before completing transfer".into(),
        )));
    }

    // 3. Complete transfer on the first surviving transport
    if let Some(mut primary_transport) = returned_transports.into_iter().next() {
        let file_checksum = compute_file_crc32c(file_path)?;
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
        resume_from,
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

    // 5. Create and pre-allocate .part file, keeping handle open for the entire session
    std::fs::create_dir_all(dest_dir)?;
    let part_path = dest_dir.join(format!("{}.part", offer.file_name));
    let final_path = dest_dir.join(&offer.file_name);

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(&part_path)?;
    file.set_len(offer.file_size)?;

    // Spawn high-throughput background disk writer to decouple disk I/O from TCP socket reads
    struct DiskWriteTask {
        file_offset: u64,
        payload: Vec<u8>,
    }

    let (disk_tx, mut disk_rx) = tokio::sync::mpsc::channel::<DiskWriteTask>(32);
    let mut writer_file = file;
    let disk_writer_handle = tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
        use std::io::{Seek, SeekFrom, Write};
        while let Some(task) = disk_rx.blocking_recv() {
            writer_file.seek(SeekFrom::Start(task.file_offset))?;
            writer_file.write_all(&task.payload)?;
        }
        writer_file.flush()?;
        drop(writer_file);
        Ok(())
    });

    let mut bytes_recv_total = 0u64;
    let mut completed_chunks_count = 0u32;

    // 6. Data Plane Receive Loop
    loop {
        let frame = match transport.receive_frame().await? {
            Some(f) => f,
            None => break, // Stream closed
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

                // Idempotent write check (§5.1)
                if tracker.is_chunk_completed(
                    chunk_data.transfer_id,
                    chunk_data.file_id,
                    chunk_data.chunk_id,
                    chunk_data.checksum,
                ) {
                    let ack = Message::ChunkAck(ChunkAckData {
                        transfer_id: chunk_data.transfer_id,
                        chunk_id: chunk_data.chunk_id,
                    });
                    transport.send_frame(&ack).await?;
                    continue;
                }

                tracker.mark_chunk_completed(
                    chunk_data.transfer_id,
                    chunk_data.file_id,
                    chunk_data.chunk_id,
                    chunk_data.checksum,
                );

                bytes_recv_total += chunk_data.payload_length as u64;
                completed_chunks_count += 1;
                update_transfer_progress(
                    chunk_data.transfer_id,
                    bytes_recv_total,
                    completed_chunks_count,
                );

                // Send immediate ChunkAck for 100% universal sender compatibility
                let ack = Message::ChunkAck(ChunkAckData {
                    transfer_id: chunk_data.transfer_id,
                    chunk_id: chunk_data.chunk_id,
                });
                transport.send_frame(&ack).await?;

                // Dispatch disk write to background worker (async backpressure if queue fills)
                let _ = disk_tx.send(DiskWriteTask {
                    file_offset: chunk_data.file_offset,
                    payload: chunk_data.payload,
                }).await;
            }
            Message::Complete(complete_data) => {
                // Drop writer channel and await completion of all background disk writes
                drop(disk_tx);
                if let Ok(res) = disk_writer_handle.await {
                    res?;
                }

                // Verify file-level Castagnoli CRC32C
                let file_crc = compute_file_crc32c(&part_path)?;
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

                // Rename .part file to final file name
                rename(&part_path, &final_path)?;

                set_transfer_status(complete_data.transfer_id, TransferStatus::Completed, None);

                // Send final completion Ack
                let ack = Message::ChunkAck(ChunkAckData {
                    transfer_id: complete_data.transfer_id,
                    chunk_id: u32::MAX,
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

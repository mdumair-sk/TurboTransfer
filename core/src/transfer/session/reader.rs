use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::checksum::{compute_file_crc32c, compute_xxhash64, Crc32cAccumulator};
use crate::chunk::{read_chunk_into_slice, ChunkPlanEntry};
use crate::util::telemetry::TransferTelemetry;

#[derive(Debug)]
pub(crate) struct PreparedChunk {
    pub(crate) entry: ChunkPlanEntry,
    pub(crate) payload: Vec<u8>,
    pub(crate) checksum: u64,
}

pub(crate) fn spawn_chunk_reader(
    file_path: PathBuf,
    chunk_size_bytes: usize,
    mut pending_chunks: VecDeque<ChunkPlanEntry>,
    resume_ranges: Vec<(u32, u32)>,
    plan_map: HashMap<u32, ChunkPlanEntry>,
    total_plan_chunks: usize,
    prepared_tx: async_channel::Sender<PreparedChunk>,
    retry_rx: Receiver<ChunkPlanEntry>,
    recycle_rx: Receiver<Vec<u8>>,
    is_cancelled: Arc<AtomicBool>,
    running_crc_tx: oneshot::Sender<u32>,
    telemetry: Arc<TransferTelemetry>,
) -> JoinHandle<Result<(), std::io::Error>> {
    tokio::task::spawn_blocking(move || -> Result<(), std::io::Error> {
        let mut file = crate::util::storage::open_sequential_read(&file_path)?;
        let mut buffer_pool: Vec<Vec<u8>> = Vec::with_capacity(64);
        let mut chunk_crc_map: HashMap<u32, (u32, usize)> = HashMap::new();
        let mut crc_tx_opt = Some(running_crc_tx);

        // Pre-calculate CRC for skipped chunks so sender has all total_plan_chunks in chunk_crc_map without re-reading whole file
        for &(start, end) in &resume_ranges {
            for cid in start..=end {
                if let Some(entry) = plan_map.get(&cid) {
                    if !chunk_crc_map.contains_key(&cid) {
                        let mut buf = vec![0u8; entry.payload_length as usize];
                        if read_chunk_into_slice(&mut file, entry.file_offset, &mut buf).is_ok() {
                            let chunk_crc = crate::checksum::compute_crc32c(&buf);
                            chunk_crc_map.insert(cid, (chunk_crc, buf.len()));
                        }
                    }
                }
            }
        }

        loop {
            if is_cancelled.load(Ordering::Relaxed) {
                break;
            }

            while let Ok(buf) = recycle_rx.try_recv() {
                buffer_pool.push(buf);
            }

            let next_entry = if let Ok(entry) = retry_rx.try_recv() {
                Some(entry)
            } else if let Some(entry) = pending_chunks.pop_front() {
                Some(entry)
            } else {
                if chunk_crc_map.len() == total_plan_chunks {
                    if let Some(tx) = crc_tx_opt.take() {
                        let mut acc = Crc32cAccumulator::new();
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

            let mut buf = buffer_pool
                .pop()
                .unwrap_or_else(|| Vec::with_capacity(chunk_size_bytes));
            buf.resize(entry.payload_length as usize, 0);

            use std::io::Seek;
            let t_r0 = std::time::Instant::now();
            file.seek(std::io::SeekFrom::Start(entry.file_offset))?;
            read_chunk_into_slice(&mut file, entry.file_offset, &mut buf)?;
            let read_us = t_r0.elapsed().as_micros() as u64;

            let t_h0 = std::time::Instant::now();
            let chunk_crc = crate::checksum::compute_crc32c(&buf);
            chunk_crc_map.insert(entry.chunk_id, (chunk_crc, buf.len()));
            let checksum = compute_xxhash64(&buf);
            let hash_us = t_h0.elapsed().as_micros() as u64;

            telemetry.record_chunk_read(
                entry.chunk_id,
                entry.payload_length as u64,
                read_us,
                hash_us,
            );

            if prepared_tx
                .send_blocking(PreparedChunk {
                    entry,
                    payload: buf,
                    checksum,
                })
                .is_err()
            {
                break;
            }
        }

        // In-flight O(1) finalization: combine CRC32Cs of all chunks in order if read completely
        if let Some(tx) = crc_tx_opt.take() {
            if chunk_crc_map.len() == total_plan_chunks {
                let mut acc = Crc32cAccumulator::new();
                for cid in 0..total_plan_chunks as u32 {
                    if let Some(&(crc, len)) = chunk_crc_map.get(&cid) {
                        acc.combine(crc, len);
                    }
                }
                let _ = tx.send(acc.finalize());
            } else if let Ok(crc) = compute_file_crc32c(&file_path) {
                let _ = tx.send(crc);
            }
        }

        Ok(())
    })
}

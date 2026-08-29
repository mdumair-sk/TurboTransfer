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

use chrono::Utc;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{interval_at, Instant};

use super::schema::{coalesce_ranges, expand_ranges, TransferMeta, TransferStatus};



#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    Usb,
    WifiDirect,
}

#[derive(Debug)]
pub enum ActorMessage {
    ChunkCompleted {
        chunk_id: u32,
        transport: TransportType,
        bytes: u64,
    },
    ChunkFailed {
        chunk_id: u32,
        transport: TransportType,
    },
    TransportStatusChanged {
        transport: TransportType,
        connected: bool,
    },
    Pause,
    Cancel,
    GetMeta(oneshot::Sender<TransferMeta>),
}

#[derive(Clone, Debug)]
pub struct MetaActorHandle {
    tx: mpsc::Sender<ActorMessage>,
}

impl MetaActorHandle {
    pub fn new(tx: mpsc::Sender<ActorMessage>) -> Self {
        Self { tx }
    }

    pub async fn send_chunk_completed(&self, chunk_id: u32, transport: TransportType, bytes: u64) {
        let _ = self
            .tx
            .send(ActorMessage::ChunkCompleted {
                chunk_id,
                transport,
                bytes,
            })
            .await;
    }

    pub fn try_send_chunk_completed(&self, chunk_id: u32, transport: TransportType, bytes: u64) {
        let _ = self.tx.try_send(ActorMessage::ChunkCompleted {
            chunk_id,
            transport,
            bytes,
        });
    }

    pub async fn send_chunk_failed(&self, chunk_id: u32, transport: TransportType) {
        let _ = self
            .tx
            .send(ActorMessage::ChunkFailed {
                chunk_id,
                transport,
            })
            .await;
    }

    pub async fn pause(&self) {
        let _ = self.tx.send(ActorMessage::Pause).await;
    }

    pub async fn cancel(&self) {
        let _ = self.tx.send(ActorMessage::Cancel).await;
    }

    pub async fn get_meta(&self) -> Result<TransferMeta, String> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(ActorMessage::GetMeta(reply_tx))
            .await
            .map_err(|e| e.to_string())?;
        reply_rx.await.map_err(|e| e.to_string())
    }
}

pub struct MetaActor {
    meta_path: PathBuf,
    meta: TransferMeta,
    completed_set: HashSet<u32>,
    dirty_events: usize,
    rx: mpsc::Receiver<ActorMessage>,
}

impl MetaActor {
    /// Spawns the MetaActor task for a given transfer.
    /// If `meta_path` exists on disk, reads and expands its `completed_ranges` into memory.
    pub fn spawn(
        meta_path: PathBuf,
        initial_meta: TransferMeta,
        buffer_size: usize,
    ) -> (MetaActorHandle, tokio::task::JoinHandle<()>) {
        let (tx, rx) = mpsc::channel(buffer_size);

        let (meta, completed_set) = if meta_path.exists() {
            match fs::read_to_string(&meta_path) {
                Ok(content) => match serde_json::from_str::<TransferMeta>(&content) {
                    Ok(loaded_meta) => {
                        let set = expand_ranges(&loaded_meta.completed_ranges);
                        (loaded_meta, set)
                    }
                    Err(_) => (initial_meta, HashSet::new()),
                },
                Err(_) => (initial_meta, HashSet::new()),
            }
        } else {
            (initial_meta, HashSet::new())
        };

        let mut actor = Self {
            meta_path,
            meta,
            completed_set,
            dirty_events: 0,
            rx,
        };

        // Flush initial meta to disk
        actor.flush_sync();

        let handle = crate::util::runtime::spawn_task(async move {
            actor.run().await;
        });

        (MetaActorHandle::new(tx), handle)
    }

    pub fn completed_set(&self) -> &HashSet<u32> {
        &self.completed_set
    }

    fn flush_sync(&mut self) {
        self.meta.completed_ranges = coalesce_ranges(&self.completed_set);
        self.meta.updated_at = Utc::now().to_rfc3339();

        if let Some(parent) = self.meta_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        if let Ok(json) = serde_json::to_string_pretty(&self.meta) {
            let _ = fs::write(&self.meta_path, json);
        }
        self.dirty_events = 0;
    }

    async fn flush_async(&mut self) {
        self.meta.completed_ranges = coalesce_ranges(&self.completed_set);
        self.meta.updated_at = Utc::now().to_rfc3339();

        let path = self.meta_path.clone();
        if let Ok(json) = serde_json::to_string_pretty(&self.meta) {
            let _ = tokio::task::spawn_blocking(move || {
                if let Some(parent) = path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(&path, json);
            }).await;
        }
        self.dirty_events = 0;
    }

    async fn run(&mut self) {
        let mut timer = interval_at(Instant::now() + Duration::from_millis(1000), Duration::from_millis(1000));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                maybe_msg = self.rx.recv() => {
                    match maybe_msg {
                        Some(msg) => {
                            let should_exit = self.handle_message(msg);
                            if should_exit {
                                self.flush_async().await;
                                break;
                            }
                        }
                        None => {
                            // Channel closed (all handles dropped)
                            if self.dirty_events > 0 {
                                self.flush_async().await;
                            }
                            break;
                        }
                    }
                }
                _ = timer.tick() => {
                    if self.dirty_events > 0 {
                        self.flush_async().await;
                    }
                }
            }
        }
    }

    /// Handles a single incoming actor message.
    /// Returns `true` if the actor loop should terminate.
    fn handle_message(&mut self, msg: ActorMessage) -> bool {
        match msg {
            ActorMessage::ChunkCompleted {
                chunk_id,
                transport,
                bytes,
            } => {
                self.completed_set.insert(chunk_id);
                match transport {
                    TransportType::Usb => self.meta.transport_stats.usb.bytes += bytes,
                    TransportType::WifiDirect => self.meta.transport_stats.wifi_direct.bytes += bytes,
                }
                self.dirty_events += 1;
                false
            }
            ActorMessage::ChunkFailed { transport, .. } => {
                match transport {
                    TransportType::Usb => {
                        self.meta.transport_stats.usb.errors += 1;
                        self.meta.transport_stats.usb.retries += 1;
                    }
                    TransportType::WifiDirect => {
                        self.meta.transport_stats.wifi_direct.errors += 1;
                        self.meta.transport_stats.wifi_direct.retries += 1;
                    }
                }
                self.dirty_events += 1;

                if self.dirty_events >= 10 {
                    self.flush_sync();
                }
                false
            }
            ActorMessage::TransportStatusChanged { .. } => {
                self.dirty_events += 1;
                if self.dirty_events >= 10 {
                    self.flush_sync();
                }
                false
            }
            ActorMessage::Pause => {
                self.meta.status = TransferStatus::Paused;
                true // Exit loop & flush
            }
            ActorMessage::Cancel => {
                self.meta.status = TransferStatus::Cancelled;
                true // Exit loop & flush
            }
            ActorMessage::GetMeta(reply) => {
                self.meta.completed_ranges = coalesce_ranges(&self.completed_set);
                let _ = reply.send(self.meta.clone());
                false
            }
        }
    }
}

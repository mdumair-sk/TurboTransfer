use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use crate::chunk::ChunkPlanEntry;
use crate::manifest::actor::MetaActorHandle;
use crate::manifest::TransportType;
use crate::protocol::Message;
use crate::scheduler::model::ChannelPerformanceModel;
use crate::scheduler::tracker::ChannelTracker;
use crate::scheduler::window::WindowController;
use crate::transfer::api::{record_channel_bytes, update_transfer_progress};
use crate::transfer::session::TransferSessionError;
use crate::util::telemetry::TransferTelemetry;

pub(crate) fn handle_multipath_ack_frame(
    frame: Message,
    is_usb: bool,
    tracker: &mut ChannelTracker,
    model: &mut ChannelPerformanceModel,
    window: &mut WindowController,
    worker_in_flight_times: &mut HashMap<u32, Instant>,
    completed: &Arc<Mutex<HashSet<u32>>>,
    completed_count: &Arc<AtomicUsize>,
    plan_map: &HashMap<u32, ChunkPlanEntry>,
    retry_tx: &Sender<ChunkPlanEntry>,
    transfer_id: Uuid,
    bytes_sent: &Arc<AtomicU64>,
    chunks_done: &Arc<AtomicU32>,
    telemetry: Option<&Arc<TransferTelemetry>>,
    channel_name: &str,
    last_socket_duration_us: u64,
    actor_handle: Option<&MetaActorHandle>,
) -> Result<(), TransferSessionError> {
    match frame {
        Message::ChunkAck(ack) => {
            let bytes_len = plan_map.get(&ack.chunk_id).map_or(0, |e| e.payload_length as u64);
            let t_disp = worker_in_flight_times.remove(&ack.chunk_id);
            let rtt_us = match t_disp {
                Some(t) => {
                    let r = t.elapsed().as_micros() as u64;
                    let rtt_ms = r as f64 / 1000.0;
                    if let Some(tel) = telemetry {
                        tel.record_chunk_ack(channel_name, ack.chunk_id, rtt_ms, bytes_len);
                    }
                    r
                }
                None => last_socket_duration_us.max(5_000),
            };
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

            let is_new = completed.lock().insert(ack.chunk_id);
            if is_new {
                if let Some(actor) = actor_handle {
                    let t_type = if is_usb {
                        TransportType::Usb
                    } else {
                        TransportType::WifiDirect
                    };
                    actor.try_send_chunk_completed(ack.chunk_id, t_type, bytes_len);
                }
                completed_count.fetch_add(1, Ordering::Relaxed);
                if let Some(entry) = plan_map.get(&ack.chunk_id) {
                    let total_b = bytes_sent.fetch_add(entry.payload_length as u64, Ordering::Relaxed)
                        + entry.payload_length as u64;
                    let total_c = chunks_done.fetch_add(1, Ordering::Relaxed) + 1;
                    update_transfer_progress(transfer_id, total_b, total_c);
                    record_channel_bytes(transfer_id, is_usb, entry.payload_length as u64);
                }
            }
        }
        Message::BatchChunkAck(batch) => {
            for cid in batch.chunk_ids {
                let bytes_len = plan_map.get(&cid).map_or(0, |e| e.payload_length as u64);
                let t_disp = worker_in_flight_times.remove(&cid);
                let rtt_us = match t_disp {
                    Some(t) => {
                        let r = t.elapsed().as_micros() as u64;
                        let rtt_ms = r as f64 / 1000.0;
                        if let Some(tel) = telemetry {
                            tel.record_chunk_ack(channel_name, cid, rtt_ms, bytes_len);
                        }
                        r
                    }
                    None => last_socket_duration_us.max(5_000),
                };
                if let Some(sample) = tracker.record_chunk_ack(
                    cid,
                    bytes_len,
                    rtt_us,
                    last_socket_duration_us,
                    None,
                ) {
                    model.update_from_tracker_and_sample(tracker, &sample);
                }

                let is_new = completed.lock().insert(cid);
                if is_new {
                    if let Some(actor) = actor_handle {
                        let t_type = if is_usb {
                            TransportType::Usb
                        } else {
                            TransportType::WifiDirect
                        };
                        actor.try_send_chunk_completed(cid, t_type, bytes_len);
                    }
                    completed_count.fetch_add(1, Ordering::Relaxed);
                    if let Some(entry) = plan_map.get(&cid) {
                        let total_b = bytes_sent.fetch_add(entry.payload_length as u64, Ordering::Relaxed)
                            + entry.payload_length as u64;
                        let total_c = chunks_done.fetch_add(1, Ordering::Relaxed) + 1;
                        update_transfer_progress(transfer_id, total_b, total_c);
                        record_channel_bytes(transfer_id, is_usb, entry.payload_length as u64);
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

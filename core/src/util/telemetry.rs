use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use parking_lot::Mutex;
use uuid::Uuid;

use crate::manifest::TransferRole;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferStage {
    Init,
    Discovery,
    Connection,
    Handshake,
    DiskRead,
    Checksum,
    NetQueue,
    NetSend,
    NetRecv,
    NetAck,
    DiskQueue,
    DiskWrite,
    Finalize,
    Control,
}

impl std::fmt::Display for TransferStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransferStage::Init => write!(f, "INIT"),
            TransferStage::Discovery => write!(f, "DISCOVERY"),
            TransferStage::Connection => write!(f, "CONNECTION"),
            TransferStage::Handshake => write!(f, "HANDSHAKE"),
            TransferStage::DiskRead => write!(f, "DISK_READ"),
            TransferStage::Checksum => write!(f, "CHECKSUM"),
            TransferStage::NetQueue => write!(f, "NET_QUEUE"),
            TransferStage::NetSend => write!(f, "NET_SEND"),
            TransferStage::NetRecv => write!(f, "NET_RECV"),
            TransferStage::NetAck => write!(f, "NET_ACK"),
            TransferStage::DiskQueue => write!(f, "DISK_QUEUE"),
            TransferStage::DiskWrite => write!(f, "DISK_WRITE"),
            TransferStage::Finalize => write!(f, "FINALIZE"),
            TransferStage::Control => write!(f, "CONTROL"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for EventLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventLevel::Debug => write!(f, "DEBUG"),
            EventLevel::Info => write!(f, "INFO"),
            EventLevel::Warn => write!(f, "WARN"),
            EventLevel::Error => write!(f, "ERROR"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferEvent {
    pub timestamp_us: u64,
    pub relative_ms: u64,
    pub stage: TransferStage,
    pub level: EventLevel,
    pub channel: String,
    pub chunk_id: Option<u32>,
    pub duration_us: Option<u64>,
    pub bytes: Option<u64>,
    pub message: String,
    pub details: HashMap<String, String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ChannelMetric {
    pub channel_name: String,
    pub bytes_transferred: u64,
    pub chunks_transferred: u32,
    pub throughput_mbps: f64,
    pub max_in_flight: u32,
    pub avg_socket_write_us: f64,
    pub avg_rtt_ms: f64,
    pub p95_rtt_ms: f64,
    pub nack_count: u64,
    pub disconnect_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BottleneckReport {
    pub transfer_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub role: String,
    pub total_duration_ms: u64,
    pub avg_throughput_mbps: f64,
    pub peak_throughput_mbps: f64,
    pub sender_disk_read_mbps: f64,
    pub sender_disk_read_avg_us: f64,
    pub sender_disk_read_p95_us: f64,
    pub sender_checksum_mbps: f64,
    pub sender_checksum_avg_us: f64,
    pub receiver_disk_write_mbps: f64,
    pub receiver_disk_write_avg_us: f64,
    pub receiver_disk_write_p95_us: f64,
    pub receiver_max_queue_depth: u32,
    pub receiver_finalize_ms: u64,
    pub channels: Vec<ChannelMetric>,
    pub stage_durations_pct: HashMap<String, f64>,
    pub primary_bottleneck: String,
    pub recommendations: Vec<String>,
}

struct TelemetryChannelTracker {
    bytes: u64,
    chunks: u32,
    current_in_flight: u32,
    max_in_flight: u32,
    socket_write_durations_us: Vec<u64>,
    rtt_samples_ms: Vec<f64>,
    nacks: u64,
    disconnects: u64,
}

impl TelemetryChannelTracker {
    fn new() -> Self {
        Self {
            bytes: 0,
            chunks: 0,
            current_in_flight: 0,
            max_in_flight: 0,
            socket_write_durations_us: Vec::with_capacity(1024),
            rtt_samples_ms: Vec::with_capacity(1024),
            nacks: 0,
            disconnects: 0,
        }
    }
}

pub struct TransferTelemetry {
    pub transfer_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub role: TransferRole,
    pub start_time: Instant,
    pub start_utc: DateTime<Utc>,
    pub end_time: Mutex<Option<Instant>>,
    pub events: Mutex<Vec<TransferEvent>>,

    // Sender read & hash stats
    read_durations_us: Mutex<Vec<u64>>,
    read_bytes_total: AtomicU64,
    hash_durations_us: Mutex<Vec<u64>>,
    hash_bytes_total: AtomicU64,

    // Receiver stats
    write_durations_us: Mutex<Vec<u64>>,
    write_bytes_total: AtomicU64,
    recv_verify_durations_us: Mutex<Vec<u64>>,
    max_queue_depth: AtomicU32,
    finalize_duration_ms: AtomicU64,
    duplicate_chunks: AtomicU32,

    // Per-channel stats
    channels: Mutex<HashMap<String, TelemetryChannelTracker>>,
    peak_throughput_mbps: Mutex<f64>,
    throughput_sampler: Mutex<(Instant, u64)>,
}

impl TransferTelemetry {
    pub fn new(transfer_id: Uuid, file_name: String, file_size: u64, role: TransferRole) -> Self {
        let now = Instant::now();
        Self {
            transfer_id,
            file_name,
            file_size,
            role,
            start_time: now,
            start_utc: Utc::now(),
            end_time: Mutex::new(None),
            events: Mutex::new(Vec::with_capacity(2048)),
            read_durations_us: Mutex::new(Vec::with_capacity(1024)),
            read_bytes_total: AtomicU64::new(0),
            hash_durations_us: Mutex::new(Vec::with_capacity(1024)),
            hash_bytes_total: AtomicU64::new(0),
            write_durations_us: Mutex::new(Vec::with_capacity(1024)),
            write_bytes_total: AtomicU64::new(0),
            recv_verify_durations_us: Mutex::new(Vec::with_capacity(1024)),
            max_queue_depth: AtomicU32::new(0),
            finalize_duration_ms: AtomicU64::new(0),
            duplicate_chunks: AtomicU32::new(0),
            channels: Mutex::new(HashMap::new()),
            peak_throughput_mbps: Mutex::new(0.0),
            throughput_sampler: Mutex::new((now, 0)),
        }
    }

    pub fn sample_throughput(&self, added_bytes: u64) {
        let mut sampler = self.throughput_sampler.lock();
        sampler.1 += added_bytes;
        let now = Instant::now();
        let elapsed = now.duration_since(sampler.0).as_secs_f64();
        if elapsed >= 0.25 {
            let mbps = (sampler.1 as f64 / (1024.0 * 1024.0)) / elapsed;
            let mut peak = self.peak_throughput_mbps.lock();
            if mbps > *peak {
                *peak = mbps;
            }
            sampler.0 = now;
            sampler.1 = 0;
        }
    }

    pub fn record_event(
        &self,
        stage: TransferStage,
        level: EventLevel,
        channel: &str,
        chunk_id: Option<u32>,
        duration_us: Option<u64>,
        bytes: Option<u64>,
        message: impl Into<String>,
        details: Option<HashMap<String, String>>,
    ) {
        let now = Instant::now();
        let relative_ms = now.duration_since(self.start_time).as_millis() as u64;
        let timestamp_us = self.start_utc.timestamp_micros() as u64 + (relative_ms * 1000);
        let msg = message.into();

        // Also log to standard Rust log for Logcat / console visibility
        match level {
            EventLevel::Debug => log::debug!("[{}] [{}] [{}] {}", self.transfer_id, stage, channel, msg),
            EventLevel::Info => log::info!("[{}] [{}] [{}] {}", self.transfer_id, stage, channel, msg),
            EventLevel::Warn => log::warn!("[{}] [{}] [{}] {}", self.transfer_id, stage, channel, msg),
            EventLevel::Error => log::error!("[{}] [{}] [{}] {}", self.transfer_id, stage, channel, msg),
        }

        let event = TransferEvent {
            timestamp_us,
            relative_ms,
            stage,
            level,
            channel: channel.to_string(),
            chunk_id,
            duration_us,
            bytes,
            message: msg,
            details: details.unwrap_or_default(),
        };

        let mut events = self.events.lock();
        if events.len() < 50_000 {
            events.push(event);
        }
    }

    pub fn record_chunk_read(&self, chunk_id: u32, bytes: u64, read_us: u64, hash_us: u64) {
        self.read_bytes_total.fetch_add(bytes, Ordering::Relaxed);
        self.hash_bytes_total.fetch_add(bytes, Ordering::Relaxed);

        {
            let mut reads = self.read_durations_us.lock();
            if reads.len() < 100_000 {
                reads.push(read_us);
            }
        }
        {
            let mut hashes = self.hash_durations_us.lock();
            if hashes.len() < 100_000 {
                hashes.push(hash_us);
            }
        }

        if chunk_id % 32 == 0 || chunk_id == 0 {
            self.record_event(
                TransferStage::DiskRead,
                EventLevel::Debug,
                "DiskReader",
                Some(chunk_id),
                Some(read_us),
                Some(bytes),
                format!("Read chunk #{} ({} bytes) in {} us (hash: {} us)", chunk_id, bytes, read_us, hash_us),
                None,
            );
        }
    }

    pub fn record_chunk_sent(&self, channel_name: &str, chunk_id: u32, bytes: u64, socket_write_us: u64) {
        let mut channels = self.channels.lock();
        let tracker = channels.entry(channel_name.to_string()).or_insert_with(TelemetryChannelTracker::new);
        tracker.bytes += bytes;
        tracker.chunks += 1;
        tracker.current_in_flight += 1;
        if tracker.current_in_flight > tracker.max_in_flight {
            tracker.max_in_flight = tracker.current_in_flight;
        }
        if tracker.socket_write_durations_us.len() < 50_000 {
            tracker.socket_write_durations_us.push(socket_write_us);
        }

        if chunk_id % 32 == 0 || chunk_id == 0 {
            drop(channels);
            self.record_event(
                TransferStage::NetSend,
                EventLevel::Debug,
                channel_name,
                Some(chunk_id),
                Some(socket_write_us),
                Some(bytes),
                format!("Sent chunk #{} on {} in {} us", chunk_id, channel_name, socket_write_us),
                None,
            );
        }
    }

    pub fn record_chunk_ack(&self, channel_name: &str, chunk_id: u32, ack_latency_ms: f64, bytes: u64) {
        {
            let mut channels = self.channels.lock();
            let tracker = channels.entry(channel_name.to_string()).or_insert_with(TelemetryChannelTracker::new);
            tracker.current_in_flight = tracker.current_in_flight.saturating_sub(1);
            if tracker.rtt_samples_ms.len() < 50_000 {
                tracker.rtt_samples_ms.push(ack_latency_ms);
            }
        }
        self.sample_throughput(bytes);

        if chunk_id % 32 == 0 || chunk_id == 0 {
            self.record_event(
                TransferStage::NetAck,
                EventLevel::Debug,
                channel_name,
                Some(chunk_id),
                Some((ack_latency_ms * 1000.0) as u64),
                Some(bytes),
                format!("Received ACK for chunk #{} on {} (Chunk ACK Latency: {:.2} ms)", chunk_id, channel_name, ack_latency_ms),
                None,
            );
        }
    }

    pub fn record_chunk_nack(&self, channel_name: &str, chunk_id: u32, reason: &str) {
        {
            let mut channels = self.channels.lock();
            let tracker = channels.entry(channel_name.to_string()).or_insert_with(TelemetryChannelTracker::new);
            tracker.nacks += 1;
        }
        self.record_event(
            TransferStage::NetAck,
            EventLevel::Warn,
            channel_name,
            Some(chunk_id),
            None,
            None,
            format!("Received NACK for chunk #{}: {}", chunk_id, reason),
            None,
        );
    }

    pub fn record_channel_disconnect(&self, channel_name: &str, reason: &str) {
        {
            let mut channels = self.channels.lock();
            let tracker = channels.entry(channel_name.to_string()).or_insert_with(TelemetryChannelTracker::new);
            tracker.disconnects += 1;
        }
        self.record_event(
            TransferStage::Connection,
            EventLevel::Warn,
            channel_name,
            None,
            None,
            None,
            format!("Transport channel disconnected: {}", reason),
            None,
        );
    }

    pub fn record_chunk_recv(&self, channel_name: &str, chunk_id: u32, bytes: u64, recv_us: u64, verify_us: u64) {
        {
            let mut verify_list = self.recv_verify_durations_us.lock();
            if verify_list.len() < 50_000 {
                verify_list.push(verify_us);
            }
        }
        let mut channels = self.channels.lock();
        let tracker = channels.entry(channel_name.to_string()).or_insert_with(TelemetryChannelTracker::new);
        tracker.bytes += bytes;
        tracker.chunks += 1;

        if chunk_id % 32 == 0 || chunk_id == 0 {
            drop(channels);
            self.record_event(
                TransferStage::NetRecv,
                EventLevel::Debug,
                channel_name,
                Some(chunk_id),
                Some(recv_us),
                Some(bytes),
                format!("Received chunk #{} ({} bytes) in {} us (verify: {} us)", chunk_id, bytes, recv_us, verify_us),
                None,
            );
        }
    }

    pub fn record_duplicate_chunk(&self, chunk_id: u32) {
        self.duplicate_chunks.fetch_add(1, Ordering::Relaxed);
        self.record_event(
            TransferStage::NetRecv,
            EventLevel::Debug,
            "Receiver",
            Some(chunk_id),
            None,
            None,
            format!("Dropped duplicate chunk #{}", chunk_id),
            None,
        );
    }

    pub fn record_disk_write(&self, chunk_id: u32, bytes: u64, write_us: u64, queue_depth: u32) {
        self.write_bytes_total.fetch_add(bytes, Ordering::Relaxed);
        let mut cur_max = self.max_queue_depth.load(Ordering::Relaxed);
        while queue_depth > cur_max {
            match self.max_queue_depth.compare_exchange_weak(cur_max, queue_depth, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => break,
                Err(actual) => cur_max = actual,
            }
        }

        {
            let mut writes = self.write_durations_us.lock();
            if writes.len() < 100_000 {
                writes.push(write_us);
            }
        }

        if chunk_id % 32 == 0 || chunk_id == 0 || queue_depth > 64 {
            let lvl = if queue_depth > 64 { EventLevel::Warn } else { EventLevel::Debug };
            self.record_event(
                TransferStage::DiskWrite,
                lvl,
                "DiskWriter",
                Some(chunk_id),
                Some(write_us),
                Some(bytes),
                format!("Wrote chunk #{} ({} bytes) to disk in {} us [queue depth: {}]", chunk_id, bytes, write_us, queue_depth),
                None,
            );
        }
    }

    pub fn record_finalize(&self, duration_ms: u64, crc_instant: bool) {
        self.finalize_duration_ms.store(duration_ms, Ordering::Relaxed);
        self.record_event(
            TransferStage::Finalize,
            EventLevel::Info,
            "Finalizer",
            None,
            Some(duration_ms * 1000),
            None,
            format!("Transfer finalized in {} ms (in-flight CRC combine: {})", duration_ms, crc_instant),
            None,
        );
    }

    pub fn update_peak_throughput(&self, mbps: f64) {
        let mut peak = self.peak_throughput_mbps.lock();
        if mbps > *peak {
            *peak = mbps;
        }
    }

    pub fn mark_completed(&self) {
        let now = Instant::now();
        let mut end = self.end_time.lock();
        if end.is_none() {
            *end = Some(now);
        }
        let elapsed_ms = now.duration_since(self.start_time).as_millis() as u64;
        let avg_mbps = if elapsed_ms > 0 {
            (self.file_size as f64 / (1024.0 * 1024.0)) / (elapsed_ms as f64 / 1000.0)
        } else {
            0.0
        };

        self.record_event(
            TransferStage::Finalize,
            EventLevel::Info,
            "Session",
            None,
            Some(elapsed_ms * 1000),
            Some(self.file_size),
            format!("Transfer completed: {} bytes in {} ms ({:.2} MB/s avg)", self.file_size, elapsed_ms, avg_mbps),
            None,
        );
    }

    pub fn mark_failed(&self, error: &str) {
        let now = Instant::now();
        let mut end = self.end_time.lock();
        if end.is_none() {
            *end = Some(now);
        }
        let elapsed_ms = now.duration_since(self.start_time).as_millis() as u64;

        self.record_event(
            TransferStage::Finalize,
            EventLevel::Error,
            "Session",
            None,
            Some(elapsed_ms * 1000),
            None,
            format!("Transfer failed after {} ms: {}", elapsed_ms, error),
            None,
        );
    }

    pub fn get_events(&self, max_count: Option<usize>) -> Vec<TransferEvent> {
        let events = self.events.lock();
        match max_count {
            Some(n) if events.len() > n => events[events.len() - n..].to_vec(),
            _ => events.clone(),
        }
    }

    pub fn generate_report(&self) -> BottleneckReport {
        self.generate_bottleneck_report()
    }

    pub fn generate_bottleneck_report(&self) -> BottleneckReport {
        let end_instant = self.end_time.lock().unwrap_or_else(Instant::now);
        let total_duration_ms = end_instant.duration_since(self.start_time).as_millis().max(1) as u64;
        let total_secs = total_duration_ms as f64 / 1000.0;

        let total_bytes = self.file_size;
        let avg_throughput_mbps = (total_bytes as f64 / (1024.0 * 1024.0)) / total_secs;
        let peak_throughput_mbps = (*self.peak_throughput_mbps.lock()).max(avg_throughput_mbps);

        // Sender Disk Read stats
        let read_list = self.read_durations_us.lock().clone();
        let (read_avg_us, read_p95_us) = calc_avg_p95(&read_list);
        let read_bytes = self.read_bytes_total.load(Ordering::Relaxed);
        let read_total_secs = (read_list.iter().sum::<u64>() as f64) / 1_000_000.0;
        let sender_disk_read_mbps = if read_total_secs > 0.0 {
            (read_bytes as f64 / (1024.0 * 1024.0)) / read_total_secs
        } else {
            0.0
        };

        // Sender Checksum stats
        let hash_list = self.hash_durations_us.lock().clone();
        let (hash_avg_us, _) = calc_avg_p95(&hash_list);
        let hash_bytes = self.hash_bytes_total.load(Ordering::Relaxed);
        let hash_total_secs = (hash_list.iter().sum::<u64>() as f64) / 1_000_000.0;
        let sender_checksum_mbps = if hash_total_secs > 0.0 {
            (hash_bytes as f64 / (1024.0 * 1024.0)) / hash_total_secs
        } else {
            0.0
        };

        // Receiver Disk Write stats
        let write_list = self.write_durations_us.lock().clone();
        let (write_avg_us, write_p95_us) = calc_avg_p95(&write_list);
        let write_bytes = self.write_bytes_total.load(Ordering::Relaxed);
        let write_total_secs = (write_list.iter().sum::<u64>() as f64) / 1_000_000.0;
        let receiver_disk_write_mbps = if write_total_secs > 0.0 {
            (write_bytes as f64 / (1024.0 * 1024.0)) / write_total_secs
        } else {
            0.0
        };
        let receiver_max_queue_depth = self.max_queue_depth.load(Ordering::Relaxed);
        let receiver_finalize_ms = self.finalize_duration_ms.load(Ordering::Relaxed);

        // Channels
        let mut channel_metrics = Vec::new();
        let channels_guard = self.channels.lock();
        for (name, tracker) in channels_guard.iter() {
            let (write_avg, _) = calc_avg_p95(&tracker.socket_write_durations_us);
            let (rtt_avg, rtt_p95) = calc_avg_p95_f64(&tracker.rtt_samples_ms);
            let ch_throughput = if total_secs > 0.0 {
                (tracker.bytes as f64 / (1024.0 * 1024.0)) / total_secs
            } else {
                0.0
            };
            channel_metrics.push(ChannelMetric {
                channel_name: name.clone(),
                bytes_transferred: tracker.bytes,
                chunks_transferred: tracker.chunks,
                throughput_mbps: ch_throughput,
                max_in_flight: tracker.max_in_flight,
                avg_socket_write_us: write_avg,
                avg_rtt_ms: rtt_avg,
                p95_rtt_ms: rtt_p95,
                nack_count: tracker.nacks,
                disconnect_count: tracker.disconnects,
            });
        }
        drop(channels_guard);

        // Stage duration breakdown percentages
        let mut stage_durations_pct = HashMap::new();
        let total_active_us = (total_duration_ms * 1000) as f64;
        let read_sum_us: u64 = read_list.iter().sum();
        let write_sum_us: u64 = write_list.iter().sum();
        let hash_sum_us: u64 = hash_list.iter().sum();
        let finalize_us = receiver_finalize_ms * 1000;

        if total_active_us > 0.0 {
            stage_durations_pct.insert("DiskRead".to_string(), (read_sum_us as f64 / total_active_us) * 100.0);
            stage_durations_pct.insert("CpuChecksum".to_string(), (hash_sum_us as f64 / total_active_us) * 100.0);
            stage_durations_pct.insert("DiskWrite".to_string(), (write_sum_us as f64 / total_active_us) * 100.0);
            stage_durations_pct.insert("Finalize".to_string(), (finalize_us as f64 / total_active_us) * 100.0);
        }

        // Bottleneck Diagnosis
        let mut recommendations = Vec::new();
        let primary_bottleneck;

        if self.role == TransferRole::Receiver && (receiver_max_queue_depth >= 96 || write_p95_us > 50_000.0) {
            primary_bottleneck = "RECEIVER_DISK_WRITE_BOTTLENECK".to_string();
            recommendations.push(format!(
                "Receiver storage write latency averaged {:.1} ms (P95: {:.1} ms) and disk queue reached {}/128 chunks. The receiving drive/flash storage is the primary constraint.",
                write_avg_us / 1000.0, write_p95_us / 1000.0, receiver_max_queue_depth
            ));
        } else if self.role == TransferRole::Sender && read_p95_us > 40_000.0 && sender_disk_read_mbps < (avg_throughput_mbps * 1.1) {
            primary_bottleneck = "SENDER_DISK_READ_BOTTLENECK".to_string();
            recommendations.push(format!(
                "Sender disk read throughput ({:.1} MB/s) was slower than network capacity. Reading chunks took an average of {:.1} ms per chunk.",
                sender_disk_read_mbps, read_avg_us / 1000.0
            ));
        } else {
            let total_disconnects: u64 = channel_metrics.iter().map(|c| c.disconnect_count).sum();
            let total_nacks: u64 = channel_metrics.iter().map(|c| c.nack_count).sum();

            if total_disconnects > 0 || total_nacks > 3 {
                primary_bottleneck = "NETWORK_PACKET_CORRUPTION_OR_DROP".to_string();
                recommendations.push(format!(
                    "Network packet corruption or disconnect detected (NACKs: {}, Disconnects: {}). Check physical USB connection or 5GHz Wi-Fi line-of-sight.",
                    total_nacks, total_disconnects
                ));
            } else if hash_avg_us > 25_000.0 && sender_checksum_mbps < 200.0 {
                primary_bottleneck = "CPU_CHECKSUM_BOTTLENECK".to_string();
                recommendations.push(format!(
                    "xxHash64 / CRC32C computation took {:.1} ms per chunk ({:.1} MB/s). CPU computation throttled the transfer pipeline.",
                    hash_avg_us / 1000.0, sender_checksum_mbps
                ));
            } else if avg_throughput_mbps >= 150.0 {
                primary_bottleneck = "BALANCED_WIRE_SPEED".to_string();
                recommendations.push(format!(
                    "Optimal wire-speed performance achieved ({:.1} MB/s average, peak {:.1} MB/s). Pipeline stages operated without stalls.",
                    avg_throughput_mbps, peak_throughput_mbps
                ));
            } else {
                primary_bottleneck = "NETWORK_BANDWIDTH_LIMIT".to_string();
                recommendations.push(format!(
                    "Transfer was network bandwidth-limited at {:.1} MB/s across {} active channel(s) (Peak: {:.1} MB/s). Disk I/O and CPU checksums operated faster than the physical wireless link.",
                    avg_throughput_mbps, channel_metrics.len(), peak_throughput_mbps
                ));
            }
        }

        BottleneckReport {
            transfer_id: self.transfer_id.to_string(),
            file_name: self.file_name.clone(),
            file_size: self.file_size,
            role: format!("{:?}", self.role),
            total_duration_ms,
            avg_throughput_mbps,
            peak_throughput_mbps,
            sender_disk_read_mbps,
            sender_disk_read_avg_us: read_avg_us,
            sender_disk_read_p95_us: read_p95_us,
            sender_checksum_mbps,
            sender_checksum_avg_us: hash_avg_us,
            receiver_disk_write_mbps,
            receiver_disk_write_avg_us: write_avg_us,
            receiver_disk_write_p95_us: write_p95_us,
            receiver_max_queue_depth,
            receiver_finalize_ms,
            channels: channel_metrics,
            stage_durations_pct,
            primary_bottleneck,
            recommendations,
        }
    }

    /// Exports structured `.json` and human-readable `.log` files to `<data_dir>/logs/`.
    pub fn export_log_files(&self, data_dir: &Path) -> Result<(PathBuf, PathBuf), std::io::Error> {
        let logs_dir = data_dir.join("logs");
        std::fs::create_dir_all(&logs_dir)?;

        let id_str = self.transfer_id.to_string();
        let json_path = logs_dir.join(format!("{}.json", id_str));
        let log_path = logs_dir.join(format!("{}.log", id_str));

        let report = self.generate_bottleneck_report();
        let events = self.get_events(None);

        #[derive(Serialize)]
        struct FullExport<'a> {
            report: &'a BottleneckReport,
            events: &'a [TransferEvent],
        }

        // Write JSON file
        let full_export = FullExport {
            report: &report,
            events: &events,
        };
        let json_str = serde_json::to_string_pretty(&full_export)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(&json_path, json_str)?;

        // Write Human-Readable .log file
        let mut log_content = String::new();
        log_content.push_str("================================================================================\n");
        log_content.push_str(&format!(" TurboTransfer Session Log: {}\n", id_str));
        log_content.push_str("================================================================================\n");
        log_content.push_str(&format!("File Name       : {}\n", self.file_name));
        log_content.push_str(&format!("File Size       : {} bytes ({:.2} MB)\n", self.file_size, self.file_size as f64 / (1024.0 * 1024.0)));
        log_content.push_str(&format!("Role            : {:?}\n", self.role));
        log_content.push_str(&format!("Start Time (UTC): {}\n", self.start_utc.to_rfc3339()));
        log_content.push_str(&format!("Duration        : {} ms ({:.2} s)\n", report.total_duration_ms, report.total_duration_ms as f64 / 1000.0));
        log_content.push_str(&format!("Average Speed   : {:.2} MB/s\n", report.avg_throughput_mbps));
        log_content.push_str(&format!("Peak Speed      : {:.2} MB/s\n", report.peak_throughput_mbps));
        log_content.push_str(&format!("Bottleneck      : {}\n", report.primary_bottleneck));
        for rec in &report.recommendations {
            log_content.push_str(&format!("  * {}\n", rec));
        }
        log_content.push_str("\n--- Channels Breakdown ---\n");
        for ch in &report.channels {
            log_content.push_str(&format!(
                "  [{}] Chunks: {}, Bytes: {} ({:.2} MB/s), Socket Write: {:.1} us, Avg ACK Latency: {:.1} ms (P95: {:.1} ms), Max In-Flight: {}, NACKs: {}, Disconnects: {}\n",
                ch.channel_name, ch.chunks_transferred, ch.bytes_transferred, ch.throughput_mbps, ch.avg_socket_write_us, ch.avg_rtt_ms, ch.p95_rtt_ms, ch.max_in_flight, ch.nack_count, ch.disconnect_count
            ));
        }
        log_content.push_str("\n--- Stage Latencies ---\n");
        if self.role == TransferRole::Sender {
            log_content.push_str(&format!("  Sender Disk Read    : {:.1} MB/s (avg {:.1} us, p95 {:.1} us)\n", report.sender_disk_read_mbps, report.sender_disk_read_avg_us, report.sender_disk_read_p95_us));
            log_content.push_str(&format!("  Sender CPU Checksum : {:.1} MB/s (avg {:.1} us)\n", report.sender_checksum_mbps, report.sender_checksum_avg_us));
            log_content.push_str("  Receiver Disk Write : N/A (Sender Role Session)\n");
        } else {
            log_content.push_str("  Sender Disk Read    : N/A (Receiver Role Session)\n");
            log_content.push_str("  Sender CPU Checksum : N/A (Receiver Role Session)\n");
            log_content.push_str(&format!("  Receiver Disk Write : {:.1} MB/s (avg {:.1} us, p95 {:.1} us, max queue {})\n", report.receiver_disk_write_mbps, report.receiver_disk_write_avg_us, report.receiver_disk_write_p95_us, report.receiver_max_queue_depth));
        }
        log_content.push_str(&format!("  Receiver Finalize   : {} ms\n", report.receiver_finalize_ms));

        log_content.push_str("\n================================================================================\n");
        log_content.push_str(" Detailed Event Timeline\n");
        log_content.push_str("================================================================================\n");
        log_content.push_str(" REL_MS | LEVEL | STAGE       | CHANNEL        | MSG\n");
        log_content.push_str("--------------------------------------------------------------------------------\n");

        for ev in events {
            log_content.push_str(&format!(
                "{:>7} | {:<5} | {:<11} | {:<14} | {}\n",
                ev.relative_ms, ev.level, ev.stage, ev.channel, ev.message
            ));
        }

        std::fs::write(&log_path, log_content)?;

        #[cfg(target_os = "android")]
        {
            let public_dirs = [
                std::path::PathBuf::from("/storage/emulated/0/Download/TurboTransfer/logs"),
                std::path::PathBuf::from("/sdcard/Download/TurboTransfer/logs"),
            ];
            for pdir in &public_dirs {
                if std::fs::create_dir_all(pdir).is_ok() {
                    let _ = std::fs::copy(&json_path, pdir.join(format!("{}.json", id_str)));
                    let _ = std::fs::copy(&log_path, pdir.join(format!("{}.log", id_str)));
                    break;
                }
            }
        }

        Ok((json_path, log_path))
    }
}

fn calc_avg_p95(values: &[u64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let sum: u64 = values.iter().sum();
    let avg = sum as f64 / values.len() as f64;

    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let p95_idx = ((sorted.len() as f64 * 0.95).floor() as usize).min(sorted.len() - 1);
    let p95 = sorted[p95_idx] as f64;

    (avg, p95)
}

fn calc_avg_p95_f64(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let sum: f64 = values.iter().sum();
    let avg = sum / values.len() as f64;

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95_idx = ((sorted.len() as f64 * 0.95).floor() as usize).min(sorted.len() - 1);
    let p95 = sorted[p95_idx];

    (avg, p95)
}

// ---------------------------------------------------------------------------
// Global Telemetry Registry
// ---------------------------------------------------------------------------

struct GlobalTelemetryRegistry {
    sessions: Mutex<HashMap<Uuid, Arc<TransferTelemetry>>>,
}

static TELEMETRY_REGISTRY: std::sync::OnceLock<GlobalTelemetryRegistry> = std::sync::OnceLock::new();

fn get_telemetry_registry() -> &'static GlobalTelemetryRegistry {
    TELEMETRY_REGISTRY.get_or_init(|| GlobalTelemetryRegistry {
        sessions: Mutex::new(HashMap::new()),
    })
}

pub fn get_or_create_telemetry(
    transfer_id: Uuid,
    file_name: &str,
    file_size: u64,
    role: TransferRole,
) -> Arc<TransferTelemetry> {
    let reg = get_telemetry_registry();
    let mut map = reg.sessions.lock();
    map.entry(transfer_id)
        .or_insert_with(|| {
            Arc::new(TransferTelemetry::new(
                transfer_id,
                file_name.to_string(),
                file_size,
                role,
            ))
        })
        .clone()
}

pub fn get_telemetry(transfer_id: Uuid) -> Option<Arc<TransferTelemetry>> {
    let reg = get_telemetry_registry();
    let map = reg.sessions.lock();
    map.get(&transfer_id).cloned()
}

pub fn export_and_clean_telemetry(transfer_id: Uuid, data_dir: &Path) -> Option<(PathBuf, PathBuf)> {
    let reg = get_telemetry_registry();
    let telemetry = {
        let mut map = reg.sessions.lock();
        map.remove(&transfer_id)
    }?;

    match telemetry.export_log_files(data_dir) {
        Ok(paths) => {
            log::info!("[Telemetry] Exported transfer {} logs to {:?}", transfer_id, paths);
            Some(paths)
        }
        Err(e) => {
            log::error!("[Telemetry] Failed to export transfer {} logs: {}", transfer_id, e);
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Unified Logger Initialization (Android Logcat + Desktop Console)
// ---------------------------------------------------------------------------

struct TurboLogger;

impl log::Log for TurboLogger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    fn log(&self, record: &log::Record) {
        let target = record.target();
        let level = record.level();
        let args = record.args();

        #[cfg(target_os = "android")]
        {
            extern "C" {
                fn __android_log_write(prio: i32, tag: *const std::os::raw::c_char, text: *const std::os::raw::c_char) -> i32;
            }
            use std::ffi::CString;
            let tag = CString::new("TurboTransfer-Core").unwrap_or_default();
            let msg = CString::new(format!("[{}] {}", target, args)).unwrap_or_default();
            let prio = match level {
                log::Level::Error => 6, // ANDROID_LOG_ERROR
                log::Level::Warn => 5,  // ANDROID_LOG_WARN
                log::Level::Info => 4,  // ANDROID_LOG_INFO
                log::Level::Debug => 3, // ANDROID_LOG_DEBUG
                log::Level::Trace => 2, // ANDROID_LOG_VERBOSE
            };
            unsafe {
                __android_log_write(prio, tag.as_ptr(), msg.as_ptr());
            }
        }

        #[cfg(not(target_os = "android"))]
        {
            eprintln!("[{:5}] [{}] {}", level, target, args);
        }
    }

    fn flush(&self) {}
}

static LOGGER_INIT: std::sync::Once = std::sync::Once::new();

pub fn init_telemetry_logger() {
    LOGGER_INIT.call_once(|| {
        let logger = Box::leak(Box::new(TurboLogger));
        let _ = log::set_logger(logger);
        log::set_max_level(log::LevelFilter::Debug);
        log::info!("TurboTransfer logging and structured telemetry initialized");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_telemetry_event_recording_and_metrics() {
        let transfer_id = Uuid::new_v4();
        let telemetry = TransferTelemetry::new(transfer_id, "test_movie.mp4", 100 * 1024 * 1024, TransferRole::Sender);

        telemetry.record_event(TransferStage::Handshake, EventLevel::Info, "Control", None, None, None, "Handshake started", None);
        telemetry.record_chunk_read(0, 2 * 1024 * 1024, 2500, 1200);
        telemetry.record_chunk_sent("Wi-Fi", 0, 2 * 1024 * 1024, 3000);
        telemetry.record_chunk_ack("Wi-Fi", 0, 4.5);
        telemetry.record_finalize(12, true);
        telemetry.mark_completed();

        let report = telemetry.generate_report();
        assert_eq!(report.transfer_id, transfer_id);
        assert_eq!(report.file_name, "test_movie.mp4");
        assert_eq!(report.channels.len(), 1);
        assert_eq!(report.channels[0].channel_name, "Wi-Fi");
        assert_eq!(report.channels[0].chunks_transferred, 1);
        assert_eq!(report.channels[0].bytes_transferred, 2 * 1024 * 1024);
        assert!(report.sender_disk_read_avg_us > 0.0);
        assert!(report.sender_checksum_avg_us > 0.0);
    }

    #[test]
    fn test_receiver_disk_write_bottleneck_diagnosis() {
        let transfer_id = Uuid::new_v4();
        let telemetry = TransferTelemetry::new(transfer_id, "large.iso", 100 * 1024 * 1024, TransferRole::Receiver);

        // Simulate high disk write latency and deep queue
        for cid in 0..20 {
            telemetry.record_chunk_recv("Wi-Fi", cid, 2 * 1024 * 1024, 1500, 500);
            telemetry.record_disk_write(cid, 2 * 1024 * 1024, 85_000, 48); // 85ms write latency per chunk
        }
        telemetry.mark_completed();

        let report = telemetry.generate_report();
        assert_eq!(report.primary_bottleneck, "RECEIVER_DISK_WRITE_BOTTLENECK");
        assert!(report.recommendations.iter().any(|r| r.contains("flash write")));
    }

    #[test]
    fn test_sender_disk_read_bottleneck_diagnosis() {
        let transfer_id = Uuid::new_v4();
        let telemetry = TransferTelemetry::new(transfer_id, "data.bin", 50 * 1024 * 1024, TransferRole::Sender);

        // Simulate slow disk read (e.g. 5 MB/s) but fast network
        for cid in 0..10 {
            telemetry.record_chunk_read(cid, 2 * 1024 * 1024, 150_000, 1000); // 150ms read per chunk
            telemetry.record_chunk_sent("Wi-Fi", cid, 2 * 1024 * 1024, 2000);
            telemetry.record_chunk_ack("Wi-Fi", cid, 3.0);
        }
        telemetry.mark_completed();

        let report = telemetry.generate_report();
        assert_eq!(report.primary_bottleneck, "SENDER_DISK_READ_BOTTLENECK");
    }

    #[test]
    fn test_network_latency_jitter_diagnosis() {
        let transfer_id = Uuid::new_v4();
        let telemetry = TransferTelemetry::new(transfer_id, "network_test.bin", 50 * 1024 * 1024, TransferRole::Sender);

        // Fast disk read and hash, but huge RTT (120ms) and NACKs
        for cid in 0..10 {
            telemetry.record_chunk_read(cid, 2 * 1024 * 1024, 1000, 500);
            telemetry.record_chunk_sent("Wi-Fi", cid, 2 * 1024 * 1024, 2000);
            telemetry.record_chunk_ack("Wi-Fi", cid, 120.0);
        }
        telemetry.record_chunk_nack("Wi-Fi", 5, "packet drop");
        telemetry.mark_completed();

        let report = telemetry.generate_report();
        assert_eq!(report.primary_bottleneck, "NETWORK_LATENCY_JITTER");
    }

    #[test]
    fn test_export_log_files_to_disk() {
        let dir = tempdir().expect("tempdir");
        let transfer_id = Uuid::new_v4();
        let telemetry = TransferTelemetry::new(transfer_id, "file.zip", 1024 * 1024, TransferRole::Sender);

        telemetry.record_event(TransferStage::Init, EventLevel::Info, "Main", None, None, None, "Transfer session initialized", None);
        telemetry.record_chunk_read(0, 1024 * 1024, 500, 300);
        telemetry.record_chunk_sent("Wi-Fi", 0, 1024 * 1024, 1000);
        telemetry.record_chunk_ack("Wi-Fi", 0, 2.5);
        telemetry.mark_completed();

        let (json_path, log_path) = telemetry.export_log_files(dir.path()).expect("export");
        assert!(json_path.exists());
        assert!(log_path.exists());

        let json_str = std::fs::read_to_string(&json_path).expect("read json");
        assert!(json_str.contains("file.zip"));
        assert!(json_str.contains(&transfer_id.to_string()));

        let log_str = std::fs::read_to_string(&log_path).expect("read log");
        assert!(log_str.contains("Transfer session initialized"));
        assert!(log_str.contains("BOTTLENECK DIAGNOSTIC SUMMARY"));
    }
}

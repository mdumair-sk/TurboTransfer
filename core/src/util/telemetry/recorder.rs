use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Instant;
use uuid::Uuid;

use super::types::{BottleneckReport, EventLevel, TransferEvent, TransferStage};
use crate::manifest::TransferRole;

pub(crate) struct TelemetryChannelTracker {
    pub(crate) bytes: u64,
    pub(crate) chunks: u32,
    pub(crate) current_in_flight: u32,
    pub(crate) max_in_flight: u32,
    pub(crate) socket_write_durations_us: Vec<u64>,
    pub(crate) rtt_samples_ms: Vec<f64>,
    pub(crate) nacks: u64,
    pub(crate) disconnects: u64,
}

impl TelemetryChannelTracker {
    pub(crate) fn new() -> Self {
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
    pub(crate) end_time: Mutex<Option<Instant>>,
    pub events: Mutex<Vec<TransferEvent>>,

    // Sender read & hash stats
    pub(crate) read_durations_us: Mutex<Vec<u64>>,
    pub(crate) read_bytes_total: AtomicU64,
    pub(crate) hash_durations_us: Mutex<Vec<u64>>,
    pub(crate) hash_bytes_total: AtomicU64,

    // Receiver stats
    pub(crate) write_durations_us: Mutex<Vec<u64>>,
    pub(crate) write_bytes_total: AtomicU64,
    pub(crate) recv_verify_durations_us: Mutex<Vec<u64>>,
    pub(crate) max_queue_depth: AtomicU32,
    pub(crate) finalize_duration_ms: AtomicU64,
    pub(crate) duplicate_chunks: AtomicU32,

    // Per-channel stats
    pub(crate) channels: Mutex<HashMap<String, TelemetryChannelTracker>>,
    pub(crate) peak_throughput_mbps: Mutex<f64>,
    pub(crate) throughput_sampler: Mutex<(Instant, u64)>,
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

    pub fn get_peak_throughput_mbps(&self) -> f64 {
        *self.peak_throughput_mbps.lock()
    }

    pub fn get_channel_bytes_transferred(&self) -> HashMap<String, u64> {
        self.channels
            .lock()
            .iter()
            .map(|(k, v)| (k.clone(), v.bytes))
            .collect()
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
            EventLevel::Debug => {
                log::debug!("[{}] [{}] [{}] {}", self.transfer_id, stage, channel, msg)
            }
            EventLevel::Info => {
                log::info!("[{}] [{}] [{}] {}", self.transfer_id, stage, channel, msg)
            }
            EventLevel::Warn => {
                log::warn!("[{}] [{}] [{}] {}", self.transfer_id, stage, channel, msg)
            }
            EventLevel::Error => {
                log::error!("[{}] [{}] [{}] {}", self.transfer_id, stage, channel, msg)
            }
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
        if events.len() < 100_000 {
            events.push(event);
        }
    }

    pub fn record_chunk_read(&self, chunk_id: u32, bytes: u64, read_us: u64, hash_us: u64) {
        self.read_bytes_total.fetch_add(bytes, Ordering::Relaxed);
        self.hash_bytes_total.fetch_add(bytes, Ordering::Relaxed);

        {
            let mut read_list = self.read_durations_us.lock();
            if read_list.len() < 50_000 {
                read_list.push(read_us);
            }
        }

        {
            let mut hash_list = self.hash_durations_us.lock();
            if hash_list.len() < 50_000 {
                hash_list.push(hash_us);
            }
        }

        if chunk_id % 64 == 0 || chunk_id == 0 {
            self.record_event(
                TransferStage::DiskRead,
                EventLevel::Debug,
                "DiskReader",
                Some(chunk_id),
                Some(read_us),
                Some(bytes),
                format!("Read chunk #{} in {} us, hash in {} us", chunk_id, read_us, hash_us),
                None,
            );
        }
    }

    pub fn record_chunk_sent(
        &self,
        channel_name: &str,
        chunk_id: u32,
        bytes: u64,
        socket_write_us: u64,
    ) {
        let mut channels = self.channels.lock();
        let tracker = channels
            .entry(channel_name.to_string())
            .or_insert_with(TelemetryChannelTracker::new);
        tracker.bytes += bytes;
        tracker.chunks += 1;
        tracker.current_in_flight += 1;
        if tracker.current_in_flight > tracker.max_in_flight {
            tracker.max_in_flight = tracker.current_in_flight;
        }
        if tracker.socket_write_durations_us.len() < 50_000 {
            tracker.socket_write_durations_us.push(socket_write_us);
        }
        drop(channels);

        self.sample_throughput(bytes);

        if chunk_id % 64 == 0 || chunk_id == 0 {
            self.record_event(
                TransferStage::NetSend,
                EventLevel::Debug,
                channel_name,
                Some(chunk_id),
                Some(socket_write_us),
                Some(bytes),
                format!("Sent chunk #{} ({} bytes) in {} us", chunk_id, bytes, socket_write_us),
                None,
            );
        }
    }

    pub fn record_chunk_ack(
        &self,
        channel_name: &str,
        chunk_id: u32,
        ack_latency_ms: f64,
        bytes: u64,
    ) {
        {
            let mut channels = self.channels.lock();
            let tracker = channels
                .entry(channel_name.to_string())
                .or_insert_with(TelemetryChannelTracker::new);
            tracker.current_in_flight = tracker.current_in_flight.saturating_sub(1);
            if tracker.rtt_samples_ms.len() < 50_000 {
                tracker.rtt_samples_ms.push(ack_latency_ms);
            }
        }

        if chunk_id % 64 == 0 || chunk_id == 0 {
            self.record_event(
                TransferStage::NetAck,
                EventLevel::Debug,
                channel_name,
                Some(chunk_id),
                Some((ack_latency_ms * 1000.0) as u64),
                Some(bytes),
                format!("ACK for chunk #{} in {:.2} ms", chunk_id, ack_latency_ms),
                None,
            );
        }
    }

    pub fn record_chunk_nack(&self, channel_name: &str, chunk_id: u32, reason: &str) {
        {
            let mut channels = self.channels.lock();
            let tracker = channels
                .entry(channel_name.to_string())
                .or_insert_with(TelemetryChannelTracker::new);
            tracker.current_in_flight = tracker.current_in_flight.saturating_sub(1);
            tracker.nacks += 1;
        }

        self.record_event(
            TransferStage::NetAck,
            EventLevel::Warn,
            channel_name,
            Some(chunk_id),
            None,
            None,
            format!("NACK for chunk #{}: {}", chunk_id, reason),
            None,
        );
    }

    pub fn record_channel_disconnect(&self, channel_name: &str, reason: &str) {
        {
            let mut channels = self.channels.lock();
            let tracker = channels
                .entry(channel_name.to_string())
                .or_insert_with(TelemetryChannelTracker::new);
            tracker.disconnects += 1;
            tracker.current_in_flight = 0;
        }

        self.record_event(
            TransferStage::Connection,
            EventLevel::Warn,
            channel_name,
            None,
            None,
            None,
            format!("Channel disconnected: {}", reason),
            None,
        );
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
            format!("Ignored duplicate chunk #{}", chunk_id),
            None,
        );
    }

    pub fn record_chunk_recv(
        &self,
        channel_name: &str,
        chunk_id: u32,
        bytes: u64,
        recv_us: u64,
        verify_us: u64,
    ) {
        {
            let mut verify_list = self.recv_verify_durations_us.lock();
            if verify_list.len() < 50_000 {
                verify_list.push(verify_us);
            }
        }

        {
            let mut channels = self.channels.lock();
            let tracker = channels
                .entry(channel_name.to_string())
                .or_insert_with(TelemetryChannelTracker::new);
            tracker.bytes += bytes;
            tracker.chunks += 1;
        }

        self.sample_throughput(bytes);

        if chunk_id % 64 == 0 || chunk_id == 0 {
            self.record_event(
                TransferStage::NetRecv,
                EventLevel::Debug,
                channel_name,
                Some(chunk_id),
                Some(recv_us),
                Some(bytes),
                format!("Received chunk #{} ({} bytes) in {} us, verified in {} us", chunk_id, bytes, recv_us, verify_us),
                None,
            );
        }
    }

    pub fn record_disk_write(&self, chunk_id: u32, bytes: u64, write_us: u64, queue_depth: u32) {
        self.write_bytes_total.fetch_add(bytes, Ordering::Relaxed);
        {
            let mut write_list = self.write_durations_us.lock();
            if write_list.len() < 50_000 {
                write_list.push(write_us);
            }
        }

        let mut prev_max = self.max_queue_depth.load(Ordering::Relaxed);
        while queue_depth > prev_max {
            match self.max_queue_depth.compare_exchange_weak(
                prev_max,
                queue_depth,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => prev_max = actual,
            }
        }

        if chunk_id % 32 == 0 || chunk_id == 0 || queue_depth > 64 {
            self.record_event(
                TransferStage::DiskWrite,
                EventLevel::Debug,
                "ReceiverDisk",
                Some(chunk_id),
                Some(write_us),
                Some(bytes),
                format!(
                    "Wrote chunk #{} ({} bytes) to disk in {} us [queue depth: {}]",
                    chunk_id, bytes, write_us, queue_depth
                ),
                None,
            );
        }
    }

    pub fn record_finalize(&self, duration_ms: u64, crc_instant: bool) {
        self.finalize_duration_ms
            .store(duration_ms, Ordering::Relaxed);
        self.record_event(
            TransferStage::Finalize,
            EventLevel::Info,
            "Finalizer",
            None,
            Some(duration_ms * 1000),
            None,
            format!(
                "Transfer finalized in {} ms (in-flight CRC combine: {})",
                duration_ms, crc_instant
            ),
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
            format!(
                "Transfer completed: {} bytes in {} ms ({:.2} MB/s avg)",
                self.file_size, elapsed_ms, avg_mbps
            ),
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
        super::classifier::generate_bottleneck_report(self)
    }

    pub fn export_log_files(&self, data_dir: &Path) -> Result<(PathBuf, PathBuf), std::io::Error> {
        super::exporter::export_log_files(self, data_dir)
    }
}

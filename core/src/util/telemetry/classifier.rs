use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::recorder::TransferTelemetry;
use super::types::{BottleneckReport, ChannelMetric};
use crate::manifest::TransferRole;

pub(crate) fn calc_avg_p95(values: &[u64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let sum: u64 = values.iter().sum();
    let avg = sum as f64 / values.len() as f64;

    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let p95_idx = ((sorted.len() as f64 * 0.95).ceil() as usize).saturating_sub(1);
    let p95 = sorted.get(p95_idx).copied().unwrap_or(0) as f64;

    (avg, p95)
}

pub(crate) fn calc_avg_p95_f64(values: &[f64]) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let sum: f64 = values.iter().sum();
    let avg = sum / values.len() as f64;

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95_idx = ((sorted.len() as f64 * 0.95).ceil() as usize).saturating_sub(1);
    let p95 = sorted.get(p95_idx).copied().unwrap_or(0.0);

    (avg, p95)
}

pub fn generate_bottleneck_report(telemetry: &TransferTelemetry) -> BottleneckReport {
    let end_instant = telemetry.end_time.lock().unwrap_or_else(Instant::now);
    let total_duration_ms = end_instant
        .duration_since(telemetry.start_time)
        .as_millis()
        .max(1) as u64;
    let total_secs = total_duration_ms as f64 / 1000.0;

    let total_bytes = telemetry.file_size;
    let avg_throughput_mbps = (total_bytes as f64 / (1024.0 * 1024.0)) / total_secs;
    let peak_throughput_mbps = (*telemetry.peak_throughput_mbps.lock()).max(avg_throughput_mbps);

    // Sender Disk Read stats
    let read_list = telemetry.read_durations_us.lock().clone();
    let (read_avg_us, read_p95_us) = calc_avg_p95(&read_list);
    let read_bytes = telemetry.read_bytes_total.load(Ordering::Relaxed);
    let read_total_secs = (read_list.iter().sum::<u64>() as f64) / 1_000_000.0;
    let sender_disk_read_mbps = if read_total_secs > 0.0 {
        (read_bytes as f64 / (1024.0 * 1024.0)) / read_total_secs
    } else {
        0.0
    };

    // Sender Checksum stats
    let hash_list = telemetry.hash_durations_us.lock().clone();
    let (hash_avg_us, _) = calc_avg_p95(&hash_list);
    let hash_bytes = telemetry.hash_bytes_total.load(Ordering::Relaxed);
    let hash_total_secs = (hash_list.iter().sum::<u64>() as f64) / 1_000_000.0;
    let sender_checksum_mbps = if hash_total_secs > 0.0 {
        (hash_bytes as f64 / (1024.0 * 1024.0)) / hash_total_secs
    } else {
        0.0
    };

    // Receiver Disk Write stats
    let write_list = telemetry.write_durations_us.lock().clone();
    let (write_avg_us, write_p95_us) = calc_avg_p95(&write_list);
    let write_bytes = telemetry.write_bytes_total.load(Ordering::Relaxed);
    let write_total_secs = (write_list.iter().sum::<u64>() as f64) / 1_000_000.0;
    let receiver_disk_write_mbps = if write_total_secs > 0.0 {
        (write_bytes as f64 / (1024.0 * 1024.0)) / write_total_secs
    } else {
        0.0
    };
    let receiver_max_queue_depth = telemetry.max_queue_depth.load(Ordering::Relaxed);
    let receiver_finalize_ms = telemetry.finalize_duration_ms.load(Ordering::Relaxed);

    // Channels
    let mut channel_metrics = Vec::new();
    let channels_guard = telemetry.channels.lock();
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
        stage_durations_pct.insert(
            "DiskRead".to_string(),
            (read_sum_us as f64 / total_active_us) * 100.0,
        );
        stage_durations_pct.insert(
            "CpuChecksum".to_string(),
            (hash_sum_us as f64 / total_active_us) * 100.0,
        );
        stage_durations_pct.insert(
            "DiskWrite".to_string(),
            (write_sum_us as f64 / total_active_us) * 100.0,
        );
        stage_durations_pct.insert(
            "Finalize".to_string(),
            (finalize_us as f64 / total_active_us) * 100.0,
        );
    }

    // Bottleneck Diagnosis
    let mut recommendations = Vec::new();
    let primary_bottleneck;

    if telemetry.role == TransferRole::Receiver
        && (receiver_max_queue_depth >= 96 || write_p95_us > 50_000.0)
    {
        primary_bottleneck = "RECEIVER_DISK_WRITE_BOTTLENECK".to_string();
        recommendations.push(format!(
            "Receiver storage write latency averaged {:.1} ms (P95: {:.1} ms) and disk queue reached {}/128 chunks. The receiving drive/flash storage is the primary constraint.",
            write_avg_us / 1000.0, write_p95_us / 1000.0, receiver_max_queue_depth
        ));
    } else if telemetry.role == TransferRole::Sender
        && read_p95_us > 40_000.0
        && sender_disk_read_mbps < (avg_throughput_mbps * 1.1)
    {
        primary_bottleneck = "SENDER_DISK_READ_BOTTLENECK".to_string();
        recommendations.push(format!(
            "Sender disk read throughput ({:.1} MB/s) was slower than network capacity. Reading chunks took an average of {:.1} ms per chunk.",
            sender_disk_read_mbps, read_avg_us / 1000.0
        ));
    } else {
        let total_disconnects: u64 = channel_metrics.iter().map(|c| c.disconnect_count).sum();
        let total_nacks: u64 = channel_metrics.iter().map(|c| c.nack_count).sum();
        let max_rtt_avg: f64 = channel_metrics.iter().map(|c| c.avg_rtt_ms).fold(0.0, f64::max);
        let max_rtt_p95: f64 = channel_metrics.iter().map(|c| c.p95_rtt_ms).fold(0.0, f64::max);

        if total_disconnects > 0 || total_nacks > 3 {
            primary_bottleneck = "NETWORK_PACKET_CORRUPTION_OR_DROP".to_string();
            recommendations.push(format!(
                "Network packet corruption or disconnect detected (NACKs: {}, Disconnects: {}). Check physical USB connection or 5GHz Wi-Fi line-of-sight.",
                total_nacks, total_disconnects
            ));
        } else if total_nacks > 0
            || ((max_rtt_avg > 600.0 || max_rtt_p95 > 1000.0) && avg_throughput_mbps < 70.0)
        {
            primary_bottleneck = "NETWORK_LATENCY_JITTER".to_string();
            recommendations.push(format!(
                "Network latency or jitter detected (Avg RTT: {:.1} ms, P95: {:.1} ms, NACKs: {}). Round-trip latency is constraining sliding window pipeline efficiency.",
                max_rtt_avg, max_rtt_p95, total_nacks
            ));
        } else if hash_avg_us > 25_000.0 && sender_checksum_mbps < 200.0 {
            primary_bottleneck = "CPU_CHECKSUM_BOTTLENECK".to_string();
            recommendations.push(format!(
                "xxHash64 / CRC32C computation took {:.1} ms per chunk ({:.1} MB/s). CPU computation throttled the transfer pipeline.",
                hash_avg_us / 1000.0, sender_checksum_mbps
            ));
        } else if avg_throughput_mbps >= 70.0 {
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
        transfer_id: telemetry.transfer_id.to_string(),
        file_name: telemetry.file_name.clone(),
        file_size: telemetry.file_size,
        role: format!("{:?}", telemetry.role),
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

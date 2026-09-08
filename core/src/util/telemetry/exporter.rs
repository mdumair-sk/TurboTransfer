use std::path::{Path, PathBuf};

use super::recorder::TransferTelemetry;
use super::types::FullExport;
use crate::manifest::TransferRole;

/// Exports structured `.json` and human-readable `.log` files to `<data_dir>/logs/`.
pub fn export_log_files(
    telemetry: &TransferTelemetry,
    data_dir: &Path,
) -> Result<(PathBuf, PathBuf), std::io::Error> {
    let logs_dir = data_dir.join("logs");
    std::fs::create_dir_all(&logs_dir)?;

    let id_str = telemetry.transfer_id.to_string();
    let json_path = logs_dir.join(format!("{}.json", id_str));
    let log_path = logs_dir.join(format!("{}.log", id_str));

    let report = telemetry.generate_report();
    let events = telemetry.get_events(None);

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
    log_content.push_str(&format!("File Name       : {}\n", telemetry.file_name));
    log_content.push_str(&format!(
        "File Size       : {} bytes ({:.2} MB)\n",
        telemetry.file_size,
        telemetry.file_size as f64 / (1024.0 * 1024.0)
    ));
    log_content.push_str(&format!("Role            : {:?}\n", telemetry.role));
    log_content.push_str(&format!(
        "Start Time (UTC): {}\n",
        telemetry.start_utc.to_rfc3339()
    ));
    log_content.push_str(&format!(
        "Duration        : {} ms ({:.2} s)\n",
        report.total_duration_ms,
        report.total_duration_ms as f64 / 1000.0
    ));
    log_content.push_str(&format!(
        "Average Speed   : {:.2} MB/s\n",
        report.avg_throughput_mbps
    ));
    log_content.push_str(&format!(
        "Peak Speed      : {:.2} MB/s\n",
        report.peak_throughput_mbps
    ));
    log_content.push_str(&format!(
        "Bottleneck      : {}\n",
        report.primary_bottleneck
    ));
    for rec in &report.recommendations {
        log_content.push_str(&format!("  * {}\n", rec));
    }
    log_content.push_str("\n--- Channels Breakdown ---\n");
    for ch in &report.channels {
        if telemetry.role == TransferRole::Sender {
            log_content.push_str(&format!(
                "  [{}] Chunks: {}, Bytes: {} ({:.2} MB/s), Socket Write: {:.1} us, Avg ACK Latency: {:.1} ms (P95: {:.1} ms), Max In-Flight: {}, NACKs: {}, Disconnects: {}\n",
                ch.channel_name, ch.chunks_transferred, ch.bytes_transferred, ch.throughput_mbps, ch.avg_socket_write_us, ch.avg_rtt_ms, ch.p95_rtt_ms, ch.max_in_flight, ch.nack_count, ch.disconnect_count
            ));
        } else {
            log_content.push_str(&format!(
                "  [{}] Chunks: {}, Bytes: {} ({:.2} MB/s), NACKs: {}, Disconnects: {}\n",
                ch.channel_name,
                ch.chunks_transferred,
                ch.bytes_transferred,
                ch.throughput_mbps,
                ch.nack_count,
                ch.disconnect_count
            ));
        }
    }
    log_content.push_str("\n--- Stage Latencies ---\n");
    if telemetry.role == TransferRole::Sender {
        log_content.push_str(&format!(
            "  Sender Disk Read    : {:.1} MB/s (avg {:.1} us, p95 {:.1} us)\n",
            report.sender_disk_read_mbps,
            report.sender_disk_read_avg_us,
            report.sender_disk_read_p95_us
        ));
        log_content.push_str(&format!(
            "  Sender CPU Checksum : {:.1} MB/s (avg {:.1} us)\n",
            report.sender_checksum_mbps, report.sender_checksum_avg_us
        ));
        log_content.push_str("  Receiver Disk Write : N/A (Sender Role Session)\n");
    } else {
        log_content.push_str("  Sender Disk Read    : N/A (Receiver Role Session)\n");
        log_content.push_str("  Sender CPU Checksum : N/A (Receiver Role Session)\n");
        log_content.push_str(&format!(
            "  Receiver Disk Write : {:.1} MB/s (avg {:.1} us, p95 {:.1} us, max queue {})\n",
            report.receiver_disk_write_mbps,
            report.receiver_disk_write_avg_us,
            report.receiver_disk_write_p95_us,
            report.receiver_max_queue_depth
        ));
    }
    log_content.push_str(&format!(
        "  Receiver Finalize   : {} ms\n",
        report.receiver_finalize_ms
    ));

    log_content.push_str(
        "\n================================================================================\n",
    );
    log_content.push_str(" Detailed Event Timeline\n");
    log_content.push_str(
        "================================================================================\n",
    );
    log_content.push_str(" REL_MS | LEVEL | STAGE       | CHANNEL        | MSG\n");
    log_content.push_str(
        "--------------------------------------------------------------------------------\n",
    );

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

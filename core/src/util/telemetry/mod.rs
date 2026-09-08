pub mod classifier;
pub mod exporter;
pub mod logger;
pub mod recorder;
pub mod types;

pub use classifier::*;
pub use exporter::*;
pub use logger::*;
pub use recorder::*;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::TransferRole;
    use tempfile::tempdir;
    use uuid::Uuid;

    #[test]
    fn test_telemetry_event_recording_and_metrics() {
        let transfer_id = Uuid::new_v4();
        let telemetry = TransferTelemetry::new(
            transfer_id,
            "test_movie.mp4".to_string(),
            100 * 1024 * 1024,
            TransferRole::Sender,
        );

        telemetry.record_event(
            TransferStage::Handshake,
            EventLevel::Info,
            "Control",
            None,
            None,
            None,
            "Handshake started",
            None,
        );
        telemetry.record_chunk_read(0, 2 * 1024 * 1024, 2500, 1200);
        telemetry.record_chunk_sent("Wi-Fi", 0, 2 * 1024 * 1024, 3000);
        telemetry.record_chunk_ack("Wi-Fi", 0, 4.5, 2 * 1024 * 1024);
        telemetry.record_finalize(12, true);
        telemetry.mark_completed();

        let report = telemetry.generate_report();
        assert_eq!(report.transfer_id, transfer_id.to_string());
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
        let telemetry = TransferTelemetry::new(
            transfer_id,
            "large.iso".to_string(),
            100 * 1024 * 1024,
            TransferRole::Receiver,
        );

        // Simulate high disk write latency and deep queue
        for cid in 0..20 {
            telemetry.record_chunk_recv("Wi-Fi", cid, 2 * 1024 * 1024, 1500, 500);
            telemetry.record_disk_write(cid, 2 * 1024 * 1024, 85_000, 48); // 85ms write latency per chunk
        }
        telemetry.mark_completed();

        let report = telemetry.generate_report();
        assert_eq!(report.primary_bottleneck, "RECEIVER_DISK_WRITE_BOTTLENECK");
        assert!(report
            .recommendations
            .iter()
            .any(|r| r.contains("flash storage") || r.contains("drive/flash")));
    }

    #[test]
    fn test_sender_disk_read_bottleneck_diagnosis() {
        let transfer_id = Uuid::new_v4();
        let telemetry = TransferTelemetry::new(
            transfer_id,
            "data.bin".to_string(),
            50 * 1024 * 1024,
            TransferRole::Sender,
        );

        // Simulate slow disk read (e.g. 5 MB/s) but fast network
        for cid in 0..10 {
            telemetry.record_chunk_read(cid, 2 * 1024 * 1024, 150_000, 1000); // 150ms read per chunk
            telemetry.record_chunk_sent("Wi-Fi", cid, 2 * 1024 * 1024, 2000);
            telemetry.record_chunk_ack("Wi-Fi", cid, 3.0, 2 * 1024 * 1024);
        }
        telemetry.mark_completed();

        let report = telemetry.generate_report();
        assert_eq!(report.primary_bottleneck, "SENDER_DISK_READ_BOTTLENECK");
    }

    #[test]
    fn test_network_latency_jitter_diagnosis() {
        let transfer_id = Uuid::new_v4();
        let telemetry = TransferTelemetry::new(
            transfer_id,
            "network_test.bin".to_string(),
            50 * 1024 * 1024,
            TransferRole::Sender,
        );

        // Fast disk read and hash, but huge RTT (120ms) and NACKs
        for cid in 0..10 {
            telemetry.record_chunk_read(cid, 2 * 1024 * 1024, 1000, 500);
            telemetry.record_chunk_sent("Wi-Fi", cid, 2 * 1024 * 1024, 2000);
            telemetry.record_chunk_ack("Wi-Fi", cid, 120.0, 2 * 1024 * 1024);
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
        let telemetry = TransferTelemetry::new(
            transfer_id,
            "file.zip".to_string(),
            1024 * 1024,
            TransferRole::Sender,
        );

        telemetry.record_event(
            TransferStage::Init,
            EventLevel::Info,
            "Main",
            None,
            None,
            None,
            "Transfer session initialized",
            None,
        );
        telemetry.record_chunk_read(0, 1024 * 1024, 500, 300);
        telemetry.record_chunk_sent("Wi-Fi", 0, 1024 * 1024, 1000);
        telemetry.record_chunk_ack("Wi-Fi", 0, 2.5, 1024 * 1024);
        telemetry.mark_completed();

        let (json_path, log_path) = telemetry.export_log_files(dir.path()).expect("export");
        assert!(json_path.exists());
        assert!(log_path.exists());

        let json_str = std::fs::read_to_string(&json_path).expect("read json");
        assert!(json_str.contains("file.zip"));
        assert!(json_str.contains(&transfer_id.to_string()));

        let log_str = std::fs::read_to_string(&log_path).expect("read log");
        assert!(log_str.contains("Transfer session initialized"));
        assert!(log_str.contains("TurboTransfer Session Log"));
    }
}

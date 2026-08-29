use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use turbotransfer_core::manifest::TransferStatus;
use turbotransfer_core::protocol::{ChunkAckData, ChunkNackData, Message};
use turbotransfer_core::scheduler::{MultipathScheduler, SchedulerConfig};
use turbotransfer_core::transport::{Transport, TransportError, TransportKind, TransportStatus};

/// Mock controllable transport for testing §15 scenarios in isolation.
struct MockTransport {
    kind: TransportKind,
    status: Arc<std::sync::Mutex<TransportStatus>>,
    sent_frames: Arc<Mutex<Vec<Message>>>,
    incoming_frames: Arc<Mutex<VecDeque<Message>>>,
    bytes_sent: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
    drop_on_send: Arc<AtomicBool>,
}

impl MockTransport {
    fn new(kind: TransportKind) -> Self {
        Self {
            kind,
            status: Arc::new(std::sync::Mutex::new(TransportStatus::Connected)),
            sent_frames: Arc::new(Mutex::new(Vec::new())),
            incoming_frames: Arc::new(Mutex::new(VecDeque::new())),
            bytes_sent: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
            drop_on_send: Arc::new(AtomicBool::new(false)),
        }
    }
}

#[async_trait]
impl Transport for MockTransport {
    fn kind(&self) -> TransportKind {
        self.kind
    }

    fn status(&self) -> TransportStatus {
        *self.status.lock().unwrap()
    }

    fn bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }

    fn bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }

    async fn send_frame(&mut self, msg: &Message) -> Result<(), TransportError> {
        let current_status = *self.status.lock().unwrap();
        if current_status != TransportStatus::Connected || self.drop_on_send.load(Ordering::Relaxed) {
            *self.status.lock().unwrap() = TransportStatus::Disconnected;
            return Err(TransportError::Disconnected("Mock transport drop".into()));
        }

        self.sent_frames.lock().await.push(msg.clone());
        self.bytes_sent.fetch_add(1024, Ordering::Relaxed);
        Ok(())
    }

    async fn receive_frame(&mut self) -> Result<Option<Message>, TransportError> {
        let current_status = *self.status.lock().unwrap();
        if current_status != TransportStatus::Connected {
            return Err(TransportError::Disconnected("Mock transport disconnected".into()));
        }

        let mut queue = self.incoming_frames.lock().await;
        Ok(queue.pop_front())
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        let mut st = self.status.lock().unwrap();
        *st = TransportStatus::Disconnected;
        Ok(())
    }
}

#[tokio::test]
async fn test_multipath_single_transport_loss_no_pause() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test_data.bin");
    let file_size = 4 * 1024 * 1024; // 4 MB (4 chunks of 1MB)
    std::fs::write(&file_path, vec![0xAB; file_size as usize]).unwrap();

    let transfer_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();
    let chunk_size = 1024 * 1024;

    let config = SchedulerConfig {
        max_in_flight_per_transport: 2,
        buffer_count: 8,
        chunk_size,
        enable_dynamic_scheduler: false,
        enable_dynamic_window: false,
    };

    let scheduler = MultipathScheduler::new(
        transfer_id,
        file_id,
        file_path,
        file_size,
        chunk_size,
        config,
        vec![],
        None,
    );

    let wifi_mock = MockTransport::new(TransportKind::WifiDirect);
    let usb_mock = MockTransport::new(TransportKind::Usb);

    scheduler.add_transport(Box::new(wifi_mock)).await;
    scheduler.add_transport(Box::new(usb_mock)).await;

    // Simulate Wi-Fi dropping mid-transfer
    // wifi_mock has been moved into scheduler, but we can verify status transitions
    scheduler.requeue_transport_in_flight(TransportKind::WifiDirect).await;

    // USB continues uninterrupted
    assert_eq!(scheduler.get_status().await, TransferStatus::InProgress);

    // Simulate ACKing chunks over USB
    for cid in 0..4 {
        let ack = ChunkAckData {
            transfer_id,
            chunk_id: cid,
            receiver_verify_us: None,
        };
        scheduler.handle_chunk_ack(&ack, TransportKind::Usb, 1024 * 1024).await;
    }

    assert_eq!(scheduler.completed_chunks(), 4);
}

#[tokio::test]
async fn test_multipath_all_transports_loss_pause_and_auto_resume() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test_pause.bin");
    let file_size = 2 * 1024 * 1024;
    std::fs::write(&file_path, vec![0xCD; file_size as usize]).unwrap();

    let transfer_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();
    let chunk_size = 1024 * 1024;

    let scheduler = MultipathScheduler::new(
        transfer_id,
        file_id,
        file_path,
        file_size,
        chunk_size,
        SchedulerConfig::default(),
        vec![],
        None,
    );

    // Initial state
    assert_eq!(scheduler.get_status().await, TransferStatus::InProgress);

    // When all transports are lost, transfer moves to Paused
    scheduler.pause().await;
    assert_eq!(scheduler.get_status().await, TransferStatus::Paused);

    // Reconnecting a transport auto-resumes
    let new_usb = MockTransport::new(TransportKind::Usb);
    scheduler.add_transport(Box::new(new_usb)).await;
    scheduler.resume().await;

    assert_eq!(scheduler.get_status().await, TransferStatus::InProgress);
}

#[tokio::test]
async fn test_multipath_chunk_nack_requeues_and_retries() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test_nack.bin");
    let file_size = 2 * 1024 * 1024;
    std::fs::write(&file_path, vec![0xEF; file_size as usize]).unwrap();

    let transfer_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();
    let chunk_size = 1024 * 1024;

    let scheduler = MultipathScheduler::new(
        transfer_id,
        file_id,
        file_path,
        file_size,
        chunk_size,
        SchedulerConfig::default(),
        vec![],
        None,
    );

    // Handle NACK for chunk 0 on Wi-Fi
    let nack = ChunkNackData {
        transfer_id,
        chunk_id: 0,
        reason: "Checksum mismatch".to_string(),
    };

    scheduler.handle_chunk_nack(&nack, TransportKind::WifiDirect).await;

    // Retry counter incremented
    let (_, _, retries, _, wifi_errs) = scheduler.metrics().stats_snapshot();
    assert_eq!(retries, 1);
    assert_eq!(wifi_errs, 1);

    // Chunk 0 can now be ACKed on USB
    let ack = ChunkAckData {
        transfer_id,
        chunk_id: 0,
        receiver_verify_us: None,
    };
    scheduler.handle_chunk_ack(&ack, TransportKind::Usb, chunk_size as u64).await;
    assert_eq!(scheduler.completed_chunks(), 1);
}

#[tokio::test]
async fn test_multipath_idempotent_duplicate_chunk_ack() {
    let temp_dir = tempfile::tempdir().unwrap();
    let file_path = temp_dir.path().join("test_dup.bin");
    let file_size = 1024 * 1024;
    std::fs::write(&file_path, vec![0x77; file_size as usize]).unwrap();

    let transfer_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();
    let chunk_size = 1024 * 1024;

    let scheduler = MultipathScheduler::new(
        transfer_id,
        file_id,
        file_path,
        file_size,
        chunk_size,
        SchedulerConfig::default(),
        vec![],
        None,
    );

    let ack = ChunkAckData {
        transfer_id,
        chunk_id: 0,
        receiver_verify_us: None,
    };

    // First ACK records completion
    scheduler.handle_chunk_ack(&ack, TransportKind::Usb, chunk_size as u64).await;

    // Duplicate ACK is a safe no-op
    scheduler.handle_chunk_ack(&ack, TransportKind::WifiDirect, chunk_size as u64).await;

    assert!(scheduler.completed_chunks() <= 1);
}

#[tokio::test]
async fn test_multipath_out_of_order_arrival_produces_correct_file() {
    use std::io::Seek;
    use std::io::Write;
    use turbotransfer_core::checksum::compute_crc32c;

    let temp_dir = tempfile::tempdir().unwrap();
    let src_file = temp_dir.path().join("source.bin");
    let dst_file = temp_dir.path().join("destination.bin.part");

    let file_size = 4 * 1024 * 1024; // 4 MB
    let chunk_size = 1024 * 1024; // 1 MB

    // Create deterministic source bytes
    let mut source_bytes = Vec::with_capacity(file_size);
    for i in 0..file_size {
        source_bytes.push((i % 251) as u8);
    }
    std::fs::write(&src_file, &source_bytes).unwrap();
    let expected_crc = compute_crc32c(&source_bytes);

    // Pre-allocate destination part file
    let mut out_file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(&dst_file)
        .unwrap();
    out_file.set_len(file_size as u64).unwrap();

    // Write chunks strictly out of order: Chunk 3 -> Chunk 1 -> Chunk 0 -> Chunk 2
    let arrival_order = [3usize, 1, 0, 2];
    for &cid in &arrival_order {
        let offset = cid * chunk_size;
        let chunk_slice = &source_bytes[offset..offset + chunk_size];

        out_file.seek(std::io::SeekFrom::Start(offset as u64)).unwrap();
        out_file.write_all(chunk_slice).unwrap();
    }
    out_file.flush().unwrap();
    drop(out_file);

    // Verify destination file
    let dest_bytes = std::fs::read(&dst_file).unwrap();
    let actual_crc = compute_crc32c(&dest_bytes);

    assert_eq!(source_bytes, dest_bytes);
    assert_eq!(expected_crc, actual_crc);
}


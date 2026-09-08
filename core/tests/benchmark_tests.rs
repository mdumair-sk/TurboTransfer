use std::path::PathBuf;
use uuid::Uuid;

use turbotransfer_core::benchmark::{
    clear_saved_calibration, get_saved_calibration, save_calibration, EphemeralFile,
    TransferConfigOverride, TransferPurpose, WindowPreset,
};
use turbotransfer_core::protocol::{
    Message, TransferOfferData, MSG_TYPE_TRANSFER_OFFER,
};

#[test]
fn benchmark_test_ephemeral_file_creation_and_cleanup() {
    let size_bytes: u64 = 4 * 1024 * 1024; // 4 MB
    let path: PathBuf;

    {
        let ephemeral = EphemeralFile::create(size_bytes, None)
            .expect("Failed to create ephemeral file");
        path = ephemeral.path.clone();

        assert!(path.exists(), "Ephemeral file should exist on disk");
        let metadata = std::fs::metadata(&path).expect("Failed to read metadata");
        assert_eq!(metadata.len(), size_bytes, "Ephemeral file size mismatch");

        // Verify deterministic xorshift64 non-zero data
        let content = std::fs::read(&path).expect("Failed to read ephemeral content");
        assert_eq!(content.len() as u64, size_bytes);
        assert!(content.iter().any(|&b| b != 0), "Payload should not be all zeroes");
    }

    // After ephemeral falls out of scope, Drop must have removed it
    assert!(!path.exists(), "Ephemeral file should be deleted on drop");
}

#[test]
fn benchmark_test_config_store_save_get_clear() {
    let test_peer = "test-peer-phone-node";

    let config = TransferConfigOverride {
        wifi_stream_count: Some(4),
        chunk_size_bytes: Some(2 * 1024 * 1024),
        wifi_window_preset: Some(WindowPreset::Max),
    };

    let expected_speed = 87.5;
    save_calibration(test_peer, config.clone(), expected_speed)
        .expect("Failed to save calibration");

    let loaded = get_saved_calibration(test_peer)
        .expect("Saved calibration not found");

    assert_eq!(loaded.device_pair_id, test_peer);
    assert_eq!(loaded.config.wifi_stream_count, Some(4));
    assert_eq!(loaded.config.chunk_size_bytes, Some(2 * 1024 * 1024));
    assert_eq!(loaded.config.wifi_window_preset, Some(WindowPreset::Max));
    assert!((loaded.expected_speed_mbps - expected_speed).abs() < f64::EPSILON);

    clear_saved_calibration(test_peer).expect("Failed to clear calibration");
    assert!(
        get_saved_calibration(test_peer).is_none(),
        "Calibration should be None after clearing"
    );
}

#[derive(serde::Serialize)]
struct LegacyOfferTest {
    transfer_id: Uuid,
    file_id: Uuid,
    file_name: String,
    file_size: u64,
    chunk_size: u32,
    total_chunks: u32,
    checksum_algo: String,
}

#[test]
fn benchmark_test_wire_protocol_backward_compatibility() {
    let transfer_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();

    // 1. Simulate legacy client sending TransferOffer without 'purpose' field
    let legacy = LegacyOfferTest {
        transfer_id,
        file_id,
        file_name: "legacy.dat".into(),
        file_size: 1024,
        chunk_size: 512,
        total_chunks: 2,
        checksum_algo: "xxhash64".into(),
    };
    let legacy_bytes = bincode::serialize(&legacy).unwrap();

    let decoded = Message::decode_payload(MSG_TYPE_TRANSFER_OFFER, &legacy_bytes).unwrap();
    match decoded {
        Message::TransferOffer(offer) => {
            assert_eq!(offer.file_name, "legacy.dat");
            assert_eq!(offer.purpose, TransferPurpose::Normal);
        }
        _ => panic!("Expected TransferOffer"),
    }

    // 2. Modern client sending TransferOffer with TransferPurpose::Benchmark
    let modern = TransferOfferData {
        transfer_id,
        file_id,
        file_name: "benchmark.dat".into(),
        file_size: 2048,
        chunk_size: 1024,
        total_chunks: 2,
        checksum_algo: "xxhash64".into(),
        purpose: TransferPurpose::Benchmark,
    };
    let modern_bytes = bincode::serialize(&modern).unwrap();

    let decoded = Message::decode_payload(MSG_TYPE_TRANSFER_OFFER, &modern_bytes).unwrap();
    match decoded {
        Message::TransferOffer(offer) => {
            assert_eq!(offer.file_name, "benchmark.dat");
            assert_eq!(offer.purpose, TransferPurpose::Benchmark);
        }
        _ => panic!("Expected TransferOffer"),
    }
}

#[test]
fn benchmark_test_calibration_key_normalization_and_cancellation() {
    use turbotransfer_core::benchmark::{cancel_calibration, get_pair_key};

    // BUG-01 verification: empty or whitespace keys normalize to "default_peer"
    assert_eq!(get_pair_key(""), "default_peer");
    assert_eq!(get_pair_key("   "), "default_peer");
    assert_eq!(get_pair_key("192.168.43.1:9876"), "192_168_43_1_9876");

    let config = TransferConfigOverride {
        wifi_stream_count: Some(3),
        chunk_size_bytes: Some(1024 * 1024),
        wifi_window_preset: Some(WindowPreset::Balanced),
    };

    // Save under empty string key (simulating default peer auto-detect)
    save_calibration("", config.clone(), 92.4).expect("Failed to save default calibration");

    // Retrieve under "default_peer" key
    let loaded = get_saved_calibration("default_peer").expect("Should load under default_peer");
    assert_eq!(loaded.device_pair_id, "default_peer");
    assert_eq!(loaded.config.wifi_stream_count, Some(3));

    // Retrieve under empty string key
    let loaded_empty = get_saved_calibration("").expect("Should load under empty string");
    assert_eq!(loaded_empty.device_pair_id, "default_peer");

    // Cancellation with empty string should safely resolve and no-op without panic
    cancel_calibration("");
    cancel_calibration("   ");

    // Clean up
    clear_saved_calibration("").expect("Failed to clear default calibration");
    assert!(get_saved_calibration("").is_none());
}

#[tokio::test]
async fn benchmark_test_meta_actor_atomic_write_recovery() {
    use std::fs;
    use tempfile::tempdir;
    use turbotransfer_core::manifest::{MetaActor, TransferMeta, TransferRole, TransportType};

    let dir = tempdir().unwrap();
    let meta_path = dir.path().join("meta.json");
    let transfer_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();
    let peer_id = Uuid::new_v4();

    let initial_meta = TransferMeta::new(
        transfer_id,
        file_id,
        "test_atomic.bin".to_string(),
        4096,
        2048,
        2,
        TransferRole::Sender,
        peer_id,
    );

    let (handle, join_handle) = MetaActor::spawn(
        meta_path.clone(),
        initial_meta,
        64,
    );

    // Send completed chunks
    handle.send_chunk_completed(0, TransportType::Usb, 2048).await;
    handle.send_chunk_completed(1, TransportType::Usb, 2048).await;

    // Wait briefly for actor to process and flush
    tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;

    // Pause to trigger clean exit and async flush
    handle.pause().await;
    let _ = join_handle.await;

    assert!(meta_path.exists(), "meta.json must exist");
    let content = fs::read_to_string(&meta_path).unwrap();
    assert!(!content.is_empty(), "meta.json must not be empty");

    // Verify valid JSON
    let meta: serde_json::Value = serde_json::from_str(&content).expect("meta.json must be valid JSON");
    assert_eq!(meta["file_name"], "test_atomic.bin");
    assert_eq!(meta["status"], "paused");

    // Verify no temporary files were leaked in the directory
    let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().flatten().collect();
    assert_eq!(entries.len(), 1, "Only meta.json should remain; no tmp files leaked");
}

#[test]
fn benchmark_test_ephemeral_registry_lifecycle() {
    use turbotransfer_core::manifest::{TransferRole, TransferStatus};
    use turbotransfer_core::transfer::api::{
        register_active_transfer_with_path, remove_active_transfer, set_transfer_status,
        transfer_control_status,
    };

    let transfer_id = Uuid::new_v4();

    // Register benchmark transfer in registry (BUG-06 fix)
    register_active_transfer_with_path(
        transfer_id,
        "benchmark_payload.bin".to_string(),
        250 * 1024 * 1024,
        TransferRole::Sender,
        250,
        "Benchmark".to_string(),
        None,
        0,
        0,
    );

    // Status must be observable
    assert_eq!(
        transfer_control_status(transfer_id),
        Some(TransferStatus::InProgress)
    );

    // Cancel transfer
    set_transfer_status(transfer_id, TransferStatus::Cancelled, None);
    assert_eq!(
        transfer_control_status(transfer_id),
        Some(TransferStatus::Cancelled)
    );

    // RAII Cleanup verification (registry.transfers removal)
    remove_active_transfer(transfer_id);
    assert_eq!(transfer_control_status(transfer_id), None);
}

#[tokio::test]
async fn benchmark_test_e2e_loopback_run() {
    let temp_dest = tempfile::tempdir().unwrap();
    let addr = "127.0.0.1:9931";

    let _receiver_handle = turbotransfer_core::transfer::api::enter_receive_mode(
        Some(addr.to_string()),
        temp_dest.path().to_path_buf(),
    )
    .await
    .expect("Failed to start receiver on 127.0.0.1:9931");

    let result = turbotransfer_core::transfer::api::run_benchmark_with_address(
        None,
        Some(addr),
        turbotransfer_core::transfer::api::TransportPreference::Automatic,
        5,
    )
    .await
    .expect("Benchmark run failed");

    assert!(result.throughput_mbps > 0.0, "Throughput must be > 0");
    assert!(result.duration_ms > 0, "Duration must be > 0");
    assert_eq!(
        result.bytes_transferred,
        5 * 1024 * 1024,
        "Bytes transferred should be 5 MB"
    );

    // Stop receiver
    turbotransfer_core::transfer::api::leave_receive_mode(Some(addr));

    // Verify receiver destination directory has no leaked payload or part files
    let remaining_files: Vec<_> = std::fs::read_dir(temp_dest.path())
        .unwrap()
        .flatten()
        .collect();
    assert!(
        remaining_files.is_empty(),
        "Receiver destination directory should have no leaked files upon completion"
    );
}

#[tokio::test]
async fn calibration_test_e2e_loopback_sweep() {
    // Set 1 MB step size for fast execution in test environment
    std::env::set_var("TURBOTRANSFER_CALIBRATION_STEP_MB", "1");

    let temp_dest = tempfile::tempdir().unwrap();
    let addr = "127.0.0.1:9932";

    let _receiver_handle = turbotransfer_core::transfer::api::enter_receive_mode(
        Some(addr.to_string()),
        temp_dest.path().to_path_buf(),
    )
    .await
    .expect("Failed to start receiver on 127.0.0.1:9932");

    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;
    use parking_lot::Mutex;

    struct TestCallback {
        steps_seen: Arc<AtomicU32>,
        stages_seen: Arc<Mutex<Vec<String>>>,
    }

    impl turbotransfer_core::benchmark::CalibrationProgressCallback for TestCallback {
        fn on_progress(&self, update: turbotransfer_core::benchmark::CalibrationProgressUpdate) {
            self.steps_seen.store(update.current_step, Ordering::SeqCst);
            self.stages_seen.lock().push(update.stage);
        }
    }

    let steps = Arc::new(AtomicU32::new(0));
    let stages = Arc::new(Mutex::new(Vec::new()));

    let callback = Box::new(TestCallback {
        steps_seen: steps.clone(),
        stages_seen: stages.clone(),
    });

    let res = turbotransfer_core::benchmark::run_calibration(
        None,
        Some(addr),
        Some(callback),
    )
    .await
    .expect("Calibration sweep failed");

    assert_eq!(res.all_candidates.len(), 10, "Should have 10 candidate results");
    assert_eq!(steps.load(Ordering::SeqCst), 10, "Should have observed step 10");

    let recorded_stages = stages.lock().clone();
    assert!(recorded_stages.contains(&"streams".to_string()));
    assert!(recorded_stages.contains(&"chunk_size".to_string()));
    assert!(recorded_stages.contains(&"window".to_string()));
    assert!(recorded_stages.contains(&"confirmation".to_string()));

    // Verify saved calibration
    let pair_key = turbotransfer_core::benchmark::get_pair_key(addr);
    let saved = turbotransfer_core::benchmark::get_saved_calibration(&pair_key)
        .expect("Saved calibration should be retrievable");
    assert_eq!(saved.device_pair_id, pair_key);
    assert_eq!(saved.config, res.best_config);

    // Verify cancellation halts execution cleanly
    struct CancelCallback {
        target: String,
    }
    impl turbotransfer_core::benchmark::CalibrationProgressCallback for CancelCallback {
        fn on_progress(&self, update: turbotransfer_core::benchmark::CalibrationProgressUpdate) {
            if update.current_step == 2 {
                turbotransfer_core::benchmark::cancel_calibration(&self.target);
            }
        }
    }

    let cancel_cb = Box::new(CancelCallback {
        target: addr.to_string(),
    });
    let cancel_res = turbotransfer_core::benchmark::run_calibration(
        None,
        Some(addr),
        Some(cancel_cb),
    )
    .await;

    assert!(
        matches!(cancel_res, Err(turbotransfer_core::transfer::session::TransferSessionError::Cancelled)),
        "Calibration should return Cancelled error"
    );

    // Clean up calibration config
    let _ = turbotransfer_core::benchmark::clear_saved_calibration(&pair_key);

    turbotransfer_core::transfer::api::leave_receive_mode(Some(addr));
}

#[tokio::test]
#[ignore]
async fn benchmark_test_physical_dual_channel_multipath() {
    let dual_addr = std::env::var("TURBOTRANSFER_TEST_PEER_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:9876, 192.168.1.11:9876".to_string());
    let size_mb: u32 = std::env::var("TURBOTRANSFER_BENCH_SIZE_MB")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);

    println!("==================================================");
    println!("PHYSICAL DUAL-CHANNEL MULTIPATH BENCHMARK PUSH");
    println!("Targets: {}", dual_addr);
    println!("Payload: {} MB ({} bytes)", size_mb, (size_mb as u64) * 1024 * 1024);
    println!("Mode: Combined (Simultaneous Bonded Multi-Channel)");
    println!("==================================================");

    let chunk_size_env: Option<u32> = std::env::var("TURBOTRANSFER_BENCH_CHUNK_SIZE")
        .ok()
        .and_then(|s| s.parse().ok());
    let config = turbotransfer_core::benchmark::TransferConfigOverride {
        wifi_stream_count: Some(4),
        chunk_size_bytes: chunk_size_env.or(Some(1024 * 1024)),
        wifi_window_preset: Some(turbotransfer_core::benchmark::WindowPreset::Max),
    };
    let result = turbotransfer_core::benchmark::run_benchmark_transfer(
        None,
        Some(&dual_addr),
        turbotransfer_core::transfer::api::TransportPreference::Combined,
        size_mb,
        Some(&config),
        turbotransfer_core::benchmark::TransferPurpose::Benchmark,
        None,
    )
    .await
    .expect("Dual-channel multipath benchmark failed");

    println!("==================================================");
    println!("DUAL-CHANNEL RESULTS OBSERVED:");
    println!("Total Aggregate Speed : {:.2} MB/s ({:.2} Mbps)", result.throughput_mbps, result.throughput_mbps * 8.0);
    println!("Peak Burst Speed      : {:.2} MB/s ({:.2} Mbps)", result.peak_speed_mbps, result.peak_speed_mbps * 8.0);
    println!("USB Channel Average   : {:.2} MB/s", result.usb_avg_mbps);
    println!("Wi-Fi Channel Average : {:.2} MB/s", result.wifi_avg_mbps);
    println!("Bytes Transferred     : {}", result.bytes_transferred);
    println!("Total Duration        : {} ms", result.duration_ms);
    println!("==================================================");

    assert!(result.throughput_mbps > 0.0);
    assert_eq!(result.bytes_transferred, (size_mb as u64) * 1024 * 1024);
}

#[tokio::test]
#[ignore]
async fn benchmark_test_pc_receiver_listener() {
    let dest_dir = std::env::temp_dir().join("turbotransfer_pc_recv");
    std::fs::create_dir_all(&dest_dir).unwrap();
    let addr = "0.0.0.0:9876".to_string();
    let _receiver = turbotransfer_core::transfer::api::enter_receive_mode(Some(addr), dest_dir).await.unwrap();
    println!("PC Receiver listening on 0.0.0.0:9876! Ready for phone incoming transfers.");
    for _ in 0..120 {
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    }
}

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;
use tokio::io::duplex;
use uuid::Uuid;

use turbotransfer_core::checksum::{
    compute_crc32c, compute_file_crc32c, compute_xxhash64, crc32c_combine, Crc32cAccumulator,
};
use turbotransfer_core::chunk::calculate_chunk_plan;
use turbotransfer_core::protocol::{
    encode_frame_parts, ChunkDataPayload, FrameReader, Message,
};
use turbotransfer_core::transfer::api::{
    enter_receive_mode, leave_receive_mode, start_transfer, transfer_control_status,
    TransportPreference, DEFAULT_WIFI_PARALLEL_STREAMS,
};
use turbotransfer_core::transport::vectored::write_all_vectored;
use turbotransfer_core::transport::Transport;
use turbotransfer_core::util::storage::{advise_sequential_read, open_sequential_read};

/// Test 1: Validate mathematical equivalence of GF(2) matrix CRC32C combination (§6.2).
#[test]
fn test_crc32c_combine_mathematical_equivalence() {
    // Test diverse slice lengths to verify all matrix squaring bits (1 to 2^24)
    let sizes = [1, 7, 16, 64, 255, 1024, 4096, 65536, 1048576, 4194304];

    for &size1 in &sizes {
        for &size2 in &sizes {
            let data1: Vec<u8> = (0..size1).map(|i| (i * 31 % 256) as u8).collect();
            let data2: Vec<u8> = (0..size2).map(|i| (i * 37 % 256) as u8).collect();
            let combined_data = [data1.as_slice(), data2.as_slice()].concat();

            let crc1 = compute_crc32c(&data1);
            let crc2 = compute_crc32c(&data2);
            let expected_combined_crc = compute_crc32c(&combined_data);

            let actual_combined_crc = crc32c_combine(crc1, crc2, size2);
            assert_eq!(
                actual_combined_crc, expected_combined_crc,
                "Failed CRC32C combine for size1={}, size2={}",
                size1, size2
            );
        }
    }
}

/// Test 2: Validate O(1) in-flight out-of-order chunk checksum finalization (§6.2).
#[test]
fn test_o1_receiver_out_of_order_checksum_finalization() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("source_test_file.bin");

    let chunk_size = 512 * 1024; // 512 KiB
    let num_chunks = 8;
    let total_size = chunk_size * num_chunks;

    let mut full_data = Vec::with_capacity(total_size);
    for i in 0..total_size {
        full_data.push((i * 17 % 256) as u8);
    }

    {
        let mut f = File::create(&file_path).unwrap();
        f.write_all(&full_data).unwrap();
    }

    let whole_file_expected_crc = compute_file_crc32c(&file_path).unwrap();

    // Chunk the file
    let plan = calculate_chunk_plan(total_size as u64, chunk_size as u32);
    let mut chunk_slices = Vec::new();
    for entry in &plan {
        let start = entry.file_offset as usize;
        let end = start + entry.payload_length as usize;
        let slice = &full_data[start..end];
        let crc = compute_crc32c(slice);
        chunk_slices.push((entry.chunk_id, crc, slice.len()));
    }

    // Simulate chunks arriving out-of-order (e.g. from 4 parallel bonded streams)
    let arrival_order = [3, 0, 7, 1, 5, 2, 6, 4];
    let mut receiver_chunk_table = HashMap::new();

    for &chunk_idx in &arrival_order {
        let (cid, crc, len) = chunk_slices[chunk_idx];
        receiver_chunk_table.insert(cid, (crc, len));
    }

    // Perform O(1) in-memory combination across chunk table
    let mut acc = Crc32cAccumulator::new();
    for cid in 0..num_chunks as u32 {
        let (crc, len) = receiver_chunk_table.get(&cid).unwrap();
        acc.combine(*crc, *len);
    }

    let calculated_combined_crc = acc.finalize();
    assert_eq!(
        calculated_combined_crc, whole_file_expected_crc,
        "Out-of-order in-flight combined CRC must match whole-file CRC bit-for-bit"
    );
    assert_eq!(acc.total_bytes(), total_size as u64);
}

/// Test 3: Validate Vectored I/O zero-copy framing and decoding (§6.4).
#[tokio::test]
async fn test_vectored_io_framing_integrity() {
    let (mut client_writer, server_reader) = duplex(2 * 1024 * 1024);
    let mut frame_reader = FrameReader::new(server_reader);

    let payload_data = vec![0x42u8; 1024 * 1024]; // 1 MiB chunk payload
    let transfer_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();
    let xxhash = compute_xxhash64(&payload_data);

    let chunk_msg = Message::ChunkData(ChunkDataPayload {
        transfer_id,
        file_id,
        chunk_id: 42,
        file_offset: 1048576,
        payload_length: payload_data.len() as u32,
        checksum: xxhash,
        payload: payload_data.clone(),
    });

    let (header, maybe_payload) = encode_frame_parts(&chunk_msg).unwrap();
    let payload_slice = maybe_payload.unwrap_or(&[]);

    // Transmit via OS vectored I/O write_all_vectored
    let send_future = async {
        write_all_vectored(&mut client_writer, &header, payload_slice).await
    };

    let recv_future = async {
        frame_reader.read_frame().await
    };

    let (send_res, recv_res) = tokio::join!(send_future, recv_future);
    send_res.expect("write_all_vectored must succeed");
    let received_msg = recv_res.expect("read_frame must succeed").expect("Must receive a valid frame");

    match received_msg {
        Message::ChunkData(d) => {
            assert_eq!(d.transfer_id, transfer_id);
            assert_eq!(d.file_id, file_id);
            assert_eq!(d.chunk_id, 42);
            assert_eq!(d.file_offset, 1048576);
            assert_eq!(d.payload_length, payload_data.len() as u32);
            assert_eq!(d.checksum, xxhash);
            assert_eq!(d.payload, payload_data);
        }
        other => panic!("Expected Message::ChunkData, got {:?}", other),
    }
}

/// Test 4: Validate Storage Prefetching advisories on sequential file read (§6.3).
#[test]
fn test_storage_prefetching_advisories() {
    let temp_dir = tempdir().unwrap();
    let file_path = temp_dir.path().join("prefetch_test.bin");
    let test_data = vec![0x77u8; 64 * 1024];

    {
        let mut f = File::create(&file_path).unwrap();
        f.write_all(&test_data).unwrap();
    }

    let file = open_sequential_read(&file_path).expect("open_sequential_read should succeed");
    let meta = file.metadata().expect("Metadata should be accessible");
    advise_sequential_read(&file, meta.len());
    assert_eq!(meta.len(), 64 * 1024);
}

/// Test 5: Validate 4x Multi-Socket TCP Channel Bonding Transfer (§6.1).
#[tokio::test]
async fn test_4x_channel_bonding_multipath_transfer() {
    assert_eq!(
        DEFAULT_WIFI_PARALLEL_STREAMS, 4,
        "DEFAULT_WIFI_PARALLEL_STREAMS must default to 4 for 4x channel bonding"
    );

    let temp_dir = tempdir().unwrap();
    let src_path = temp_dir.path().join("multi_stream_src.bin");
    let dest_dir = temp_dir.path().join("receiver_out");

    let file_size = 4 * 1024 * 1024; // 4 MiB
    let content: Vec<u8> = (0..file_size).map(|i| (i % 251) as u8).collect();
    {
        let mut f = File::create(&src_path).unwrap();
        f.write_all(&content).unwrap();
    }

    let expected_crc = compute_file_crc32c(&src_path).unwrap();

    // Start receiver on dedicated test port
    let receiver_addr = "127.0.0.1:19889";
    let _receiver_handle = enter_receive_mode(Some(receiver_addr.to_string()), dest_dir.clone())
        .await
        .expect("Receiver must bind to 127.0.0.1:19889");

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Start sender with WifiDirectOnly (which uses DEFAULT_WIFI_PARALLEL_STREAMS = 4 streams)
    let transfer_handle = start_transfer(
        src_path.clone(),
        Some("multi_stream_src.bin".to_string()),
        None,
        TransportPreference::WifiDirectOnly,
        Some(receiver_addr.to_string()),
    )
    .await
    .expect("Sender must initiate 4x bonded stream transfer");

    let transfer_id = transfer_handle.transfer_id;

    // Poll until complete
    let mut completed = false;
    for _ in 0..100 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if let Some(status) = transfer_control_status(transfer_id) {
            if status == turbotransfer_core::manifest::TransferStatus::Completed {
                completed = true;
                break;
            }
        }
    }

    leave_receive_mode(Some(receiver_addr));
    assert!(completed, "4x channel bonded transfer should complete to 100%");

    // Verify Fix B1: 100% of throughput is accounted to Wi-Fi, 0 to USB
    let progress = turbotransfer_core::transfer::api::get_progress(transfer_id)
        .expect("Progress must exist");
    assert!(
        progress.wifi_throughput_bps > 0.0,
        "Transferred throughput must be tagged as Wi-Fi"
    );
    assert_eq!(
        progress.usb_throughput_bps, 0.0,
        "Zero throughput should be attributed to USB in WifiDirectOnly mode"
    );

    // Verify received file matches source
    let output_file = dest_dir.join("multi_stream_src.bin");
    assert!(output_file.exists(), "Output file must exist after transfer");

    let final_crc = compute_file_crc32c(&output_file).unwrap();
    assert_eq!(final_crc, expected_crc, "Output file CRC must match source exactly");
}

/// Test 6: Verify duplicate chunk handling maintains complete CRC table for O(1) GF(2) checksum (Fix B2).
#[tokio::test]
async fn test_duplicate_chunk_crc_table_retention() {
    let temp_dir = tempdir().unwrap();
    let dest_dir = temp_dir.path().join("dest_dup");
    std::fs::create_dir_all(&dest_dir).unwrap();

    let chunk_size = 128 * 1024;
    let file_size = chunk_size * 2;
    let data: Vec<u8> = (0..file_size).map(|i| (i % 251) as u8).collect();

    let part_path = dest_dir.join("dup_test.bin.part");
    {
        let f = File::create(&part_path).unwrap();
        f.set_len(file_size as u64).unwrap();
    }

    let mut tracker = turbotransfer_core::transfer::InMemoryChunkTracker::new();
    let transfer_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();

    let (client, server) = duplex(64 * 1024);
    let transport = turbotransfer_core::transport::StreamTransport::new(server, turbotransfer_core::transport::TransportKind::Tcp);

    let recv_task = tokio::spawn(async move {
        turbotransfer_core::transfer::session::receive_file_session(
            Uuid::new_v4(),
            "Recv",
            &dest_dir,
            &mut tracker,
            transport,
        )
        .await
    });

    let send_data = data.clone();
    let send_task = tokio::spawn(async move {
        let mut client_transport = turbotransfer_core::transport::StreamTransport::new(client, turbotransfer_core::transport::TransportKind::Tcp);
        // Hello
        client_transport.send_frame(&Message::Hello(turbotransfer_core::protocol::HelloData {
            device_id: Uuid::new_v4(),
            device_name: "Sender".into(),
            protocol_version: 1,
        })).await.unwrap();

        let _ = client_transport.receive_frame().await.unwrap(); // peer hello

        // Offer
        client_transport.send_frame(&Message::TransferOffer(turbotransfer_core::protocol::TransferOfferData {
            transfer_id,
            file_id,
            file_name: "dup_test.bin".into(),
            file_size: file_size as u64,
            chunk_size: chunk_size as u32,
            total_chunks: 2,
            checksum_algo: "xxhash64".into(),
        })).await.unwrap();

        let _ = client_transport.receive_frame().await.unwrap(); // accept

        // Send chunk 0
        let c0_payload = send_data[0..chunk_size].to_vec();
        let c0_hash = compute_xxhash64(&c0_payload);
        client_transport.send_frame(&Message::ChunkData(ChunkDataPayload {
            transfer_id,
            file_id,
            chunk_id: 0,
            file_offset: 0,
            payload_length: chunk_size as u32,
            checksum: c0_hash,
            payload: c0_payload.clone(),
        })).await.unwrap();
        let _ = client_transport.receive_frame().await.unwrap(); // ack 0

        // Send chunk 0 AGAIN (duplicate)
        client_transport.send_frame(&Message::ChunkData(ChunkDataPayload {
            transfer_id,
            file_id,
            chunk_id: 0,
            file_offset: 0,
            payload_length: chunk_size as u32,
            checksum: c0_hash,
            payload: c0_payload,
        })).await.unwrap();
        let _ = client_transport.receive_frame().await.unwrap(); // ack 0 dup

        // Send chunk 1
        let c1_payload = send_data[chunk_size..file_size].to_vec();
        let c1_hash = compute_xxhash64(&c1_payload);
        client_transport.send_frame(&Message::ChunkData(ChunkDataPayload {
            transfer_id,
            file_id,
            chunk_id: 1,
            file_offset: chunk_size as u64,
            payload_length: chunk_size as u32,
            checksum: c1_hash,
            payload: c1_payload,
        })).await.unwrap();
        let _ = client_transport.receive_frame().await.unwrap(); // ack 1

        // Send Complete
        let full_crc = compute_crc32c(&send_data);
        client_transport.send_frame(&Message::Complete(turbotransfer_core::protocol::CompleteData {
            transfer_id,
            file_checksum: full_crc,
        })).await.unwrap();
        let _ = client_transport.receive_frame().await.unwrap(); // final ack
    });

    let (r_res, s_res) = tokio::join!(recv_task, send_task);
    s_res.unwrap();
    let out_path = r_res.unwrap().expect("Receiver session should succeed without CRC mismatch");
    assert_eq!(compute_file_crc32c(&out_path).unwrap(), compute_crc32c(&data));
}

/// Test 7: Verify telemetry peak speed, channel throughput breakdown, and accurate bottleneck reporting.
#[test]
fn test_telemetry_peak_speed_and_bottleneck_accuracy() {
    let tid = Uuid::new_v4();
    let file_size = 100 * 1024 * 1024; // 100 MB
    let tel = turbotransfer_core::util::telemetry::TransferTelemetry::new(
        tid,
        "test_file.bin".to_string(),
        file_size,
        turbotransfer_core::manifest::TransferRole::Sender,
    );

    // Simulate sending 50 chunks of 2MB over USB and Wi-Fi
    for cid in 0..50 {
        let channel = if cid % 2 == 0 { "USB" } else { "WiFi-Stream-1" };
        tel.record_chunk_read(cid, 2 * 1024 * 1024, 2500, 1500);
        tel.record_chunk_sent(channel, cid, 2 * 1024 * 1024, 500);
        tel.record_chunk_ack(channel, cid, 15.0, 2 * 1024 * 1024);
    }

    tel.mark_completed();

    let report = tel.generate_bottleneck_report();
    assert!(report.avg_throughput_mbps > 0.0, "Average throughput must be positive");
    assert!(report.peak_throughput_mbps >= report.avg_throughput_mbps, "Peak throughput must be >= average throughput");
    assert_eq!(report.channels.len(), 2, "Must have 2 active channels");

    for ch in &report.channels {
        assert!(ch.bytes_transferred > 0);
        assert!(ch.throughput_mbps > 0.0, "Channel throughput must be > 0 MB/s");
        assert_eq!(ch.nack_count, 0);
        assert_eq!(ch.disconnect_count, 0);
    }

    // Zero packet loss & fast wire speed => BALANCED_WIRE_SPEED or NETWORK_BANDWIDTH_LIMIT (never NETWORK_LATENCY_JITTER)
    assert_ne!(
        report.primary_bottleneck,
        "NETWORK_LATENCY_JITTER",
        "Zero NACKs must never be diagnosed as NETWORK_LATENCY_JITTER"
    );

    let temp_dir = tempdir().unwrap();
    let (_, log_path) = tel.export_log_files(temp_dir.path()).expect("Log export must succeed");
    let log_content = std::fs::read_to_string(log_path).expect("Log file must exist");

    assert!(log_content.contains("Peak Speed"), "Log must contain Peak Speed");
    assert!(log_content.contains("MB/s"), "Log must contain MB/s throughput values");
    assert!(log_content.contains("Sender Disk Read"), "Sender log must contain Sender Disk Read");
    assert!(log_content.contains("N/A (Sender Role Session)"), "Sender log must mark Receiver Disk Write as N/A");
}


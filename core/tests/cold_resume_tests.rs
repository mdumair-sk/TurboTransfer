use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};
use tokio::net::{TcpListener, TcpStream};
use uuid::Uuid;

use turbotransfer_core::checksum::compute_file_crc32c;
use turbotransfer_core::manifest::{MetaActor, TransferMeta, TransferRole};
use turbotransfer_core::transfer::session::{receive_file_session, send_file_session};
use turbotransfer_core::transfer::tracker::InMemoryChunkTracker;
use turbotransfer_core::transport::TcpTransport;

#[tokio::test]
async fn test_cold_resume_process_restart_mid_transfer() {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        run_cold_resume_test().await;
    })
    .await
    .expect("test_cold_resume_process_restart_mid_transfer timed out after 30s");
}

async fn run_cold_resume_test() {
    let temp_dir = tempfile::tempdir().unwrap();
    let src_path = temp_dir.path().join("source_video.mp4");
    let dest_dir = temp_dir.path().join("dest");
    let meta_dir = temp_dir.path().join("meta");
    std::fs::create_dir_all(&dest_dir).unwrap();
    std::fs::create_dir_all(&meta_dir).unwrap();

    let file_size = 4 * 1024 * 1024; // 4 MB (4 chunks of 1 MB)
    let chunk_size = 1024 * 1024; // 1 MB

    // Generate deterministic source file
    let mut source_bytes = Vec::with_capacity(file_size);
    for i in 0..file_size {
        source_bytes.push(((i * 7 + 13) % 256) as u8);
    }
    std::fs::write(&src_path, &source_bytes).unwrap();
    let expected_crc = compute_file_crc32c(&src_path).unwrap();

    let transfer_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();
    let meta_file = meta_dir.join(format!("{}.meta.json", transfer_id));

    println!("============================================================");
    println!(" PHASE 1: Start transfer, complete chunks 0 & 1, then crash");
    println!("============================================================");

    // 1. Initial MetaActor session
    let initial_meta = TransferMeta::new(
        transfer_id,
        file_id,
        "source_video.mp4".to_string(),
        file_size as u64,
        chunk_size as u32,
        4,
        TransferRole::Receiver,
        Uuid::new_v4(),
    );

    let (actor_handle, _join) = MetaActor::spawn(meta_file.clone(), initial_meta, 100);

    // Receiver writes chunk 0 and chunk 1 to .part file
    let part_path = dest_dir.join("source_video.mp4.part");
    {
        let mut part_file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&part_path)
            .unwrap();
        part_file.set_len(file_size as u64).unwrap();

        // Write chunk 0
        part_file.seek(SeekFrom::Start(0)).unwrap();
        part_file.write_all(&source_bytes[0..chunk_size]).unwrap();

        // Write chunk 1
        part_file.seek(SeekFrom::Start(chunk_size as u64)).unwrap();
        part_file
            .write_all(&source_bytes[chunk_size..2 * chunk_size])
            .unwrap();

        part_file.flush().unwrap();
    }

    // Record chunk 0 & 1 completions in MetaActor
    actor_handle
        .send_chunk_completed(0, turbotransfer_core::manifest::TransportType::Usb, chunk_size as u64)
        .await;
    actor_handle
        .send_chunk_completed(1, turbotransfer_core::manifest::TransportType::Usb, chunk_size as u64)
        .await;

    // Pause / Flush meta.json synchronously
    actor_handle.pause().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    println!("============================================================");
    println!(" PHASE 2: Simulate complete process crash & state drop");
    println!("============================================================");
    drop(actor_handle);
    drop(_join);

    // Verify meta.json on disk contains completed_ranges: [[0, 1]]
    let meta_content = std::fs::read_to_string(&meta_file).unwrap();
    let saved_meta: TransferMeta = serde_json::from_str(&meta_content).unwrap();
    assert_eq!(saved_meta.completed_ranges, vec![(0, 1)]);
    println!("  -> Verified meta.json on disk has completed_ranges = {:?}", saved_meta.completed_ranges);

    println!("============================================================");
    println!(" PHASE 3: Restart fresh process & resume transfer");
    println!("============================================================");

    // Bind receiver loopback listener
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen_addr = listener.local_addr().unwrap();

    let dest_dir_clone = dest_dir.clone();
    let saved_ranges = saved_meta.completed_ranges.clone();

    // Spawn fresh receiver task with tracker initialized from persisted meta.json ranges
    let receiver_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let transport = TcpTransport::from_stream(stream);
        let mut tracker = InMemoryChunkTracker::from_ranges(transfer_id, file_id, &saved_ranges);

        receive_file_session(
            Uuid::new_v4(),
            "FreshReceiver",
            &dest_dir_clone,
            &mut tracker,
            transport,
        )
        .await
    });

    // Sender connects and sends remaining chunks
    let client_stream = TcpStream::connect(listen_addr).await.unwrap();
    let sender_transport = TcpTransport::from_stream(client_stream);

    let sender_task = tokio::spawn(async move {
        send_file_session(
            Uuid::new_v4(),
            "FreshSender",
            &src_path,
            chunk_size as u32,
            transfer_id,
            sender_transport,
            None,
            None,
        )
        .await
    });

    sender_task.await.unwrap().expect("Sender session failed");
    let output_path = receiver_task.await.unwrap().expect("Receiver session failed");

    println!("============================================================");
    println!(" PHASE 4: Validate byte-for-byte CRC32c match on resumed file");
    println!("============================================================");
    let final_bytes = std::fs::read(&output_path).unwrap();
    let final_crc = compute_file_crc32c(&output_path).unwrap();

    assert_eq!(source_bytes, final_bytes);
    assert_eq!(expected_crc, final_crc);
    println!("  -> SUCCESS: Resumed file is byte-for-byte identical! CRC32c: 0x{:08X}", final_crc);
}

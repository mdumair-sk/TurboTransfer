use std::io::Write;
use tempfile::tempdir;
use tokio::time::{sleep, Duration};
use turbotransfer_core::manifest::TransferStatus;
use turbotransfer_core::transfer::{
    enter_receive_mode, get_progress, leave_receive_mode, start_transfer, TransportPreference,
};

#[tokio::test]
async fn test_tcp_file_transfer_loopback() {
    let temp_dir = tempdir().unwrap();
    let src_dir = temp_dir.path().join("src");
    let dest_dir = temp_dir.path().join("dest");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&dest_dir).unwrap();

    let src_file_path = src_dir.join("test_direct_tcp.bin");
    let test_data: Vec<u8> = (0..(3 * 1024 * 1024)).map(|i| (i % 239) as u8).collect();

    {
        let mut file = std::fs::File::create(&src_file_path).unwrap();
        file.write_all(&test_data).unwrap();
        file.flush().unwrap();
    }

    let listen_addr = "127.0.0.1:9911".to_string();

    // 1. Start continuous receive mode on loopback TCP listener
    let _receive_task = enter_receive_mode(Some(listen_addr.clone()), dest_dir.clone())
        .await
        .expect("Enter receive mode failed");

    // Short pause to ensure listener socket is bound
    sleep(Duration::from_millis(50)).await;

    // 2. Start sender client connecting over loopback TCP
    let handle = start_transfer(
        src_file_path.clone(),
        None,
        TransportPreference::Automatic,
        Some(listen_addr.clone()),
    )
    .await
    .expect("Start transfer failed");

    // 3. Await transfer completion via progress monitor
    let mut completed = false;
    for _ in 0..100 {
        if let Some(p) = get_progress(handle.transfer_id) {
            if p.status == TransferStatus::Completed {
                completed = true;
                break;
            } else if p.status == TransferStatus::Failed {
                panic!("Transfer failed unexpectedly");
            }
        }
        sleep(Duration::from_millis(50)).await;
    }
    assert!(completed, "Transfer must reach Completed status");

    // 4. Verify received file matches source name and byte content
    let output_path = dest_dir.join("test_direct_tcp.bin");
    assert!(output_path.exists(), "Received output file must exist on disk");

    let reassembled_bytes = std::fs::read(&output_path).unwrap();
    assert_eq!(
        reassembled_bytes.len(),
        test_data.len(),
        "File length must match"
    );
    assert_eq!(reassembled_bytes, test_data, "Byte content must be identical");

    // Clean up receive mode
    leave_receive_mode(Some(&listen_addr));
}

#[tokio::test]
async fn test_tcp_sequential_multi_file_transfers_same_listener() {
    let temp_dir = tempdir().unwrap();
    let src_dir = temp_dir.path().join("src_seq");
    let dest_dir = temp_dir.path().join("dest_seq");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&dest_dir).unwrap();

    let file1_path = src_dir.join("file1.bin");
    let file2_path = src_dir.join("file2.bin");
    let data1 = vec![0x11u8; 1024 * 1024]; // 1MB
    let data2 = vec![0x22u8; 2 * 1024 * 1024]; // 2MB

    std::fs::write(&file1_path, &data1).unwrap();
    std::fs::write(&file2_path, &data2).unwrap();

    let listen_addr = "127.0.0.1:9912".to_string();

    // 1. Start continuous receive mode
    let _receive_task = enter_receive_mode(Some(listen_addr.clone()), dest_dir.clone())
        .await
        .expect("Enter receive mode failed");

    sleep(Duration::from_millis(50)).await;

    // 2. Transfer File 1
    let handle1 = start_transfer(
        file1_path.clone(),
        None,
        TransportPreference::Automatic,
        Some(listen_addr.clone()),
    )
    .await
    .expect("Start transfer 1 failed");

    let mut completed1 = false;
    for _ in 0..100 {
        if let Some(p) = get_progress(handle1.transfer_id) {
            if p.status == TransferStatus::Completed {
                completed1 = true;
                break;
            }
        }
        sleep(Duration::from_millis(50)).await;
    }
    assert!(completed1, "File 1 transfer must complete");

    // 3. Immediately Transfer File 2 without restarting the receiver
    let handle2 = start_transfer(
        file2_path.clone(),
        None,
        TransportPreference::Automatic,
        Some(listen_addr.clone()),
    )
    .await
    .expect("Start transfer 2 failed on existing listener");

    let mut completed2 = false;
    for _ in 0..100 {
        if let Some(p) = get_progress(handle2.transfer_id) {
            if p.status == TransferStatus::Completed {
                completed2 = true;
                break;
            }
        }
        sleep(Duration::from_millis(50)).await;
    }
    assert!(completed2, "File 2 transfer must complete seamlessly on the same listener");

    // 4. Verify both files exist and are intact on disk
    let out1 = dest_dir.join("file1.bin");
    let out2 = dest_dir.join("file2.bin");
    assert_eq!(std::fs::read(&out1).unwrap(), data1);
    assert_eq!(std::fs::read(&out2).unwrap(), data2);

    leave_receive_mode(Some(&listen_addr));
}

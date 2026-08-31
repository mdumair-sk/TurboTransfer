use std::collections::HashSet;
use tempfile::tempdir;
use tokio::time::{sleep, Duration};
use turbotransfer_core::manifest::{
    coalesce_ranges, expand_ranges, MetaActor, TransferMeta, TransferRole,
    TransferStatus, TransportType,
};

use uuid::Uuid;

#[test]
fn test_range_coalescing_and_expansion() {
    // 1. Adjacent chunks merge into one range
    let mut set1 = HashSet::new();
    set1.insert(0);
    set1.insert(1);
    set1.insert(2);
    let ranges1 = coalesce_ranges(&set1);
    assert_eq!(ranges1, vec![(0, 2)]);
    assert_eq!(expand_ranges(&ranges1), set1);

    // 2. Non-adjacent stay separate
    let mut set2 = HashSet::new();
    set2.insert(0);
    set2.insert(2);
    set2.insert(4);
    let ranges2 = coalesce_ranges(&set2);
    assert_eq!(ranges2, vec![(0, 0), (2, 2), (4, 4)]);
    assert_eq!(expand_ranges(&ranges2), set2);

    // 3. Out-of-order insertion produces sorted/merged result
    let mut set3 = HashSet::new();
    for &id in &[5, 1, 2, 0, 4] {
        set3.insert(id);
    }
    let ranges3 = coalesce_ranges(&set3);
    assert_eq!(ranges3, vec![(0, 2), (4, 5)]);
    assert_eq!(expand_ranges(&ranges3), set3);

    // 4. Single-chunk file
    let mut set4 = HashSet::new();
    set4.insert(0);
    let ranges4 = coalesce_ranges(&set4);
    assert_eq!(ranges4, vec![(0, 0)]);
    assert_eq!(expand_ranges(&ranges4), set4);
}

#[tokio::test]
async fn test_actor_batching_flush_count_threshold() {
    let temp_dir = tempdir().unwrap();
    let meta_path = temp_dir.path().join("meta.json");

    let initial_meta = TransferMeta::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "batch_test.bin".into(),
        1000,
        100,
        10,
        TransferRole::Sender,
        Uuid::new_v4(),
    );

    let (handle, join_handle) = MetaActor::spawn(meta_path.clone(), initial_meta, 32);

    // Initial spawn flushes 0 ranges
    let initial_disk_meta: TransferMeta =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    assert!(initial_disk_meta.completed_ranges.is_empty());

    // Send 10 events
    for i in 0..10 {
        handle
            .send_chunk_completed(i, TransportType::Usb, 100)
            .await;
    }

    // Closing handle flushes all pending dirty events
    drop(handle);
    join_handle.await.unwrap();

    let disk_meta: TransferMeta =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    assert_eq!(disk_meta.completed_ranges, vec![(0, 9)]);
}

#[tokio::test]
async fn test_actor_immediate_cancel_flush() {
    let temp_dir = tempdir().unwrap();
    let meta_path = temp_dir.path().join("meta_cancel.json");

    let initial_meta = TransferMeta::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "cancel_test.bin".into(),
        1000,
        100,
        10,
        TransferRole::Receiver,
        Uuid::new_v4(),
    );

    let (handle, join_handle) = MetaActor::spawn(meta_path.clone(), initial_meta, 32);

    // Send 3 events (less than 10)
    for i in 0..3 {
        handle
            .send_chunk_completed(i, TransportType::WifiDirect, 100)
            .await;
    }

    // Send Cancel mid-batch
    handle.cancel().await;

    join_handle.await.unwrap();

    // Assert immediate flush happened, completed_ranges saved, and status is Cancelled
    let disk_meta: TransferMeta =
        serde_json::from_str(&std::fs::read_to_string(&meta_path).unwrap()).unwrap();
    assert_eq!(disk_meta.status, TransferStatus::Cancelled);
    assert_eq!(disk_meta.completed_ranges, vec![(0, 2)]);
    assert_eq!(disk_meta.transport_stats.wifi_direct.bytes, 300);
}

#[tokio::test]
async fn test_actor_restart_simulation() {
    let temp_dir = tempdir().unwrap();
    let meta_path = temp_dir.path().join("meta_restart.json");

    let initial_meta = TransferMeta::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "restart_test.bin".into(),
        2000,
        100,
        20,
        TransferRole::Receiver,
        Uuid::new_v4(),
    );

    // Phase 1: Start actor, complete chunks 0..5, drop actor
    {
        let (handle, join_handle) = MetaActor::spawn(meta_path.clone(), initial_meta.clone(), 32);
        for i in 0..5 {
            handle
                .send_chunk_completed(i, TransportType::Usb, 100)
                .await;
        }
        handle.pause().await; // Synchronously flushes and exits
        join_handle.await.unwrap();
    }

    // Phase 2: Start fresh actor pointing to same meta_path
    let (fresh_handle, fresh_join) = MetaActor::spawn(meta_path.clone(), initial_meta, 32);

    let fresh_meta = fresh_handle.get_meta().await.unwrap();

    // Assert state matches what was flushed in Phase 1
    assert_eq!(fresh_meta.completed_ranges, vec![(0, 4)]);
    assert_eq!(fresh_meta.status, TransferStatus::Paused);
    assert_eq!(fresh_meta.transport_stats.usb.bytes, 500);

    fresh_handle.cancel().await;
    fresh_join.await.unwrap();
}

#[test]
fn test_pause_and_cancel_from_non_tokio_thread() {
    let temp_dir = tempdir().unwrap();
    let meta_path = temp_dir.path().join("thread_test.meta.json");
    let transfer_id = Uuid::new_v4();

    let initial_meta = TransferMeta::new(
        transfer_id,
        Uuid::new_v4(),
        "thread_test.bin".into(),
        1000,
        100,
        10,
        TransferRole::Sender,
        Uuid::new_v4(),
    );

    let (actor_handle, _join_handle) = MetaActor::spawn(meta_path, initial_meta, 32);

    turbotransfer_core::transfer::api::register_active_transfer(
        transfer_id,
        "thread_test.bin".into(),
        1000,
        TransferRole::Sender,
        10,
        "TestTransport".into(),
    );
    turbotransfer_core::transfer::api::set_transfer_actor_handle(transfer_id, actor_handle);

    // Call pause_transfer from a plain OS thread outside any Tokio context
    let pause_thread = std::thread::spawn(move || {
        turbotransfer_core::transfer::api::pause_transfer(transfer_id);
    });
    pause_thread.join().expect("pause_transfer must not panic on plain OS thread");

    let status = turbotransfer_core::transfer::api::transfer_control_status(transfer_id);
    assert_eq!(status, Some(TransferStatus::Paused));

    // Call cancel_transfer from another plain OS thread
    let cancel_thread = std::thread::spawn(move || {
        turbotransfer_core::transfer::api::cancel_transfer(transfer_id);
    });
    cancel_thread.join().expect("cancel_transfer must not panic on plain OS thread");

    let status_after_cancel = turbotransfer_core::transfer::api::transfer_control_status(transfer_id);
    assert_eq!(status_after_cancel, Some(TransferStatus::Cancelled));
}

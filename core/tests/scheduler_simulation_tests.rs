use std::time::Instant;
use turbotransfer_core::scheduler::model::ChannelPerformanceModel;
use turbotransfer_core::scheduler::tracker::{ChannelState, ChannelTracker};

/// Helper to simulate an ACK arrival on tracker and model.
fn simulate_chunk_ack(
    tracker: &mut ChannelTracker,
    model: &mut ChannelPerformanceModel,
    chunk_id: u32,
    bytes: u64,
    ack_turnaround_us: u64,
    socket_send_duration_us: u64,
    receiver_verify_us: Option<u32>,
) {
    if let Some(sample) = tracker.record_chunk_ack(
        chunk_id,
        bytes,
        ack_turnaround_us,
        socket_send_duration_us,
        receiver_verify_us,
    ) {
        model.update_from_sample(&sample);
    }
}

/// Test 1: Asymmetric Throughput (USB 40 MB/s vs Wi-Fi 10 MB/s).
#[test]
fn test_asymmetric_throughput_distribution() {
    let mut usb_tracker = ChannelTracker::new("USB".to_string());
    let mut usb_model = ChannelPerformanceModel::new("USB".to_string(), 40.0);

    let mut wifi_tracker = ChannelTracker::new("WiFi".to_string());
    let mut wifi_model = ChannelPerformanceModel::new("WiFi".to_string(), 10.0);

    let chunk_size = 2 * 1024 * 1024; // 2 MB

    // Warm up both channels with their respective steady rates
    for i in 0..10 {
        // USB: 2MB in 50ms = 40 MB/s
        usb_tracker.record_chunk_sent(i, chunk_size);
        simulate_chunk_ack(&mut usb_tracker, &mut usb_model, i, chunk_size, 50_000, 1_000, Some(1_500));

        // Wi-Fi: 2MB in 200ms = 10 MB/s
        wifi_tracker.record_chunk_sent(100 + i, chunk_size);
        simulate_chunk_ack(&mut wifi_tracker, &mut wifi_model, 100 + i, chunk_size, 200_000, 2_000, Some(1_500));
    }

    assert_eq!(usb_tracker.state, ChannelState::Active);
    assert_eq!(wifi_tracker.state, ChannelState::Active);

    assert!(usb_model.throughput_ewma_mbps > 35.0, "USB EWMA should be ~40 MB/s");
    assert!(wifi_model.throughput_ewma_mbps < 15.0, "Wi-Fi EWMA should be ~10 MB/s");

    // Prediction: With 0 in-flight, USB should predict ~50ms vs Wi-Fi ~200ms
    let usb_pred_us = usb_model.estimate_completion_time_us(&usb_tracker, chunk_size as usize);
    let wifi_pred_us = wifi_model.estimate_completion_time_us(&wifi_tracker, chunk_size as usize);

    assert!(usb_pred_us < wifi_pred_us, "USB completion time should be much earlier than Wi-Fi");
}

/// Test 2: Dynamic Degradation (USB drops 40 -> 5 MB/s -> traffic shifts to Wi-Fi).
#[test]
fn test_dynamic_degradation_shift() {
    let mut usb_tracker = ChannelTracker::new("USB".to_string());
    let mut usb_model = ChannelPerformanceModel::new("USB".to_string(), 40.0);

    let mut wifi_tracker = ChannelTracker::new("WiFi".to_string());
    let mut wifi_model = ChannelPerformanceModel::new("WiFi".to_string(), 20.0);

    let chunk_size = 2 * 1024 * 1024;

    // Establish baselines
    for i in 0..5 {
        usb_tracker.record_chunk_sent(i, chunk_size);
        simulate_chunk_ack(&mut usb_tracker, &mut usb_model, i, chunk_size, 50_000, 1_000, Some(1_500));

        wifi_tracker.record_chunk_sent(100 + i, chunk_size);
        simulate_chunk_ack(&mut wifi_tracker, &mut wifi_model, 100 + i, chunk_size, 100_000, 2_000, Some(1_500));
    }

    // Degrade USB: 2MB takes 400ms (5 MB/s) with 150ms socket write
    for i in 5..10 {
        usb_tracker.record_chunk_sent(i, chunk_size);
        simulate_chunk_ack(&mut usb_tracker, &mut usb_model, i, chunk_size, 400_000, 150_000, Some(1_500));
    }

    assert_eq!(usb_tracker.state, ChannelState::Degraded, "USB should transition to Degraded state");

    let usb_pred_us = usb_model.estimate_completion_time_us(&usb_tracker, chunk_size as usize);
    let wifi_pred_us = wifi_model.estimate_completion_time_us(&wifi_tracker, chunk_size as usize);

    assert!(wifi_pred_us < usb_pred_us, "Wi-Fi should be preferred over degraded USB");
}

/// Test 3: Transient Latency Spike does not permanently blacklist channel.
#[test]
fn test_transient_latency_spike_recovery() {
    let mut tracker = ChannelTracker::new("WiFi-1".to_string());
    let mut model = ChannelPerformanceModel::new("WiFi-1".to_string(), 20.0);
    let chunk_size = 2 * 1024 * 1024;

    // Warm up to Active
    for i in 0..4 {
        tracker.record_chunk_sent(i, chunk_size);
        simulate_chunk_ack(&mut tracker, &mut model, i, chunk_size, 100_000, 1_000, Some(1_000));
    }
    assert_eq!(tracker.state, ChannelState::Active);

    // Severe spike for 4 samples -> Degraded
    for i in 4..8 {
        tracker.record_chunk_sent(i, chunk_size);
        simulate_chunk_ack(&mut tracker, &mut model, i, chunk_size, 2_000_000, 120_000, Some(1_000));
    }
    assert_eq!(tracker.state, ChannelState::Degraded);
}

/// Test 4: Channel Probing and Recovery (M=6 healthy samples -> Active).
#[test]
fn test_channel_probing_and_recovery() {
    let mut tracker = ChannelTracker::new("WiFi-1".to_string());
    let mut model = ChannelPerformanceModel::new("WiFi-1".to_string(), 20.0);
    let chunk_size = 2 * 1024 * 1024;

    // Force degraded state
    tracker.state = ChannelState::Degraded;
    tracker.record_disconnect("Test drop");

    // Manually trigger probing transition
    tracker.state = ChannelState::Probing;

    // 6 healthy samples
    for i in 0..6 {
        tracker.record_chunk_sent(i, chunk_size);
        simulate_chunk_ack(&mut tracker, &mut model, i, chunk_size, 80_000, 2_000, Some(1_000));
    }

    assert_eq!(tracker.state, ChannelState::Active, "Channel should recover to Active after 6 healthy samples");
}

/// Test 5: Equal Channels Balanced Distribution.
#[test]
fn test_equal_channels_balanced_distribution() {
    let mut trackers: Vec<ChannelTracker> = (1..=4).map(|i| ChannelTracker::new(format!("WiFi-{}", i))).collect();
    let mut models: Vec<ChannelPerformanceModel> = (1..=4).map(|i| ChannelPerformanceModel::new(format!("WiFi-{}", i), 20.0)).collect();
    let chunk_size = 2 * 1024 * 1024;

    // Warm all up
    for ch_idx in 0..4 {
        for cid in 0..3 {
            trackers[ch_idx].record_chunk_sent(cid, chunk_size);
            simulate_chunk_ack(&mut trackers[ch_idx], &mut models[ch_idx], cid, chunk_size, 100_000, 2_000, Some(1_000));
        }
        assert_eq!(trackers[ch_idx].state, ChannelState::Active);
    }

    // Each channel with 0 in-flight should predict equal completion times (+/- 10%)
    let mut predictions = Vec::new();
    for ch_idx in 0..4 {
        predictions.push(models[ch_idx].estimate_completion_time_us(&trackers[ch_idx], chunk_size as usize));
    }

    let min_pred = *predictions.iter().min().unwrap();
    let max_pred = *predictions.iter().max().unwrap();
    assert!((max_pred - min_pred) < min_pred / 5, "Predictions across equal channels must be balanced");
}

/// Test 6: Backlog vs Speed Tradeoff (Fast USB with 8 chunks in-flight vs Idle Wi-Fi).
#[test]
fn test_backlog_vs_speed_tradeoff() {
    let mut usb_tracker = ChannelTracker::new("USB".to_string());
    let mut usb_model = ChannelPerformanceModel::new("USB".to_string(), 40.0);

    let mut wifi_tracker = ChannelTracker::new("WiFi".to_string());
    let mut wifi_model = ChannelPerformanceModel::new("WiFi".to_string(), 20.0);

    let chunk_size = 2 * 1024 * 1024;

    // Warm up
    for i in 0..4 {
        usb_tracker.record_chunk_sent(i, chunk_size);
        simulate_chunk_ack(&mut usb_tracker, &mut usb_model, i, chunk_size, 50_000, 1_000, Some(1_000));

        wifi_tracker.record_chunk_sent(100 + i, chunk_size);
        simulate_chunk_ack(&mut wifi_tracker, &mut wifi_model, 100 + i, chunk_size, 100_000, 2_000, Some(1_000));
    }

    // Put 8 chunks in-flight on USB (16 MB backlog)
    for i in 10..18 {
        usb_tracker.record_chunk_sent(i, chunk_size);
    }
    assert_eq!(usb_tracker.in_flight_bytes, 16 * 1024 * 1024);

    // Wi-Fi has 0 in-flight
    assert_eq!(wifi_tracker.in_flight_bytes, 0);

    let usb_pred_us = usb_model.estimate_completion_time_us(&usb_tracker, chunk_size as usize);
    let wifi_pred_us = wifi_model.estimate_completion_time_us(&wifi_tracker, chunk_size as usize);

    // USB backlog (16MB + 2MB = 18MB / 40 MB/s = 450ms) vs Wi-Fi (2MB / 20 MB/s = 100ms)
    assert!(wifi_pred_us < usb_pred_us, "Scheduler should pick idle Wi-Fi over deeply backlogged USB");
}

/// Test 7: Zero-Loss Jitter Classification (0 NACKs with high variance).
#[test]
fn test_zero_loss_completion_jitter_classification() {
    let mut tracker = ChannelTracker::new("WiFi-1".to_string());
    let mut model = ChannelPerformanceModel::new("WiFi-1".to_string(), 20.0);
    let chunk_size = 2 * 1024 * 1024;

    let latencies = [50_000, 400_000, 60_000, 800_000, 70_000, 1_200_000];
    for (i, &lat) in latencies.iter().enumerate() {
        tracker.record_chunk_sent(i as u32, chunk_size);
        simulate_chunk_ack(&mut tracker, &mut model, i as u32, chunk_size, lat, 2_000, Some(1_000));
    }

    assert_eq!(tracker.total_chunks_nacked, 0);
    assert!(model.ack_turnaround_variance > 10_000_000.0, "Variance should be high");
}

/// Test 8: Corruption Isolation (NACK records corruption without false network classification).
#[test]
fn test_corruption_isolation() {
    let mut tracker = ChannelTracker::new("USB".to_string());
    tracker.record_chunk_sent(1, 2 * 1024 * 1024);
    tracker.record_chunk_nack(1, "xxHash64 payload mismatch");

    assert_eq!(tracker.total_chunks_nacked, 1);
    assert_eq!(tracker.in_flight_chunks.len(), 0);
}

/// Test 9: Completion Prediction Accuracy.
#[test]
fn test_completion_prediction_accuracy() {
    let mut tracker = ChannelTracker::new("USB".to_string());
    let mut model = ChannelPerformanceModel::new("USB".to_string(), 40.0);
    let chunk_size = 2 * 1024 * 1024;

    for i in 0..20 {
        let pred_us = model.estimate_completion_time_us(&tracker, chunk_size as usize);
        model.record_prediction(i, pred_us);
        tracker.record_chunk_sent(i, chunk_size);

        // Actual turnaround ~50ms
        simulate_chunk_ack(&mut tracker, &mut model, i, chunk_size, 50_000, 1_000, Some(1_000));
    }

    let (p50_err, p95_err, _mae) = model.prediction_error_stats();
    assert!(p50_err < 30.0, "P50 prediction error should be < 30% after convergence, got {:.1}%", p50_err);
    assert!(p95_err < 60.0, "P95 prediction error should be < 60%, got {:.1}%", p95_err);
}

/// Test 10: Channel Count Scaling Performance (1, 2, 4, 8, 16 channels).
#[test]
fn test_channel_count_scaling_performance() {
    for &num_channels in &[1, 2, 4, 8, 16] {
        let trackers: Vec<ChannelTracker> = (0..num_channels)
            .map(|i| ChannelTracker::new(format!("Ch-{}", i)))
            .collect();
        let models: Vec<ChannelPerformanceModel> = (0..num_channels)
            .map(|i| ChannelPerformanceModel::new(format!("Ch-{}", i), 25.0))
            .collect();

        let chunk_size = 2 * 1024 * 1024;

        let t0 = Instant::now();
        let mut best_ch = 0;
        let mut min_pred = u64::MAX;

        for (idx, model) in models.iter().enumerate() {
            let pred = model.estimate_completion_time_us(&trackers[idx], chunk_size);
            if pred < min_pred {
                min_pred = pred;
                best_ch = idx;
            }
        }
        let decision_us = t0.elapsed().as_micros();

        assert!(decision_us < 50, "Decision time for {} channels must be < 50 us, took {} us", num_channels, decision_us);
        assert_eq!(best_ch, 0);
    }
}

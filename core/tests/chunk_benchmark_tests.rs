use turbotransfer_core::scheduler::model::ChannelPerformanceModel;
use turbotransfer_core::scheduler::tracker::ChannelTracker;

/// Benchmark scheduler throughput across configurations: USB-only, Wi-Fi 1..4 streams, and Bonded USB+Wi-Fi.
#[test]
fn test_multichannel_config_bench() {
    struct ChannelSim {
        _name: String,
        capacity_mbps: f64,
        tracker: ChannelTracker,
        model: ChannelPerformanceModel,
    }

    let configs: [(&str, Vec<(&str, f64)>); 5] = [
        ("USB Only", vec![("USB", 45.0)]),
        ("Wi-Fi 1-Stream", vec![("WiFi-1", 18.0)]),
        ("Wi-Fi 2-Stream Bonded", vec![("WiFi-1", 18.0), ("WiFi-2", 18.0)]),
        ("Wi-Fi 4-Stream Bonded", vec![("WiFi-1", 18.0), ("WiFi-2", 18.0), ("WiFi-3", 18.0), ("WiFi-4", 18.0)]),
        ("USB + 4x Wi-Fi Hybrid", vec![("USB", 45.0), ("WiFi-1", 18.0), ("WiFi-2", 18.0), ("WiFi-3", 18.0), ("WiFi-4", 18.0)]),
    ];

    let chunk_size = 2 * 1024 * 1024; // 2 MB
    let total_file_bytes = 100 * 1024 * 1024; // 100 MB
    let total_chunks = total_file_bytes / chunk_size;

    println!("\n=== Multi-Channel Configuration Benchmark (100 MB Transfer) ===");

    for (cfg_name, channels_def) in &configs {
        let mut channels: Vec<ChannelSim> = channels_def
            .iter()
            .map(|&(name, cap)| ChannelSim {
                _name: name.to_string(),
                capacity_mbps: cap,
                tracker: ChannelTracker::new(name.to_string()),
                model: ChannelPerformanceModel::new(name.to_string(), cap),
            })
            .collect();

        let mut sim_time_ms = 0.0;
        let mut completed_chunks = 0;

        while completed_chunks < total_chunks {
            // Find channel with lowest E[T]
            let mut best_idx = 0;
            let mut min_pred_us = u64::MAX;

            for (idx, ch) in channels.iter().enumerate() {
                let pred = ch.model.estimate_completion_time_us(&ch.tracker, chunk_size as usize);
                if pred < min_pred_us {
                    min_pred_us = pred;
                    best_idx = idx;
                }
            }

            let ch = &mut channels[best_idx];
            ch.tracker.record_chunk_sent(completed_chunks as u32, chunk_size);

            let service_sec = (chunk_size as f64 / (1024.0 * 1024.0)) / ch.capacity_mbps;
            let service_us = (service_sec * 1_000_000.0) as u64;

            if let Some(sample) = ch.tracker.record_chunk_ack(
                completed_chunks as u32,
                chunk_size,
                service_us,
                1_000,
                Some(1_200),
            ) {
                ch.model.update_from_tracker_and_sample(&ch.tracker, &sample);
            }

            sim_time_ms += service_sec * 1000.0 / (channels.len() as f64);
            completed_chunks += 1;
        }

        let aggregate_mbps = (total_file_bytes as f64 / (1024.0 * 1024.0)) / (sim_time_ms / 1000.0);
        println!(
            "Config: {:<22} | Channels: {} | Simulated Rate: {:>6.2} MB/s",
            cfg_name,
            channels.len(),
            aggregate_mbps
        );

        assert!(aggregate_mbps > 0.0);
    }
}

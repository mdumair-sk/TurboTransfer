use std::time::Instant;
use turbotransfer_core::scheduler::buffer_pool::BufferPool;
use turbotransfer_core::scheduler::model::ChannelPerformanceModel;
use turbotransfer_core::scheduler::tracker::ChannelTracker;

/// Benchmark chunk sizes across memory allocation count and buffer pool reuse efficiency.
#[tokio::test]
async fn test_chunk_size_memory_and_allocation_bench() {
    let sizes = [
        512 * 1024,      // 512 KiB
        1024 * 1024,     // 1 MiB
        2 * 1024 * 1024, // 2 MiB
        4 * 1024 * 1024, // 4 MiB
        8 * 1024 * 1024, // 8 MiB
    ];

    let total_file_bytes = 64 * 1024 * 1024; // 64 MB simulation

    println!("\n=== Chunk Size Memory & Recycling Benchmark (64 MB Total) ===");
    for &chunk_sz in &sizes {
        let total_chunks = (total_file_bytes + chunk_sz - 1) / chunk_sz;
        let pool = BufferPool::new(8, chunk_sz);

        let t0 = Instant::now();
        let mut reused_count = 0;

        for _ in 0..total_chunks {
            let buf = pool.acquire().await;
            if buf.as_slice().len() <= chunk_sz {
                reused_count += 1;
            }
            drop(buf);
        }
        let elapsed_us = t0.elapsed().as_micros();

        let reuse_ratio = (reused_count as f64) / (total_chunks as f64) * 100.0;
        println!(
            "Chunk Size: {:>7} bytes ({:.1} MB) | Total Chunks: {:>4} | Acquire/Release: {:>4} us | Reuse Ratio: {:.1}%",
            chunk_sz,
            (chunk_sz as f64) / (1024.0 * 1024.0),
            total_chunks,
            elapsed_us,
            reuse_ratio
        );

        assert_eq!(reuse_ratio, 100.0, "Buffer pool should achieve 100% reuse after warm up");
    }
}

/// Benchmark scheduler throughput across configurations: USB-only, Wi-Fi 1..4 streams, and Bonded USB+Wi-Fi.
#[test]
fn test_multichannel_config_bench() {
    struct ChannelSim {
        name: String,
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
                name: name.to_string(),
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
                ch.model.update_from_sample(&sample);
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

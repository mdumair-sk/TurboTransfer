use chrono::Utc;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Instant;
use uuid::Uuid;

use super::ephemeral_file::EphemeralFile;
use super::types::{BenchmarkResult, TransferConfigOverride, TransferPurpose};
use crate::transfer::api::{
    resolve_and_connect_transports_with_streams, TransportPreference,
};
use crate::transfer::session::{
    send_file_session_multipath_ext, SessionOptions, TransferSessionError,
};
use crate::util::telemetry::get_telemetry;

/// Runs a single benchmark transfer of `size_mb` to the target peer.
pub async fn run_benchmark_transfer(
    target_device_id: Option<Uuid>,
    address: Option<&str>,
    transport_pref: TransportPreference,
    size_mb: u32,
    config_override: Option<&TransferConfigOverride>,
    purpose: TransferPurpose,
    active_tx_slot: Option<Arc<Mutex<Option<Uuid>>>>,
) -> Result<BenchmarkResult, TransferSessionError> {
    let size_bytes = (size_mb as u64) * 1024 * 1024;
    let ephemeral = EphemeralFile::create(size_bytes, None)
        .map_err(TransferSessionError::Io)?;

    let target_id = target_device_id.unwrap_or_else(Uuid::new_v4);
    let transfer_id = Uuid::new_v4();
    if let Some(slot) = &active_tx_slot {
        *slot.lock() = Some(transfer_id);
    }
    let stream_override = config_override.and_then(|c| c.wifi_stream_count);
    let (transports, _transport_names) = resolve_and_connect_transports_with_streams(
        transport_pref,
        address,
        stream_override,
    )
    .await?;

    let is_high_speed = transport_pref == TransportPreference::UsbOnly
        || transport_pref == TransportPreference::Combined;
    let chunk_size = config_override
        .and_then(|c| c.chunk_size_bytes)
        .unwrap_or_else(|| crate::chunk::select_optimal_chunk_size(size_bytes, is_high_speed));
    let window_preset = config_override.and_then(|c| c.wifi_window_preset);
    let session_options = SessionOptions {
        purpose,
        wifi_window_preset: window_preset,
    };

    let plan = crate::chunk::calculate_chunk_plan(size_bytes, chunk_size);
    let total_chunks = plan.len().max(1) as u32;
    let transport_label = match transport_pref {
        TransportPreference::UsbOnly => "USB (Benchmark)".to_string(),
        TransportPreference::WifiDirectOnly => "Wi-Fi (Benchmark)".to_string(),
        TransportPreference::Combined => "Multipath (Benchmark)".to_string(),
        TransportPreference::Automatic => "Auto (Benchmark)".to_string(),
    };
    crate::transfer::api::register_active_transfer_with_path(
        transfer_id,
        "benchmark_payload.bin".to_string(),
        size_bytes,
        crate::manifest::TransferRole::Sender,
        total_chunks,
        transport_label,
        Some(ephemeral.path.clone()),
        0,
        0,
    );

    struct TransferCleanupGuard {
        transfer_id: Uuid,
        slot: Option<Arc<Mutex<Option<Uuid>>>>,
    }
    impl Drop for TransferCleanupGuard {
        fn drop(&mut self) {
            if let Some(slot) = &self.slot {
                *slot.lock() = None;
            }
            crate::transfer::api::remove_active_transfer(self.transfer_id);
        }
    }
    let _guard = TransferCleanupGuard {
        transfer_id,
        slot: active_tx_slot.clone(),
    };
    let t0 = Instant::now();
    let res = send_file_session_multipath_ext(
        Uuid::new_v4(),
        "TurboBenchmark",
        &ephemeral.path,
        chunk_size,
        transfer_id,
        transports,
        Some("benchmark_payload.bin"),
        session_options,
    )
    .await;

    let elapsed = t0.elapsed();
    let duration_ms = elapsed.as_millis().max(1) as u64;

    res?;

    // Extract telemetry from transfer session
    let (peak_mbps, usb_avg, wifi_avg) = if let Some(tel) = get_telemetry(transfer_id) {
        let peak = tel.get_peak_throughput_mbps();
        let channels = tel.get_channel_bytes_transferred();
        let mut usb_bytes = 0u64;
        let mut wifi_bytes = 0u64;
        for (name, bytes) in channels.iter() {
            if name.contains("USB") {
                usb_bytes += bytes;
            } else {
                wifi_bytes += bytes;
            }
        }
        let dur_s = elapsed.as_secs_f64().max(0.001);
        let usb_mbps = (usb_bytes as f64 / (1024.0 * 1024.0)) / dur_s;
        let wifi_mbps = (wifi_bytes as f64 / (1024.0 * 1024.0)) / dur_s;
        (peak, usb_mbps, wifi_mbps)
    } else {
        (0.0, 0.0, 0.0)
    };

    let dur_s = elapsed.as_secs_f64().max(0.001);
    let avg_speed_mbps = (size_bytes as f64 / (1024.0 * 1024.0)) / dur_s;
    let peak_speed_mbps = peak_mbps.max(avg_speed_mbps);

    Ok(BenchmarkResult {
        target_device_id: target_id,
        transport: Some(transport_pref),
        throughput_mbps: avg_speed_mbps,
        avg_speed_mbps,
        peak_speed_mbps,
        usb_avg_mbps: usb_avg,
        wifi_avg_mbps: wifi_avg,
        duration_ms,
        bytes_transferred: size_bytes,
        timestamp: Utc::now(),
    })
}

/// Executes a standard benchmark (250 MB ephemeral push) to the peer.
pub async fn run_benchmark(
    target_device_id: Option<Uuid>,
    address: Option<&str>,
    transport_pref: TransportPreference,
    size_mb: Option<u32>,
) -> Result<BenchmarkResult, TransferSessionError> {
    let size = size_mb.unwrap_or(250);
    run_benchmark_transfer(
        target_device_id,
        address,
        transport_pref,
        size,
        None,
        TransferPurpose::Benchmark,
        None,
    )
    .await
}

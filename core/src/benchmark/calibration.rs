use chrono::Utc;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Instant;
use uuid::Uuid;

use super::config_store::save_calibration;
use super::runner::run_benchmark_transfer;
use super::types::{
    CalibrationCandidateResult, CalibrationProgressUpdate, CalibrationResult,
    TransferConfigOverride, TransferPurpose, WindowPreset,
};
use crate::transfer::api::TransportPreference;
use crate::transfer::session::TransferSessionError;

pub trait CalibrationProgressCallback: Send + Sync {
    fn on_progress(&self, update: CalibrationProgressUpdate);
}

static ACTIVE_CALIBRATIONS: LazyLock<Mutex<HashMap<String, (Arc<AtomicBool>, Arc<Mutex<Option<Uuid>>>)>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn cancel_calibration(target_key: &str) {
    let normalized = crate::benchmark::config_store::get_pair_key(target_key);
    let map = ACTIVE_CALIBRATIONS.lock();
    if let Some((token, active_tx_slot)) = map.get(&normalized) {
        token.store(true, Ordering::SeqCst);
        if let Some(active_id) = *active_tx_slot.lock() {
            crate::transfer::api::cancel_transfer(active_id);
        }
    }
}

pub async fn run_calibration(
    target_device_id: Option<Uuid>,
    address: Option<&str>,
    callback: Option<Box<dyn CalibrationProgressCallback>>,
) -> Result<CalibrationResult, TransferSessionError> {
    run_calibration_with_size(target_device_id, address, callback, None).await
}

pub async fn run_calibration_with_size(
    target_device_id: Option<Uuid>,
    address: Option<&str>,
    callback: Option<Box<dyn CalibrationProgressCallback>>,
    step_size_mb: Option<u32>,
) -> Result<CalibrationResult, TransferSessionError> {
    let target_id = target_device_id.unwrap_or_else(Uuid::new_v4);
    let raw_key = match address.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(addr) => addr.to_string(),
        None => target_device_id.map(|id| id.to_string()).unwrap_or_default(),
    };
    let target_key = crate::benchmark::config_store::get_pair_key(&raw_key);

    let cancel_token = Arc::new(AtomicBool::new(false));
    let active_transfer_id = Arc::new(Mutex::new(None));
    {
        let mut map = ACTIVE_CALIBRATIONS.lock();
        map.insert(
            target_key.clone(),
            (cancel_token.clone(), active_transfer_id.clone()),
        );
    }

    let result = run_calibration_internal(
        target_id,
        address,
        &target_key,
        cancel_token.clone(),
        active_transfer_id.clone(),
        callback,
        step_size_mb,
    )
    .await;
    {
        let mut map = ACTIVE_CALIBRATIONS.lock();
        map.remove(&target_key);
    }

    result
}

async fn run_calibration_internal(
    target_id: Uuid,
    address: Option<&str>,
    target_key: &str,
    cancel_token: Arc<AtomicBool>,
    active_transfer_id: Arc<Mutex<Option<Uuid>>>,
    callback: Option<Box<dyn CalibrationProgressCallback>>,
    custom_size_mb: Option<u32>,
) -> Result<CalibrationResult, TransferSessionError> {
    let step_size_mb = custom_size_mb
        .or_else(|| {
            std::env::var("TURBOTRANSFER_CALIBRATION_STEP_MB")
                .ok()
                .and_then(|v| v.parse().ok())
        })
        .unwrap_or(250);
    let total_steps = 10u32;
    let mut current_step = 0u32;
    let mut all_candidates = Vec::new();
    let start_all = Instant::now();

    let check_cancel = || -> Result<(), TransferSessionError> {
        if cancel_token.load(Ordering::Relaxed) {
            Err(TransferSessionError::Cancelled)
        } else {
            Ok(())
        }
    };

    // Stage 1: Wi-Fi Stream Count {2, 3, 4}
    let mut best_streams = 3usize;
    let mut best_stream_speed = 0.0f64;

    for &streams in &[2usize, 3, 4] {
        check_cancel()?;
        current_step += 1;
        let config = TransferConfigOverride {
            wifi_stream_count: Some(streams),
            chunk_size_bytes: None,
            wifi_window_preset: Some(WindowPreset::Balanced),
        };

        if let Some(cb) = &callback {
            cb.on_progress(CalibrationProgressUpdate {
                current_step,
                total_steps,
                stage: "streams".to_string(),
                config_under_test: config.clone(),
                last_result_mbps: all_candidates.last().map(|c: &CalibrationCandidateResult| c.avg_speed_mbps),
            });
        }
        check_cancel()?;

        let res = run_benchmark_transfer(
            Some(target_id),
            address,
            TransportPreference::Combined,
            step_size_mb,
            Some(&config),
            TransferPurpose::Calibration,
            Some(active_transfer_id.clone()),
        )
        .await?;

        let candidate = CalibrationCandidateResult {
            config: config.clone(),
            avg_speed_mbps: res.avg_speed_mbps,
            duration_ms: res.duration_ms,
            sweep_stage: "streams".to_string(),
        };

        if res.avg_speed_mbps > best_stream_speed {
            best_stream_speed = res.avg_speed_mbps;
            best_streams = streams;
        }

        all_candidates.push(candidate);
    }

    // Stage 2: Chunk Size {1 MiB, 2 MiB, 4 MiB}
    let mut best_chunk = 2 * 1024 * 1024u32;
    let mut best_chunk_speed = 0.0f64;

    for &chunk_mb in &[1u32, 2, 4] {
        check_cancel()?;
        current_step += 1;
        let chunk_bytes = chunk_mb * 1024 * 1024;
        let config = TransferConfigOverride {
            wifi_stream_count: Some(best_streams),
            chunk_size_bytes: Some(chunk_bytes),
            wifi_window_preset: Some(WindowPreset::Balanced),
        };

        if let Some(cb) = &callback {
            cb.on_progress(CalibrationProgressUpdate {
                current_step,
                total_steps,
                stage: "chunk_size".to_string(),
                config_under_test: config.clone(),
                last_result_mbps: all_candidates.last().map(|c: &CalibrationCandidateResult| c.avg_speed_mbps),
            });
        }
        check_cancel()?;

        let res = run_benchmark_transfer(
            Some(target_id),
            address,
            TransportPreference::Combined,
            step_size_mb,
            Some(&config),
            TransferPurpose::Calibration,
            Some(active_transfer_id.clone()),
        )
        .await?;

        let candidate = CalibrationCandidateResult {
            config: config.clone(),
            avg_speed_mbps: res.avg_speed_mbps,
            duration_ms: res.duration_ms,
            sweep_stage: "chunk_size".to_string(),
        };

        if res.avg_speed_mbps > best_chunk_speed {
            best_chunk_speed = res.avg_speed_mbps;
            best_chunk = chunk_bytes;
        }

        all_candidates.push(candidate);
    }

    // Stage 3: Window Preset {Balanced, Aggressive, Max}
    let mut best_window = WindowPreset::Balanced;
    let mut best_window_speed = 0.0f64;

    for &window in &[
        WindowPreset::Balanced,
        WindowPreset::Aggressive,
        WindowPreset::Max,
    ] {
        check_cancel()?;
        current_step += 1;
        let config = TransferConfigOverride {
            wifi_stream_count: Some(best_streams),
            chunk_size_bytes: Some(best_chunk),
            wifi_window_preset: Some(window),
        };

        if let Some(cb) = &callback {
            cb.on_progress(CalibrationProgressUpdate {
                current_step,
                total_steps,
                stage: "window".to_string(),
                config_under_test: config.clone(),
                last_result_mbps: all_candidates.last().map(|c: &CalibrationCandidateResult| c.avg_speed_mbps),
            });
        }
        check_cancel()?;

        let res = run_benchmark_transfer(
            Some(target_id),
            address,
            TransportPreference::Combined,
            step_size_mb,
            Some(&config),
            TransferPurpose::Calibration,
            Some(active_transfer_id.clone()),
        )
        .await?;

        let candidate = CalibrationCandidateResult {
            config: config.clone(),
            avg_speed_mbps: res.avg_speed_mbps,
            duration_ms: res.duration_ms,
            sweep_stage: "window".to_string(),
        };

        if res.avg_speed_mbps > best_window_speed {
            best_window_speed = res.avg_speed_mbps;
            best_window = window;
        }

        all_candidates.push(candidate);
    }

    // Stage 4: Confirmation Run
    check_cancel()?;
    current_step += 1;
    let winning_config = TransferConfigOverride {
        wifi_stream_count: Some(best_streams),
        chunk_size_bytes: Some(best_chunk),
        wifi_window_preset: Some(best_window),
    };

    if let Some(cb) = &callback {
        cb.on_progress(CalibrationProgressUpdate {
            current_step,
            total_steps,
            stage: "confirmation".to_string(),
            config_under_test: winning_config.clone(),
            last_result_mbps: all_candidates.last().map(|c: &CalibrationCandidateResult| c.avg_speed_mbps),
        });
    }
    check_cancel()?;

    let confirmation_res = run_benchmark_transfer(
        Some(target_id),
        address,
        TransportPreference::Combined,
        step_size_mb,
        Some(&winning_config),
        TransferPurpose::Calibration,
        Some(active_transfer_id.clone()),
    )
    .await?;

    all_candidates.push(CalibrationCandidateResult {
        config: winning_config.clone(),
        avg_speed_mbps: confirmation_res.avg_speed_mbps,
        duration_ms: confirmation_res.duration_ms,
        sweep_stage: "confirmation".to_string(),
    });

    let best_speed_mbps = (best_window_speed + confirmation_res.avg_speed_mbps) / 2.0;

    // Persist winning calibration profile
    let _ = save_calibration(target_key, winning_config.clone(), best_speed_mbps);

    let total_duration_ms = start_all.elapsed().as_millis() as u64;

    Ok(CalibrationResult {
        target_device_id: target_id,
        best_config: winning_config,
        best_speed_mbps,
        all_candidates,
        total_duration_ms,
        timestamp: Utc::now(),
    })
}

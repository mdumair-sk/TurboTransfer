//! Channel performance modeling, sample-aware EWMAs, variance tracking, and completion time prediction.

use std::collections::HashMap;
use std::time::Instant;
use super::tracker::{AckSample, ChannelState, ChannelTracker};

/// Record for evaluating scheduler prediction accuracy.
#[derive(Debug, Clone, Copy)]
pub struct PredictionRecord {
    pub chunk_id: u32,
    pub predicted_completion_us: u64,
    pub actual_completion_us: u64,
    pub error_us: u64,
    pub error_pct: f64,
}

/// Statistical performance model for an individual transport channel.
pub struct ChannelPerformanceModel {
    pub channel_name: String,
    pub initial_capacity_mbps: f64,

    // EWMA and variance values
    pub throughput_ewma_mbps: f64,
    pub throughput_variance: f64,
    pub ack_turnaround_ewma_us: f64,
    pub ack_turnaround_variance: f64,
    pub socket_duration_ewma_us: f64,
    pub socket_duration_variance: f64,
    pub estimated_capacity_mbps: f64,

    // Smoothing constants (§10)
    alpha_throughput: f64,
    alpha_ack: f64,
    alpha_socket: f64,

    // Prediction tracking
    pending_predictions: HashMap<u32, (u64, Instant)>,
    prediction_history: Vec<PredictionRecord>,
}

impl ChannelPerformanceModel {
    pub fn new(channel_name: String, initial_capacity_mbps: f64) -> Self {
        Self {
            channel_name,
            initial_capacity_mbps,
            throughput_ewma_mbps: 0.0,
            throughput_variance: 0.0,
            ack_turnaround_ewma_us: 0.0,
            ack_turnaround_variance: 0.0,
            socket_duration_ewma_us: 0.0,
            socket_duration_variance: 0.0,
            estimated_capacity_mbps: initial_capacity_mbps,
            alpha_throughput: 0.20,
            alpha_ack: 0.15,
            alpha_socket: 0.15,
            pending_predictions: HashMap::new(),
            prediction_history: Vec::with_capacity(64),
        }
    }

    /// Ingests a new ACK sample from ChannelTracker and updates EWMAs and variances.
    pub fn update_from_sample(&mut self, sample: &AckSample) {
        let sample_sec = (sample.ack_turnaround_us as f64) / 1_000_000.0;
        let sample_mbps = if sample_sec > 0.00001 {
            ((sample.bytes as f64) / (1024.0 * 1024.0)) / sample_sec
        } else {
            0.0
        };

        // 1. Throughput EWMA & Variance
        if self.throughput_ewma_mbps == 0.0 {
            self.throughput_ewma_mbps = sample_mbps;
            self.throughput_variance = 0.0;
        } else {
            let diff = sample_mbps - self.throughput_ewma_mbps;
            self.throughput_ewma_mbps += self.alpha_throughput * diff;
            self.throughput_variance = (1.0 - self.alpha_throughput) * self.throughput_variance
                + self.alpha_throughput * diff * diff;
        }

        // 2. ACK Turnaround EWMA & Variance
        let ack_us = sample.ack_turnaround_us as f64;
        if self.ack_turnaround_ewma_us == 0.0 {
            self.ack_turnaround_ewma_us = ack_us;
            self.ack_turnaround_variance = 0.0;
        } else {
            let diff = ack_us - self.ack_turnaround_ewma_us;
            self.ack_turnaround_ewma_us += self.alpha_ack * diff;
            self.ack_turnaround_variance = (1.0 - self.alpha_ack) * self.ack_turnaround_variance
                + self.alpha_ack * diff * diff;
        }

        // 3. Socket Send Duration EWMA & Variance
        let sock_us = sample.socket_send_duration_us as f64;
        if self.socket_duration_ewma_us == 0.0 {
            self.socket_duration_ewma_us = sock_us;
            self.socket_duration_variance = 0.0;
        } else {
            let diff = sock_us - self.socket_duration_ewma_us;
            self.socket_duration_ewma_us += self.alpha_socket * diff;
            self.socket_duration_variance = (1.0 - self.alpha_socket) * self.socket_duration_variance
                + self.alpha_socket * diff * diff;
        }

        // 4. Update Capacity Estimate (peak observed rolling vs baseline)
        if sample_mbps > self.estimated_capacity_mbps {
            self.estimated_capacity_mbps = sample_mbps;
        } else {
            self.estimated_capacity_mbps = self.estimated_capacity_mbps * 0.98 + sample_mbps * 0.02;
        }

        // 5. Complete pending prediction if present
        if let Some((pred_us, _start_time)) = self.pending_predictions.remove(&sample.chunk_id) {
            let actual_us = sample.ack_turnaround_us;
            let error_us = if actual_us > pred_us { actual_us - pred_us } else { pred_us - actual_us };
            let error_pct = if pred_us > 0 { (error_us as f64) / (pred_us as f64) * 100.0 } else { 0.0 };

            if self.prediction_history.len() == 64 {
                self.prediction_history.remove(0);
            }
            self.prediction_history.push(PredictionRecord {
                chunk_id: sample.chunk_id,
                predicted_completion_us: pred_us,
                actual_completion_us: actual_us,
                error_us,
                error_pct,
            });
        }
    }

    /// Predicts completion time in microseconds for a new chunk of given size on this channel.
    pub fn estimate_completion_time_us(&self, tracker: &ChannelTracker, chunk_size: usize) -> u64 {
        let total_work_bytes = tracker.in_flight_bytes + (chunk_size as u64);
        let work_mb = (total_work_bytes as f64) / (1024.0 * 1024.0);

        let effective_mbps = match tracker.state {
            ChannelState::Unknown => self.initial_capacity_mbps.max(1.0),
            ChannelState::WarmingUp => (self.initial_capacity_mbps * 0.5).max(self.throughput_ewma_mbps).max(1.0),
            ChannelState::Active => self.throughput_ewma_mbps.max(1.0),
            ChannelState::Degraded => (self.throughput_ewma_mbps * 0.2).max(0.5),
            ChannelState::Probing => (self.throughput_ewma_mbps * 0.6).max(1.0),
        };

        let est_seconds = work_mb / effective_mbps;
        (est_seconds * 1_000_000.0) as u64
    }

    /// Registers a scheduling prediction before chunk send.
    pub fn record_prediction(&mut self, chunk_id: u32, predicted_us: u64) {
        self.pending_predictions.insert(chunk_id, (predicted_us, Instant::now()));
    }

    /// Returns (P50 error %, P95 error %, MAE in microseconds).
    pub fn prediction_error_stats(&self) -> (f64, f64, f64) {
        if self.prediction_history.is_empty() {
            return (0.0, 0.0, 0.0);
        }

        let mut errors_pct: Vec<f64> = self.prediction_history.iter().map(|r| r.error_pct).collect();
        errors_pct.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        let p50_idx = (errors_pct.len() as f64 * 0.50) as usize;
        let p95_idx = (errors_pct.len() as f64 * 0.95).min((errors_pct.len() - 1) as f64) as usize;

        let p50 = errors_pct[p50_idx];
        let p95 = errors_pct[p95_idx];

        let sum_err: u64 = self.prediction_history.iter().map(|r| r.error_us).sum();
        let mae = (sum_err as f64) / (self.prediction_history.len() as f64);

        (p50, p95, mae)
    }
}

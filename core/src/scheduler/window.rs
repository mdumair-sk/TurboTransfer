//! AIMD Concurrency & Congestion Window Controller (§10).

use std::time::{Duration, Instant};
use super::model::ChannelPerformanceModel;
use super::tracker::ChannelTracker;

/// Configuration and controller for channel concurrency window sizing.
#[derive(Debug, Clone)]
pub struct WindowController {
    pub min_window: usize,
    pub max_window: usize,
    pub current_window: usize,

    chunks_since_adjust: usize,
    last_adjust_time: Instant,
    throughput_at_last_adjust: f64,
    increase_cooldown_chunks: usize,
    decrease_cooldown: Duration,
}

impl WindowController {
    pub fn new(min_window: usize, max_window: usize, initial_window: usize) -> Self {
        Self {
            min_window: min_window.max(1),
            max_window: max_window.max(1),
            current_window: initial_window.clamp(min_window, max_window),
            chunks_since_adjust: 0,
            last_adjust_time: Instant::now(),
            throughput_at_last_adjust: 0.0,
            increase_cooldown_chunks: 5,
            decrease_cooldown: Duration::from_millis(500),
        }
    }

    /// Evaluates recent channel performance and updates the allowable in-flight window.
    pub fn evaluate_and_adjust(&mut self, tracker: &ChannelTracker, model: &ChannelPerformanceModel) -> usize {
        self.chunks_since_adjust += 1;

        let cur_throughput = model.throughput_ewma_mbps;
        let throughput_gain_pct = if self.throughput_at_last_adjust > 0.0 {
            ((cur_throughput - self.throughput_at_last_adjust) / self.throughput_at_last_adjust) * 100.0
        } else {
            0.0
        };

        // 1. Check Multiplicative Decrease on Corroborated Backpressure
        let is_sustained_backpressure = model.socket_duration_ewma_us > 100_000.0
            && tracker.socket_blocking_ratio() > 0.30
            && throughput_gain_pct <= 0.0;

        if is_sustained_backpressure && self.last_adjust_time.elapsed() >= self.decrease_cooldown {
            let new_win = (self.current_window / 2).max(self.min_window);
            if new_win != self.current_window {
                self.current_window = new_win;
                self.chunks_since_adjust = 0;
                self.last_adjust_time = Instant::now();
                self.throughput_at_last_adjust = cur_throughput;
                return self.current_window;
            }
        }

        // 2. Check Additive Increase on Healthy Throughput Gain
        let is_healthy_gain = self.chunks_since_adjust >= self.increase_cooldown_chunks
            && self.current_window < self.max_window
            && (throughput_gain_pct >= 3.0 || self.throughput_at_last_adjust == 0.0)
            && tracker.socket_blocking_ratio() < 0.10
            && model.socket_duration_ewma_us < 20_000.0;

        if is_healthy_gain {
            self.current_window += 1;
            self.chunks_since_adjust = 0;
            self.last_adjust_time = Instant::now();
            self.throughput_at_last_adjust = cur_throughput;
        }

        self.current_window
    }
}

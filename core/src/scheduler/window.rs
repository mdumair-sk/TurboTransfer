//! AIMD Concurrency & Congestion Window Controller (§10).

use std::time::{Duration, Instant};
use super::model::ChannelPerformanceModel;
use super::tracker::ChannelTracker;

/// Default window presets
pub const USB_MIN_WINDOW: usize = 8;
pub const USB_MAX_WINDOW: usize = 32;
pub const USB_INITIAL_WINDOW: usize = 16;

pub const WIFI_MIN_WINDOW: usize = 12;
pub const WIFI_MAX_WINDOW: usize = 32;
pub const WIFI_INITIAL_WINDOW: usize = 16;

/// Configuration and controller for channel concurrency window sizing.
#[derive(Debug, Clone)]
pub struct WindowController {
    pub min_window: usize,
    pub max_window: usize,
    pub current_window: usize,

    pub window_before_probe: usize,
    pub goodput_at_last_adjust: f64,
    pub rtt_at_last_adjust: f64,
    pub socket_backpressure_threshold_us: f64,
    pub rtt_congestion_threshold_us: f64,

    chunks_since_adjust: usize,
    last_adjust_time: Instant,
    increase_cooldown_chunks: usize,
    decrease_cooldown: Duration,
}

impl WindowController {
    pub fn new(min_window: usize, max_window: usize, initial_window: usize) -> Self {
        Self::with_thresholds(min_window, max_window, initial_window, 50_000.0, 200_000.0)
    }

    pub fn with_backpressure_threshold(
        min_window: usize,
        max_window: usize,
        initial_window: usize,
        socket_backpressure_threshold_us: f64,
    ) -> Self {
        Self::with_thresholds(
            min_window,
            max_window,
            initial_window,
            socket_backpressure_threshold_us,
            200_000.0,
        )
    }

    pub fn with_thresholds(
        min_window: usize,
        max_window: usize,
        initial_window: usize,
        socket_backpressure_threshold_us: f64,
        rtt_congestion_threshold_us: f64,
    ) -> Self {
        let min_win = min_window.max(1);
        let max_win = max_window.max(min_win);
        let init_win = initial_window.clamp(min_win, max_win);
        Self {
            min_window: min_win,
            max_window: max_win,
            current_window: init_win,
            window_before_probe: init_win,
            goodput_at_last_adjust: 0.0,
            rtt_at_last_adjust: 0.0,
            socket_backpressure_threshold_us,
            rtt_congestion_threshold_us,
            chunks_since_adjust: 0,
            last_adjust_time: Instant::now(),
            increase_cooldown_chunks: 2,
            decrease_cooldown: Duration::from_millis(500),
        }
    }

    /// Presets tailored for USB (high bandwidth, low latency).
    pub fn for_usb() -> Self {
        Self::with_thresholds(USB_MIN_WINDOW, USB_MAX_WINDOW, USB_INITIAL_WINDOW, 50_000.0, 200_000.0)
    }

    /// Presets tailored for Wi-Fi Direct / TCP streams (higher RTT, larger in-flight needed for pipeline fill).
    pub fn for_wifi() -> Self {
        Self::with_thresholds(WIFI_MIN_WINDOW, WIFI_MAX_WINDOW, WIFI_INITIAL_WINDOW, 400_000.0, 1_500_000.0)
    }

    /// Evaluates recent channel performance and updates the allowable in-flight window.
    pub fn evaluate_and_adjust(&mut self, tracker: &ChannelTracker, model: &ChannelPerformanceModel) -> usize {
        self.chunks_since_adjust += 1;

        let cur_goodput = if model.goodput_ewma_mbps > 0.0 {
            model.goodput_ewma_mbps
        } else {
            model.throughput_ewma_mbps
        };
        let cur_rtt = model.ack_turnaround_ewma_us;

        let goodput_gain_pct = if self.goodput_at_last_adjust > 0.0 {
            ((cur_goodput - self.goodput_at_last_adjust) / self.goodput_at_last_adjust) * 100.0
        } else {
            0.0
        };

        let rtt_gain_pct = if self.rtt_at_last_adjust > 0.0 {
            ((cur_rtt - self.rtt_at_last_adjust) / self.rtt_at_last_adjust) * 100.0
        } else {
            0.0
        };

        // 1. Multiplicative Decrease on Multi-Signal Corroborated Congestion
        let is_socket_backpressure = model.socket_duration_ewma_us > self.socket_backpressure_threshold_us
            && tracker.socket_blocking_ratio() > 0.25;
        let is_severe_rtt_inflation = cur_rtt > self.rtt_congestion_threshold_us && self.rtt_at_last_adjust > 0.0 && rtt_gain_pct > 80.0;
        let is_congestion = (is_socket_backpressure || is_severe_rtt_inflation) && goodput_gain_pct <= 0.0;
        let cooldown_passed = self.chunks_since_adjust >= 3
            && (self.last_adjust_time.elapsed() >= self.decrease_cooldown || self.chunks_since_adjust >= 4);

        if is_congestion && cooldown_passed {
            let new_win = (self.current_window * 3 / 4).max(self.min_window);
            if new_win != self.current_window {
                self.window_before_probe = self.current_window;
                self.current_window = new_win;
                self.chunks_since_adjust = 0;
                self.last_adjust_time = Instant::now();
                self.goodput_at_last_adjust = cur_goodput;
                self.rtt_at_last_adjust = cur_rtt;
                return self.current_window;
            }
        }

        // 2. Goodput-Gain Gating: If probing yielded real degradation or severe latency bloat, stall / retreat
        if self.current_window > self.window_before_probe
            && self.chunks_since_adjust >= self.increase_cooldown_chunks
            && goodput_gain_pct < -20.0
            && rtt_gain_pct > 35.0
        {
            self.current_window = self.window_before_probe;
            self.chunks_since_adjust = 0;
            self.last_adjust_time = Instant::now();
            self.goodput_at_last_adjust = cur_goodput;
            self.rtt_at_last_adjust = cur_rtt;
            return self.current_window;
        }

        // 3. Additive Increase on Healthy Goodput Gain / Stable Transmission
        let is_healthy = self.chunks_since_adjust >= self.increase_cooldown_chunks
            && self.current_window < self.max_window
            && (goodput_gain_pct >= -10.0 || self.goodput_at_last_adjust == 0.0)
            && tracker.socket_blocking_ratio() < 0.15
            && model.socket_duration_ewma_us < self.socket_backpressure_threshold_us * 0.90
            && rtt_gain_pct < 60.0;

        if is_healthy {
            self.window_before_probe = self.current_window;
            self.current_window += 1;
            self.chunks_since_adjust = 0;
            self.last_adjust_time = Instant::now();
            self.goodput_at_last_adjust = cur_goodput;
            self.rtt_at_last_adjust = cur_rtt;
        }

        self.current_window
    }
}

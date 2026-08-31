//! Ground-truth channel state tracking, in-flight accounting, and utilization.

use std::collections::HashSet;
use std::time::Instant;

/// Lifecycle state of a transport channel (§10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelState {
    /// Channel just registered; no performance data available.
    Unknown,
    /// Channel undergoing initial probing to establish baseline.
    WarmingUp,
    /// Channel actively transferring data with stable performance.
    Active,
    /// Channel has experienced performance collapse or severe latency spike.
    Degraded,
    /// Channel is being sent limited traffic to test if performance has recovered.
    Probing,
}

/// Raw sample recorded upon chunk acknowledgment.
#[derive(Debug, Clone, Copy)]
pub struct AckSample {
    pub chunk_id: u32,
    pub bytes: u64,
    pub ack_turnaround_us: u64,
    pub socket_send_duration_us: u64,
    pub receiver_verify_us: Option<u32>,
    pub ack_residual_us: u64,
    pub timestamp: Instant,
}

/// Ground-truth tracker for an individual transport channel.
pub struct ChannelTracker {
    pub name: String,
    pub state: ChannelState,
    pub session_start: Instant,
    pub in_flight_chunks: HashSet<u32>,
    pub in_flight_bytes: u64,
    pub max_in_flight_observed: usize,
    pub total_chunks_sent: u64,
    pub total_chunks_acked: u64,
    pub total_chunks_nacked: u64,
    pub total_bytes_sent: u64,
    pub total_bytes_acked: u64,
    pub disconnect_count: u32,

    // Concurrency and throughput accounting
    pub last_ack_time: Option<Instant>,
    pub ack_window_start: Option<Instant>,
    pub ack_window_bytes: u64,
    pub last_inter_ack_us: Option<u64>,

    // Utilization tracking (time with >= 1 chunk in-flight)
    busy_time_us: u64,
    last_busy_start: Option<Instant>,

    // Hysteresis counters
    consecutive_severe_samples: usize,
    consecutive_healthy_samples: usize,
    last_degraded_time: Option<Instant>,

    // Recent samples ring buffer (capacity 32)
    recent_samples: Vec<AckSample>,
}

impl ChannelTracker {
    pub fn new(name: String) -> Self {
        let now = Instant::now();
        Self {
            name,
            state: ChannelState::Unknown,
            session_start: now,
            in_flight_chunks: HashSet::new(),
            in_flight_bytes: 0,
            max_in_flight_observed: 0,
            total_chunks_sent: 0,
            total_chunks_acked: 0,
            total_chunks_nacked: 0,
            total_bytes_sent: 0,
            total_bytes_acked: 0,
            disconnect_count: 0,
            last_ack_time: None,
            ack_window_start: None,
            ack_window_bytes: 0,
            last_inter_ack_us: None,
            busy_time_us: 0,
            last_busy_start: None,
            consecutive_severe_samples: 0,
            consecutive_healthy_samples: 0,
            last_degraded_time: None,
            recent_samples: Vec::with_capacity(32),
        }
    }

    /// Records when a chunk is dispatched on this channel.
    pub fn record_chunk_sent(&mut self, chunk_id: u32, bytes: u64) {
        let now = Instant::now();
        if self.in_flight_chunks.is_empty() && self.last_busy_start.is_none() {
            self.last_busy_start = Some(now);
        }

        self.in_flight_chunks.insert(chunk_id);
        self.in_flight_bytes += bytes;
        self.total_chunks_sent += 1;
        self.total_bytes_sent += bytes;

        let cur_len = self.in_flight_chunks.len();
        if cur_len > self.max_in_flight_observed {
            self.max_in_flight_observed = cur_len;
        }

        if self.state == ChannelState::Unknown {
            self.state = ChannelState::WarmingUp;
        }
    }

    /// Records when an ACK is received for a chunk.
    pub fn record_chunk_ack(
        &mut self,
        chunk_id: u32,
        bytes: u64,
        ack_turnaround_us: u64,
        socket_send_duration_us: u64,
        receiver_verify_us: Option<u32>,
    ) -> Option<AckSample> {
        let was_in_flight = self.in_flight_chunks.remove(&chunk_id);
        if was_in_flight {
            self.in_flight_bytes = self.in_flight_bytes.saturating_sub(bytes);
        }

        let now = Instant::now();
        if self.in_flight_chunks.is_empty() {
            if let Some(start) = self.last_busy_start.take() {
                self.busy_time_us += now.duration_since(start).as_micros() as u64;
            }
        }

        if let Some(last_ack) = self.last_ack_time {
            let inter_us = now.duration_since(last_ack).as_micros() as u64;
            self.last_inter_ack_us = Some(inter_us);
        }
        self.last_ack_time = Some(now);

        if let Some(win_start) = self.ack_window_start {
            if now.duration_since(win_start).as_millis() >= 1000 {
                self.ack_window_start = Some(now);
                self.ack_window_bytes = bytes;
            } else {
                self.ack_window_bytes += bytes;
            }
        } else {
            self.ack_window_start = Some(now);
            self.ack_window_bytes = bytes;
        }

        self.total_chunks_acked += 1;
        self.total_bytes_acked += bytes;

        let verify_val = receiver_verify_us.unwrap_or(0) as u64;
        let ack_residual_us = ack_turnaround_us.saturating_sub(verify_val);

        let sample = AckSample {
            chunk_id,
            bytes,
            ack_turnaround_us,
            socket_send_duration_us,
            receiver_verify_us,
            ack_residual_us,
            timestamp: now,
        };

        if self.recent_samples.len() == 32 {
            self.recent_samples.remove(0);
        }
        self.recent_samples.push(sample);

        self.update_state_machine_on_sample(&sample);

        Some(sample)
    }

    /// Records when a NACK is received.
    pub fn record_chunk_nack(&mut self, chunk_id: u32, _reason: &str) {
        if self.in_flight_chunks.remove(&chunk_id) {
            // In-flight chunk removed
        }
        self.total_chunks_nacked += 1;
        self.consecutive_severe_samples += 1;
        if self.consecutive_severe_samples >= 4 && self.state == ChannelState::Active {
            self.state = ChannelState::Degraded;
            self.last_degraded_time = Some(Instant::now());
        }
    }

    /// Records a transport disconnect / drop.
    pub fn record_disconnect(&mut self, _reason: &str) {
        self.disconnect_count += 1;
        self.in_flight_chunks.clear();
        self.in_flight_bytes = 0;
        self.state = ChannelState::Degraded;
        self.last_degraded_time = Some(Instant::now());
    }

    /// Updates the channel state machine with hysteresis rules (§10).
    fn update_state_machine_on_sample(&mut self, sample: &AckSample) {
        let is_severe = sample.socket_send_duration_us > 100_000 || sample.ack_turnaround_us > 1_500_000;
        let is_healthy = sample.socket_send_duration_us < 10_000 && sample.ack_turnaround_us < 500_000;

        if is_severe {
            self.consecutive_severe_samples += 1;
            self.consecutive_healthy_samples = 0;
        } else if is_healthy {
            self.consecutive_healthy_samples += 1;
            self.consecutive_severe_samples = 0;
        } else {
            self.consecutive_severe_samples = 0;
            self.consecutive_healthy_samples = 0;
        }

        match self.state {
            ChannelState::Unknown => {
                self.state = ChannelState::WarmingUp;
            }
            ChannelState::WarmingUp => {
                if self.total_chunks_acked >= 3 {
                    self.state = ChannelState::Active;
                }
            }
            ChannelState::Active => {
                if self.consecutive_severe_samples >= 4 {
                    self.state = ChannelState::Degraded;
                    self.last_degraded_time = Some(Instant::now());
                }
            }
            ChannelState::Degraded => {
                // After 1s cooldown, transition to Probing
                if let Some(t_deg) = self.last_degraded_time {
                    if t_deg.elapsed().as_secs_f64() >= 1.0 {
                        self.state = ChannelState::Probing;
                        self.consecutive_healthy_samples = 0;
                    }
                }
            }
            ChannelState::Probing => {
                if self.consecutive_healthy_samples >= 6 {
                    self.state = ChannelState::Active;
                } else if self.consecutive_severe_samples >= 2 {
                    self.state = ChannelState::Degraded;
                    self.last_degraded_time = Some(Instant::now());
                }
            }
        }
    }

    /// Computes inflight utilization: percentage of session time with >= 1 chunk outstanding.
    pub fn inflight_utilization_pct(&self) -> f64 {
        let now = Instant::now();
        let total_busy = if let Some(start) = self.last_busy_start {
            self.busy_time_us + now.duration_since(start).as_micros() as u64
        } else {
            self.busy_time_us
        };

        let elapsed_us = now.duration_since(self.session_start).as_micros() as u64;
        if elapsed_us == 0 {
            0.0
        } else {
            ((total_busy as f64) / (elapsed_us as f64) * 100.0).clamp(0.0, 100.0)
        }
    }

    /// Returns the number of chunks currently in-flight on this channel.
    pub fn in_flight_count(&self) -> usize {
        self.in_flight_chunks.len()
    }

    /// Returns the total bytes currently in-flight on this channel.
    pub fn in_flight_bytes(&self) -> u64 {
        self.in_flight_bytes
    }

    /// Returns the inter-ACK arrival interval in microseconds if available.
    pub fn inter_ack_interval_us(&self) -> Option<u64> {
        self.last_inter_ack_us
    }

    /// Calculates rolling goodput in MB/s over a bounded interval.
    pub fn rolling_goodput_mbps(&self) -> f64 {
        if self.recent_samples.is_empty() {
            return 0.0;
        }

        // 1. Multi-sample elapsed duration in recent window (smoothed across packet bursts)
        let now = Instant::now();
        let valid_samples: Vec<&AckSample> = self
            .recent_samples
            .iter()
            .filter(|s| now.duration_since(s.timestamp).as_millis() <= 1000)
            .collect();

        if valid_samples.len() >= 2 {
            let oldest = valid_samples.first().unwrap().timestamp;
            let newest = valid_samples.last().unwrap().timestamp;
            let dt = newest.duration_since(oldest).as_secs_f64();
            if dt >= 0.005 {
                let sum_bytes: u64 = valid_samples.iter().map(|s| s.bytes).sum();
                return ((sum_bytes as f64) / (1024.0 * 1024.0)) / dt;
            }
        }

        // 2. If inter-ACK interval is available, calculate pipelined goodput with 500us safe floor
        if let Some(inter_us) = self.last_inter_ack_us {
            let dt = (inter_us as f64) / 1_000_000.0;
            if dt >= 0.0005 {
                let last_b = self.recent_samples.last().map(|s| s.bytes).unwrap_or(0);
                let raw_mbps = ((last_b as f64) / (1024.0 * 1024.0)) / dt;
                return raw_mbps.min(300.0);
            }
        }

        // 3. Fallback to latest sample turnaround
        let last_sample = self.recent_samples.last().unwrap();
        let sec = (last_sample.ack_turnaround_us as f64) / 1_000_000.0;
        if sec > 0.0005 {
            let raw_mbps = ((last_sample.bytes as f64) / (1024.0 * 1024.0)) / sec;
            raw_mbps.min(300.0)
        } else {
            0.0
        }
    }

    /// Returns fraction of recent socket sends taking >10ms (blocking ratio).
    pub fn socket_blocking_ratio(&self) -> f64 {
        if self.recent_samples.is_empty() {
            return 0.0;
        }
        let blocked = self.recent_samples.iter().filter(|s| s.socket_send_duration_us > 10_000).count();
        (blocked as f64) / (self.recent_samples.len() as f64)
    }

    pub fn recent_samples(&self) -> &[AckSample] {
        &self.recent_samples
    }
}

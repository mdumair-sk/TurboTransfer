use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::transport::TransportKind;

/// Sample of bytes transferred at a specific timestamp.
#[derive(Debug, Clone, Copy)]
struct ByteSample {
    timestamp: Instant,
    bytes: u64,
}

/// Rolling 2-second window throughput calculator (§10.1, §12).
pub struct RollingWindowTracker {
    window_duration: Duration,
    samples: VecDeque<ByteSample>,
    total_bytes_in_window: u64,
}

impl RollingWindowTracker {
    pub fn new(window_duration: Duration) -> Self {
        Self {
            window_duration,
            samples: VecDeque::new(),
            total_bytes_in_window: 0,
        }
    }

    /// Records transferred bytes at the current instant.
    pub fn record_bytes(&mut self, bytes: u64) {
        let now = Instant::now();
        self.samples.push_back(ByteSample {
            timestamp: now,
            bytes,
        });
        self.total_bytes_in_window += bytes;
        self.prune(now);
    }

    /// Prunes samples older than `window_duration`.
    fn prune(&mut self, now: Instant) {
        while let Some(front) = self.samples.front() {
            if now.duration_since(front.timestamp) > self.window_duration {
                self.total_bytes_in_window -= front.bytes;
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// Calculates current rolling throughput in Bytes per second.
    pub fn current_throughput_bps(&mut self) -> f64 {
        let now = Instant::now();
        self.prune(now);

        if self.samples.is_empty() {
            return 0.0;
        }

        let elapsed = if let Some(front) = self.samples.front() {
            now.duration_since(front.timestamp).as_secs_f64().max(0.001)
        } else {
            self.window_duration.as_secs_f64()
        };

        (self.total_bytes_in_window as f64) / elapsed
    }
}

/// Consolidated metrics and rolling throughput tracker for multipath transfers.
pub struct ThroughputTracker {
    usb_tracker: Mutex<RollingWindowTracker>,
    wifi_tracker: Mutex<RollingWindowTracker>,
    tcp_tracker: Mutex<RollingWindowTracker>,
    total_usb_bytes: Mutex<u64>,
    total_wifi_bytes: Mutex<u64>,
    total_retries: Mutex<u64>,
    usb_errors: Mutex<u64>,
    wifi_errors: Mutex<u64>,
}

impl Default for ThroughputTracker {
    fn default() -> Self {
        Self::new(Duration::from_secs(2))
    }
}

impl ThroughputTracker {
    pub fn new(window: Duration) -> Self {
        Self {
            usb_tracker: Mutex::new(RollingWindowTracker::new(window)),
            wifi_tracker: Mutex::new(RollingWindowTracker::new(window)),
            tcp_tracker: Mutex::new(RollingWindowTracker::new(window)),
            total_usb_bytes: Mutex::new(0),
            total_wifi_bytes: Mutex::new(0),
            total_retries: Mutex::new(0),
            usb_errors: Mutex::new(0),
            wifi_errors: Mutex::new(0),
        }
    }

    /// Records bytes completed on a specific transport.
    pub fn record_transport_bytes(&self, kind: TransportKind, bytes: u64) {
        match kind {
            TransportKind::Usb => {
                let mut tracker = self.usb_tracker.lock().unwrap();
                tracker.record_bytes(bytes);
                *self.total_usb_bytes.lock().unwrap() += bytes;
            }
            TransportKind::WifiDirect => {
                let mut tracker = self.wifi_tracker.lock().unwrap();
                tracker.record_bytes(bytes);
                *self.total_wifi_bytes.lock().unwrap() += bytes;
            }
            TransportKind::Tcp => {
                let mut tracker = self.tcp_tracker.lock().unwrap();
                tracker.record_bytes(bytes);
            }
        }
    }

    /// Increments retry count for a transport.
    pub fn record_retry(&self, kind: TransportKind) {
        *self.total_retries.lock().unwrap() += 1;
        match kind {
            TransportKind::Usb => *self.usb_errors.lock().unwrap() += 1,
            TransportKind::WifiDirect => *self.wifi_errors.lock().unwrap() += 1,
            _ => {}
        }
    }

    /// Returns snapshot of throughput metrics: (usb_bps, wifi_bps, aggregate_bps).
    pub fn throughput_snapshot(&self) -> (f64, f64, f64) {
        let usb_bps = self.usb_tracker.lock().unwrap().current_throughput_bps();
        let wifi_bps = self.wifi_tracker.lock().unwrap().current_throughput_bps();
        let tcp_bps = self.tcp_tracker.lock().unwrap().current_throughput_bps();
        let agg_bps = usb_bps + wifi_bps + tcp_bps;
        (usb_bps, wifi_bps, agg_bps)
    }

    /// Returns total stats: (usb_bytes, wifi_bytes, retries, usb_errors, wifi_errors).
    pub fn stats_snapshot(&self) -> (u64, u64, u64, u64, u64) {
        (
            *self.total_usb_bytes.lock().unwrap(),
            *self.total_wifi_bytes.lock().unwrap(),
            *self.total_retries.lock().unwrap(),
            *self.usb_errors.lock().unwrap(),
            *self.wifi_errors.lock().unwrap(),
        )
    }
}

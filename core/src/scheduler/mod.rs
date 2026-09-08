//! Dynamic rate-adaptive multipath scheduling across active transports (§10).

pub mod buffer_pool;
pub mod metrics;
pub mod model;
pub mod multipath;
pub mod tracker;
pub mod window;

pub use buffer_pool::BufferPool;
pub use metrics::{RollingWindowTracker, ThroughputTracker};
pub use model::{ChannelPerformanceModel, PredictionRecord};
pub use multipath::{MultipathScheduler, SchedulerConfig, SchedulerDecision};
pub use tracker::{AckSample, ChannelState, ChannelTracker};
pub use window::{
    WindowController, USB_INITIAL_WINDOW, USB_MAX_WINDOW, USB_MIN_WINDOW, WIFI_INITIAL_WINDOW,
    WIFI_MAX_WINDOW, WIFI_MIN_WINDOW,
};

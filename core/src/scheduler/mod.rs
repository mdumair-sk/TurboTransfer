//! Dynamic rate-adaptive multipath scheduling across active transports (§10).

pub mod buffer_pool;
pub mod metrics;
pub mod multipath;

pub use buffer_pool::BufferPool;
pub use metrics::{RollingWindowTracker, ThroughputTracker};
pub use multipath::{MultipathScheduler, SchedulerConfig};

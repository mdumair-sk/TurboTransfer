//! Dynamic rate-adaptive multipath scheduling across active transports (§10).

pub mod model;
pub mod tracker;
pub mod window;

pub use model::ChannelPerformanceModel;
pub use tracker::{AckSample, ChannelState, ChannelTracker};
pub use window::{
    WindowController, USB_INITIAL_WINDOW, USB_MAX_WINDOW, USB_MIN_WINDOW, WIFI_INITIAL_WINDOW,
    WIFI_MAX_WINDOW, WIFI_MIN_WINDOW,
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::protocol::{Message, ProtocolError};

pub mod stream;
pub mod tcp;
pub mod usb;
pub mod vectored;
pub mod wifi_direct;

pub use stream::StreamTransport;
pub use tcp::{TcpListenerTransport, TcpTransport};
pub use usb::{AdbDeviceInfo, UsbTransport, UsbTransportConfig};
pub use vectored::write_all_vectored;
pub use wifi_direct::{WifiDirectConfig, WifiDirectTransport};

/// Discriminator for active physical or virtual transport types (§2, §8, §9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TransportKind {
    Tcp,
    Usb,
    WifiDirect,
}

impl std::fmt::Display for TransportKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportKind::Tcp => write!(f, "TCP"),
            TransportKind::Usb => write!(f, "USB"),
            TransportKind::WifiDirect => write!(f, "Wi-Fi Direct"),
        }
    }
}

/// Operational connectivity status for a transport channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportStatus {
    Connected,
    Connecting,
    Disconnected,
    Failed,
}

impl std::fmt::Display for TransportStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportStatus::Connected => write!(f, "Connected"),
            TransportStatus::Connecting => write!(f, "Connecting"),
            TransportStatus::Disconnected => write!(f, "Disconnected"),
            TransportStatus::Failed => write!(f, "Failed"),
        }
    }
}

/// Errors surfaced by a transport layer, suitable for scheduler retry logic (§10.3).
#[derive(Error, Debug)]
pub enum TransportError {
    #[error("Transport disconnected: {0}")]
    Disconnected(String),

    #[error("Transport connection timeout: {0}")]
    Timeout(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    #[error("Transport error: {0}")]
    Other(String),
}

/// Shared abstraction implemented by all TurboTransfer transport backends (§8, §9).
///
/// Both Wi-Fi Direct and USB transports describe themselves as "a plain TCP socket once
/// the tunnel/group is up, same framing as §6.1." This trait captures that shared contract.
#[async_trait]
pub trait Transport: Send + Sync {
    /// Returns the physical or virtual category of this transport.
    fn kind(&self) -> TransportKind;

    /// Returns the current connectivity status.
    fn status(&self) -> TransportStatus;

    /// Helper indicating whether the transport is currently ready for frame transmission.
    fn is_connected(&self) -> bool {
        self.status() == TransportStatus::Connected
    }

    /// Total payload and framing bytes sent through this transport.
    fn bytes_sent(&self) -> u64;

    /// Total payload and framing bytes received through this transport.
    fn bytes_received(&self) -> u64;

    /// Transmits a protocol frame over the transport channel.
    async fn send_frame(&mut self, msg: &Message) -> Result<(), TransportError>;

    /// Receives the next protocol frame from the transport channel.
    /// Returns `Ok(None)` if the connection reached EOF cleanly.
    async fn receive_frame(&mut self) -> Result<Option<Message>, TransportError>;

    /// Gracefully closes or shuts down the transport channel.
    async fn close(&mut self) -> Result<(), TransportError>;
}

#[async_trait]
impl Transport for Box<dyn Transport> {
    fn kind(&self) -> TransportKind {
        (**self).kind()
    }

    fn status(&self) -> TransportStatus {
        (**self).status()
    }

    fn bytes_sent(&self) -> u64 {
        (**self).bytes_sent()
    }

    fn bytes_received(&self) -> u64 {
        (**self).bytes_received()
    }

    async fn send_frame(&mut self, msg: &Message) -> Result<(), TransportError> {
        (**self).send_frame(msg).await
    }

    async fn receive_frame(&mut self) -> Result<Option<Message>, TransportError> {
        (**self).receive_frame().await
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        (**self).close().await
    }
}

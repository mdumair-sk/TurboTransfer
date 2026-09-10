use async_trait::async_trait;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};

use super::stream::StreamTransport;
pub use super::stream::{StreamReadHalf as TcpReadHalf, StreamWriteHalf as TcpWriteHalf};
use super::{Transport, TransportError, TransportKind, TransportReadHalf, TransportStatus, TransportWriteHalf};
use crate::protocol::Message;

/// Concrete TCP implementation of the `Transport` trait using OS sockets (§8, §9).
pub struct TcpTransport {
    inner: StreamTransport<TcpStream>,
    local_addr: Option<SocketAddr>,
    peer_addr: Option<SocketAddr>,
}

/// Configures high-performance TCP socket parameters (TCP_NODELAY + high BDP buffer sizing)
/// to maximize Bandwidth-Delay Product (BDP) over 5 GHz Wi-Fi and high-speed USB links.
pub fn configure_tcp_stream(stream: &TcpStream) {
    let _ = stream.set_nodelay(true);
    #[cfg(windows)]
    {
        const WIN_BUFFER_SIZE: usize = 8 * 1024 * 1024;
        let sock = socket2::SockRef::from(stream);
        let _ = sock.set_recv_buffer_size(WIN_BUFFER_SIZE);
        let _ = sock.set_send_buffer_size(WIN_BUFFER_SIZE);
    }
    #[cfg(not(windows))]
    {
        // On Linux / Android, only raise buffer floor if default is below 512KB,
        // preserving the kernel's dynamic TCP window scaling (tcp_wmem / tcp_rmem)
        // which can dynamically scale up to 6.7MB / 8.8MB for maximum wire saturation.
        const LINUX_MIN_FLOOR: usize = 512 * 1024;
        const LINUX_BUFFER_SIZE: usize = 4 * 1024 * 1024;
        let sock = socket2::SockRef::from(stream);
        if let Ok(cur_rcv) = sock.recv_buffer_size() {
            if cur_rcv < LINUX_MIN_FLOOR {
                let _ = sock.set_recv_buffer_size(LINUX_BUFFER_SIZE);
            }
        }
        if let Ok(cur_snd) = sock.send_buffer_size() {
            if cur_snd < LINUX_MIN_FLOOR {
                let _ = sock.set_send_buffer_size(LINUX_BUFFER_SIZE);
            }
        }
    }
}

impl TcpTransport {
    /// Connects to a peer over a TCP socket at the specified address (e.g. "192.168.1.19:9876").
    pub async fn connect(addr: &str) -> Result<Self, TransportError> {
        let stream = TcpStream::connect(addr).await.map_err(|e| {
            TransportError::Disconnected(format!("Failed to connect to {}: {}", addr, e))
        })?;
        Ok(Self::from_stream(stream))
    }

    /// Wraps an established `TcpStream` into a `TcpTransport`.
    pub fn from_stream(stream: TcpStream) -> Self {
        configure_tcp_stream(&stream);
        let local_addr = stream.local_addr().ok();
        let peer_addr = stream.peer_addr().ok();
        let inner = StreamTransport::new(stream, TransportKind::Tcp);
        Self {
            inner,
            local_addr,
            peer_addr,
        }
    }

    /// Returns the local socket address if bound.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr
    }

    /// Returns the remote peer socket address if connected.
    pub fn peer_addr(&self) -> Option<SocketAddr> {
        self.peer_addr
    }
}

#[async_trait]
impl Transport for TcpTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Tcp
    }

    fn status(&self) -> TransportStatus {
        self.inner.status()
    }

    fn bytes_sent(&self) -> u64 {
        self.inner.bytes_sent()
    }

    fn bytes_received(&self) -> u64 {
        self.inner.bytes_received()
    }

    async fn send_frame(&mut self, msg: &Message) -> Result<(), TransportError> {
        self.inner.send_frame(msg).await
    }

    async fn receive_frame(&mut self) -> Result<Option<Message>, TransportError> {
        self.inner.receive_frame().await
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.inner.close().await
    }

    fn split_boxed(
        self: Box<Self>,
    ) -> Result<(Box<dyn TransportWriteHalf>, Box<dyn TransportReadHalf>), TransportError> {
        Box::new(self.inner).split_boxed()
    }
}

/// Helper listener for binding on OS interfaces and accepting `TcpTransport` connections (§8, §9).
pub struct TcpListenerTransport {
    listener: TcpListener,
}

impl TcpListenerTransport {
    /// Binds a TCP listener to the specified address (e.g. "0.0.0.0:9876" or "127.0.0.1:9876").
    pub async fn bind(addr: &str) -> Result<Self, TransportError> {
        let listener = TcpListener::bind(addr).await.map_err(|e| {
            TransportError::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to bind TCP listener to {}: {}", addr, e),
            ))
        })?;
        Ok(Self { listener })
    }

    /// Returns the local bound address.
    pub fn local_addr(&self) -> Result<SocketAddr, TransportError> {
        self.listener.local_addr().map_err(TransportError::Io)
    }

    /// Accepts an incoming connection and returns a new `TcpTransport`.
    pub async fn accept(&self) -> Result<(TcpTransport, SocketAddr), TransportError> {
        let (stream, peer_addr) = self.listener.accept().await.map_err(TransportError::Io)?;
        let transport = TcpTransport::from_stream(stream);
        Ok((transport, peer_addr))
    }
}

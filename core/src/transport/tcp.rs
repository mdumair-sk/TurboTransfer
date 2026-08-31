use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::{TcpListener, TcpStream};

use super::{Transport, TransportError, TransportKind, TransportStatus};
use crate::protocol::{encode_frame_parts, FrameReader, Message};

/// Concrete TCP implementation of the `Transport` trait using OS sockets (§8, §9).
pub struct TcpTransport {
    reader: FrameReader<ReadHalf<TcpStream>>,
    writer: WriteHalf<TcpStream>,
    local_addr: Option<SocketAddr>,
    peer_addr: Option<SocketAddr>,
    status: TransportStatus,
    bytes_sent: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
}

/// Configures high-performance TCP socket parameters (TCP_NODELAY + high BDP buffer sizing)
/// to maximize Bandwidth-Delay Product (BDP) over 5 GHz Wi-Fi and high-speed USB links.
pub fn configure_tcp_stream(stream: &TcpStream) {
    let _ = stream.set_nodelay(true);
    const BUFFER_SIZE: usize = 4 * 1024 * 1024;
    let sock = socket2::SockRef::from(stream);
    #[cfg(windows)]
    {
        let _ = sock.set_recv_buffer_size(BUFFER_SIZE);
        let _ = sock.set_send_buffer_size(BUFFER_SIZE);
    }
    #[cfg(not(windows))]
    {
        // On Linux / Android, only raise buffer floor if default is below 2MB,
        // preserving the kernel's dynamic TCP window scaling (tcp_wmem / tcp_rmem).
        if let Ok(cur_rcv) = sock.recv_buffer_size() {
            if cur_rcv < 2 * 1024 * 1024 {
                let _ = sock.set_recv_buffer_size(BUFFER_SIZE);
            }
        }
        if let Ok(cur_snd) = sock.send_buffer_size() {
            if cur_snd < 2 * 1024 * 1024 {
                let _ = sock.set_send_buffer_size(BUFFER_SIZE);
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
        let (read_half, write_half) = tokio::io::split(stream);

        Self {
            reader: FrameReader::new(read_half),
            writer: write_half,
            local_addr,
            peer_addr,
            status: TransportStatus::Connected,
            bytes_sent: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
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
        self.status
    }

    fn bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }

    fn bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }

    async fn send_frame(&mut self, msg: &Message) -> Result<(), TransportError> {
        if self.status != TransportStatus::Connected {
            return Err(TransportError::Disconnected(
                "Cannot send frame on disconnected TCP transport".into(),
            ));
        }

        let (header, maybe_payload) = encode_frame_parts(msg)?;
        let payload = maybe_payload.unwrap_or(&[]);
        let frame_len = (header.len() + payload.len()) as u64;

        if let Err(e) = super::vectored::write_all_vectored(&mut self.writer, &header, payload).await {
            self.status = TransportStatus::Disconnected;
            return Err(TransportError::Disconnected(format!(
                "Socket write error: {}",
                e
            )));
        }

        // Flush handshake and session termination control messages immediately,
        // but stream continuous ChunkData, ChunkAck, and BatchChunkAck without synchronous flush
        // since TCP_NODELAY already transmits frames immediately at the OS level.
        if !matches!(
            msg,
            Message::ChunkData(_) | Message::ChunkAck(_) | Message::BatchChunkAck(_)
        ) {
            if let Err(e) = self.writer.flush().await {
                self.status = TransportStatus::Disconnected;
                return Err(TransportError::Disconnected(format!(
                    "Socket flush error: {}",
                    e
                )));
            }
        }

        self.bytes_sent.fetch_add(frame_len, Ordering::Relaxed);
        Ok(())
    }

    async fn receive_frame(&mut self) -> Result<Option<Message>, TransportError> {
        if self.status != TransportStatus::Connected {
            return Err(TransportError::Disconnected(
                "Cannot receive frame on disconnected TCP transport".into(),
            ));
        }

        match self.reader.read_frame_with_length().await {
            Ok(Some((msg, frame_len))) => {
                self.bytes_received.fetch_add(frame_len as u64, Ordering::Relaxed);
                Ok(Some(msg))
            }
            Ok(None) => {
                self.status = TransportStatus::Disconnected;
                Ok(None)
            }
            Err(e) => {
                self.status = TransportStatus::Disconnected;
                Err(TransportError::from(e))
            }
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.status = TransportStatus::Disconnected;
        let _ = self.writer.shutdown().await;
        Ok(())
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

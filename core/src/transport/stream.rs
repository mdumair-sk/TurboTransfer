use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf};

use super::{Transport, TransportError, TransportKind, TransportReadHalf, TransportStatus, TransportWriteHalf};
use crate::protocol::{encode_frame_parts, FrameReader, Message};

/// Adapter that turns any bidirectional asynchronous byte stream into a `Transport` implementation.
pub struct StreamTransport<S> {
    reader: FrameReader<ReadHalf<S>>,
    writer: WriteHalf<S>,
    kind: TransportKind,
    status: TransportStatus,
    bytes_sent: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
}

impl<S> StreamTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
{
    /// Creates a new `StreamTransport` wrapping the given stream.
    pub fn new(stream: S, kind: TransportKind) -> Self {
        let (read_half, write_half) = tokio::io::split(stream);
        Self {
            reader: FrameReader::new(read_half),
            writer: write_half,
            kind,
            status: TransportStatus::Connected,
            bytes_sent: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
        }
    }
}

#[async_trait]
impl<S> Transport for StreamTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
{
    fn kind(&self) -> TransportKind {
        self.kind
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
                "Cannot send frame on disconnected stream transport".into(),
            ));
        }

        let (header, maybe_payload) = encode_frame_parts(msg)?;
        let payload = maybe_payload.unwrap_or(&[]);
        let frame_len = (header.len() + payload.len()) as u64;

        if let Err(e) = super::vectored::write_all_vectored(&mut self.writer, &header, payload).await {
            self.status = TransportStatus::Disconnected;
            return Err(TransportError::Disconnected(format!(
                "Stream write error: {}",
                e
            )));
        }

        if let Err(e) = self.writer.flush().await {
            self.status = TransportStatus::Disconnected;
            return Err(TransportError::Disconnected(format!(
                "Stream flush error: {}",
                e
            )));
        }

        self.bytes_sent.fetch_add(frame_len, Ordering::Relaxed);
        Ok(())
    }

    async fn receive_frame(&mut self) -> Result<Option<Message>, TransportError> {
        if self.status != TransportStatus::Connected {
            return Err(TransportError::Disconnected(
                "Cannot receive frame on disconnected stream transport".into(),
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

    fn split_boxed(
        self: Box<Self>,
    ) -> Result<(Box<dyn TransportWriteHalf>, Box<dyn TransportReadHalf>), TransportError> {
        let is_connected = Arc::new(std::sync::atomic::AtomicBool::new(
            self.status == TransportStatus::Connected,
        ));
        let write_half = StreamWriteHalf {
            writer: self.writer,
            bytes_sent: self.bytes_sent,
            is_connected: is_connected.clone(),
        };
        let read_half = StreamReadHalf {
            reader: self.reader,
            bytes_received: self.bytes_received,
            is_connected,
        };
        Ok((Box::new(write_half), Box::new(read_half)))
    }
}

/// Write half of a decoupled generic stream transport (§8, §9, §10).
pub struct StreamWriteHalf<S> {
    writer: WriteHalf<S>,
    bytes_sent: Arc<AtomicU64>,
    is_connected: Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait]
impl<S> TransportWriteHalf for StreamWriteHalf<S>
where
    S: AsyncWrite + Unpin + Send + Sync + 'static,
{
    async fn send_frame(&mut self, msg: &Message) -> Result<(), TransportError> {
        if !self.is_connected.load(Ordering::Relaxed) {
            return Err(TransportError::Disconnected(
                "Cannot send frame on disconnected stream write half".into(),
            ));
        }

        let (header, maybe_payload) = encode_frame_parts(msg)?;
        let payload = maybe_payload.unwrap_or(&[]);
        let frame_len = (header.len() + payload.len()) as u64;

        if let Err(e) = super::vectored::write_all_vectored(&mut self.writer, &header, payload).await {
            self.is_connected.store(false, Ordering::Relaxed);
            return Err(TransportError::Disconnected(format!(
                "Stream write error: {}",
                e
            )));
        }

        if !matches!(
            msg,
            Message::ChunkData(_) | Message::ChunkAck(_) | Message::BatchChunkAck(_)
        ) {
            if let Err(e) = self.writer.flush().await {
                self.is_connected.store(false, Ordering::Relaxed);
                return Err(TransportError::Disconnected(format!(
                    "Stream flush error: {}",
                    e
                )));
            }
        }

        self.bytes_sent.fetch_add(frame_len, Ordering::Relaxed);
        Ok(())
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.is_connected.store(false, Ordering::Relaxed);
        let _ = self.writer.shutdown().await;
        Ok(())
    }

    fn bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }
}

/// Read half of a decoupled generic stream transport (§8, §9, §10).
pub struct StreamReadHalf<S> {
    reader: FrameReader<ReadHalf<S>>,
    bytes_received: Arc<AtomicU64>,
    is_connected: Arc<std::sync::atomic::AtomicBool>,
}

#[async_trait]
impl<S> TransportReadHalf for StreamReadHalf<S>
where
    S: AsyncRead + Unpin + Send + Sync + 'static,
{
    async fn receive_frame(&mut self) -> Result<Option<Message>, TransportError> {
        if !self.is_connected.load(Ordering::Relaxed) {
            return Err(TransportError::Disconnected(
                "Cannot receive frame on disconnected stream read half".into(),
            ));
        }

        match self.reader.read_frame_with_length().await {
            Ok(Some((msg, frame_len))) => {
                self.bytes_received.fetch_add(frame_len as u64, Ordering::Relaxed);
                Ok(Some(msg))
            }
            Ok(None) => {
                self.is_connected.store(false, Ordering::Relaxed);
                Ok(None)
            }
            Err(e) => {
                self.is_connected.store(false, Ordering::Relaxed);
                Err(TransportError::from(e))
            }
        }
    }

    fn bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }
}

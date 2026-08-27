use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf};

use super::{Transport, TransportError, TransportKind, TransportStatus};
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
        let mut frame_len = header.len() as u64;

        if let Err(e) = self.writer.write_all(&header).await {
            self.status = TransportStatus::Disconnected;
            return Err(TransportError::Disconnected(format!(
                "Stream write error: {}",
                e
            )));
        }

        if let Some(payload) = maybe_payload {
            frame_len += payload.len() as u64;
            if let Err(e) = self.writer.write_all(payload).await {
                self.status = TransportStatus::Disconnected;
                return Err(TransportError::Disconnected(format!(
                    "Stream write error: {}",
                    e
                )));
            }
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

        match self.reader.read_frame().await {
            Ok(Some(msg)) => {
                self.bytes_received.fetch_add(64, Ordering::Relaxed);
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

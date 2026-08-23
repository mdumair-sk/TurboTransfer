use bytes::BytesMut;
use tokio::io::{AsyncRead, AsyncReadExt};

use super::error::ProtocolError;
use super::messages::Message;

/// Default maximum allowed frame length (256 MiB) to guard against excessive allocation.
pub const DEFAULT_MAX_FRAME_SIZE: usize = 256 * 1024 * 1024;

/// Encodes a `Message` into a binary frame:
/// `[4-byte LE message_length] [1-byte message_type] [N-byte bincode payload]`
/// where `message_length = 1 + payload.len()`.
pub fn encode_frame(msg: &Message) -> Result<Vec<u8>, ProtocolError> {
    let msg_type = msg.message_type();
    // Pre-allocate header + payload capacity to eliminate multiple re-allocations for large chunks
    let capacity = match msg {
        Message::ChunkData(d) => 4 + 1 + d.payload.len() + 128,
        _ => 256,
    };
    let mut out = Vec::with_capacity(capacity);
    out.extend_from_slice(&[0u8; 4]); // Placeholder for length prefix
    out.push(msg_type);

    match msg {
        Message::Hello(d) => {
            bincode::serialize_into(&mut out, d).map_err(|e| ProtocolError::SerializationError(e.to_string()))?;
        }
        Message::TransferOffer(d) => {
            bincode::serialize_into(&mut out, d).map_err(|e| ProtocolError::SerializationError(e.to_string()))?;
        }
        Message::TransferAccept(d) => {
            bincode::serialize_into(&mut out, d).map_err(|e| ProtocolError::SerializationError(e.to_string()))?;
        }
        Message::TransferReject(d) => {
            bincode::serialize_into(&mut out, d).map_err(|e| ProtocolError::SerializationError(e.to_string()))?;
        }
        Message::ChunkData(d) => {
            bincode::serialize_into(&mut out, d).map_err(|e| ProtocolError::SerializationError(e.to_string()))?;
        }
        Message::ChunkAck(d) => {
            bincode::serialize_into(&mut out, d).map_err(|e| ProtocolError::SerializationError(e.to_string()))?;
        }
        Message::ChunkNack(d) => {
            bincode::serialize_into(&mut out, d).map_err(|e| ProtocolError::SerializationError(e.to_string()))?;
        }
        Message::Pause(d) => {
            bincode::serialize_into(&mut out, d).map_err(|e| ProtocolError::SerializationError(e.to_string()))?;
        }
        Message::Resume(d) => {
            bincode::serialize_into(&mut out, d).map_err(|e| ProtocolError::SerializationError(e.to_string()))?;
        }
        Message::Cancel(d) => {
            bincode::serialize_into(&mut out, d).map_err(|e| ProtocolError::SerializationError(e.to_string()))?;
        }
        Message::Complete(d) => {
            bincode::serialize_into(&mut out, d).map_err(|e| ProtocolError::SerializationError(e.to_string()))?;
        }
        Message::Heartbeat(d) => {
            bincode::serialize_into(&mut out, d).map_err(|e| ProtocolError::SerializationError(e.to_string()))?;
        }
        Message::BatchChunkAck(d) => {
            bincode::serialize_into(&mut out, d).map_err(|e| ProtocolError::SerializationError(e.to_string()))?;
        }
    }

    let frame_len = (out.len() - 4) as u32;
    out[0..4].copy_from_slice(&frame_len.to_le_bytes());
    Ok(out)
}

/// Decodes a complete binary frame byte slice into a `Message`.
pub fn decode_frame(bytes: &[u8]) -> Result<Message, ProtocolError> {
    if bytes.len() < 5 {
        return Err(ProtocolError::TruncatedFrame {
            expected: 5,
            actual: bytes.len(),
        });
    }

    let length_prefix = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    if length_prefix == 0 {
        return Err(ProtocolError::DeserializationError(
            "Invalid frame length: 0".into(),
        ));
    }

    let expected_len = 4 + length_prefix;
    if bytes.len() != expected_len {
        return Err(ProtocolError::TruncatedFrame {
            expected: expected_len,
            actual: bytes.len(),
        });
    }

    let msg_type = bytes[4];
    let payload_bytes = &bytes[5..];
    Message::decode_payload(msg_type, payload_bytes)
}

/// Async reader for framing messages over an `AsyncRead` stream.
pub struct FrameReader<R> {
    reader: R,
    buffer: BytesMut,
    max_frame_size: usize,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    /// Creates a new `FrameReader` wrapping an `AsyncRead` stream with the default max frame size.
    pub fn new(reader: R) -> Self {
        Self::with_max_frame_size(reader, DEFAULT_MAX_FRAME_SIZE)
    }

    /// Creates a new `FrameReader` with a custom maximum allowed frame size.
    pub fn with_max_frame_size(reader: R, max_frame_size: usize) -> Self {
        Self {
            reader,
            buffer: BytesMut::with_capacity(512 * 1024),
            max_frame_size,
        }
    }

    /// Asynchronously reads the next full `Message` frame from the underlying stream.
    /// Handles arbitrary byte fragmentation across reads.
    /// Returns `Ok(None)` on clean EOF, or `Err` on truncation or invalid framing.
    pub async fn read_frame(&mut self) -> Result<Option<Message>, ProtocolError> {
        loop {
            if self.buffer.len() >= 4 {
                let length_prefix =
                    u32::from_le_bytes(self.buffer[0..4].try_into().unwrap()) as usize;
                if length_prefix == 0 {
                    return Err(ProtocolError::DeserializationError(
                        "Invalid frame length: 0".into(),
                    ));
                }
                if length_prefix > self.max_frame_size {
                    return Err(ProtocolError::FrameTooLarge(
                        length_prefix,
                        self.max_frame_size,
                    ));
                }
                let total_frame_len = 4 + length_prefix;
                if self.buffer.len() >= total_frame_len {
                    let frame_bytes = self.buffer.split_to(total_frame_len);
                    let msg = decode_frame(&frame_bytes)?;
                    return Ok(Some(msg));
                }
            }

            // Read directly into BytesMut buffer without any stack buffer allocation or intermediate copy
            if self.buffer.capacity() - self.buffer.len() < 256 * 1024 {
                self.buffer.reserve(512 * 1024);
            }
            let bytes_read = self.reader.read_buf(&mut self.buffer).await?;
            if bytes_read == 0 {
                if self.buffer.is_empty() {
                    return Ok(None);
                } else {
                    return Err(ProtocolError::UnexpectedEof);
                }
            }
        }
    }
}

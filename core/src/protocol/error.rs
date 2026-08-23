use thiserror::Error;

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("Unknown or invalid message type code: 0x{0:02X}")]
    InvalidMessageType(u8),

    #[error("Failed to serialize message payload: {0}")]
    SerializationError(String),

    #[error("Failed to deserialize message payload: {0}")]
    DeserializationError(String),

    #[error("Frame length {0} exceeds maximum allowed size of {1} bytes")]
    FrameTooLarge(usize, usize),

    #[error("Frame buffer incomplete or truncated: expected {expected} bytes, got {actual}")]
    TruncatedFrame { expected: usize, actual: usize },

    #[error("Unexpected end of stream while reading frame")]
    UnexpectedEof,

    #[error("I/O error during frame processing: {0}")]
    IoError(#[from] std::io::Error),
}

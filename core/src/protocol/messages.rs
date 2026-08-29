use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::ProtocolError;

/// Message type code for `Hello` (0x01)
pub const MSG_TYPE_HELLO: u8 = 0x01;
/// Message type code for `TransferOffer` (0x02)
pub const MSG_TYPE_TRANSFER_OFFER: u8 = 0x02;
/// Message type code for `TransferAccept` (0x03)
pub const MSG_TYPE_TRANSFER_ACCEPT: u8 = 0x03;
/// Message type code for `TransferReject` (0x04)
pub const MSG_TYPE_TRANSFER_REJECT: u8 = 0x04;
/// Message type code for `ChunkData` (0x05)
pub const MSG_TYPE_CHUNK_DATA: u8 = 0x05;
/// Message type code for `ChunkAck` (0x06)
pub const MSG_TYPE_CHUNK_ACK: u8 = 0x06;
/// Message type code for `ChunkNack` (0x07)
pub const MSG_TYPE_CHUNK_NACK: u8 = 0x07;
/// Message type code for `Pause` (0x08)
pub const MSG_TYPE_PAUSE: u8 = 0x08;
/// Message type code for `Resume` (0x09)
pub const MSG_TYPE_RESUME: u8 = 0x09;
/// Message type code for `Cancel` (0x0A)
pub const MSG_TYPE_CANCEL: u8 = 0x0A;
/// Message type code for `Complete` (0x0B)
pub const MSG_TYPE_COMPLETE: u8 = 0x0B;
/// Message type code for `Heartbeat` (0x0C)
pub const MSG_TYPE_HEARTBEAT: u8 = 0x0C;
/// Message type code for `BatchChunkAck` (0x0D)
pub const MSG_TYPE_BATCH_CHUNK_ACK: u8 = 0x0D;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HelloData {
    pub device_id: Uuid,
    pub device_name: String,
    pub protocol_version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferOfferData {
    pub transfer_id: Uuid,
    pub file_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub chunk_size: u32,
    pub total_chunks: u32,
    pub checksum_algo: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferAcceptData {
    pub transfer_id: Uuid,
    /// Completed range list stored as inclusive [start, end] chunk-id pairs.
    pub resume_from: Option<Vec<(u32, u32)>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferRejectData {
    pub transfer_id: Uuid,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkDataPayload {
    pub transfer_id: Uuid,
    pub file_id: Uuid,
    pub chunk_id: u32,
    pub file_offset: u64,
    pub payload_length: u32,
    pub checksum: u64,
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkAckData {
    pub transfer_id: Uuid,
    pub chunk_id: u32,
    #[serde(default)]
    pub receiver_verify_us: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkNackData {
    pub transfer_id: Uuid,
    pub chunk_id: u32,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PauseData {
    pub transfer_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeData {
    pub transfer_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelData {
    pub transfer_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompleteData {
    pub transfer_id: Uuid,
    pub file_checksum: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatData {
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchChunkAckData {
    pub transfer_id: Uuid,
    pub chunk_ids: Vec<u32>,
    #[serde(default)]
    pub sum_receiver_verify_us: Option<u32>,
}

/// The 13 wire-protocol message types for TurboTransfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    Hello(HelloData),
    TransferOffer(TransferOfferData),
    TransferAccept(TransferAcceptData),
    TransferReject(TransferRejectData),
    ChunkData(ChunkDataPayload),
    ChunkAck(ChunkAckData),
    ChunkNack(ChunkNackData),
    Pause(PauseData),
    Resume(ResumeData),
    Cancel(CancelData),
    Complete(CompleteData),
    Heartbeat(HeartbeatData),
    BatchChunkAck(BatchChunkAckData),
}

impl Message {
    /// Returns the 1-byte message type code for this message.
    pub fn message_type(&self) -> u8 {
        match self {
            Message::Hello(_) => MSG_TYPE_HELLO,
            Message::TransferOffer(_) => MSG_TYPE_TRANSFER_OFFER,
            Message::TransferAccept(_) => MSG_TYPE_TRANSFER_ACCEPT,
            Message::TransferReject(_) => MSG_TYPE_TRANSFER_REJECT,
            Message::ChunkData(_) => MSG_TYPE_CHUNK_DATA,
            Message::ChunkAck(_) => MSG_TYPE_CHUNK_ACK,
            Message::ChunkNack(_) => MSG_TYPE_CHUNK_NACK,
            Message::Pause(_) => MSG_TYPE_PAUSE,
            Message::Resume(_) => MSG_TYPE_RESUME,
            Message::Cancel(_) => MSG_TYPE_CANCEL,
            Message::Complete(_) => MSG_TYPE_COMPLETE,
            Message::Heartbeat(_) => MSG_TYPE_HEARTBEAT,
            Message::BatchChunkAck(_) => MSG_TYPE_BATCH_CHUNK_ACK,
        }
    }

    /// Deserializes a message given its type code and bincode payload.
    pub fn decode_payload(type_code: u8, payload: &[u8]) -> Result<Self, ProtocolError> {
        let msg = match type_code {
            MSG_TYPE_HELLO => {
                let data: HelloData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::Hello(data)
            }
            MSG_TYPE_TRANSFER_OFFER => {
                let data: TransferOfferData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::TransferOffer(data)
            }
            MSG_TYPE_TRANSFER_ACCEPT => {
                let data: TransferAcceptData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::TransferAccept(data)
            }
            MSG_TYPE_TRANSFER_REJECT => {
                let data: TransferRejectData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::TransferReject(data)
            }
            MSG_TYPE_CHUNK_DATA => {
                let data: ChunkDataPayload = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::ChunkData(data)
            }
            MSG_TYPE_CHUNK_ACK => {
                let data: ChunkAckData = if payload.len() == 20 {
                    // Legacy 20-byte payload: transfer_id (16) + chunk_id (4)
                    let t_id = bincode::deserialize(&payload[0..16])
                        .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                    let c_id = bincode::deserialize(&payload[16..20])
                        .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                    ChunkAckData {
                        transfer_id: t_id,
                        chunk_id: c_id,
                        receiver_verify_us: None,
                    }
                } else {
                    bincode::deserialize(payload)
                        .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?
                };
                Message::ChunkAck(data)
            }
            MSG_TYPE_CHUNK_NACK => {
                let data: ChunkNackData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::ChunkNack(data)
            }
            MSG_TYPE_PAUSE => {
                let data: PauseData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::Pause(data)
            }
            MSG_TYPE_RESUME => {
                let data: ResumeData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::Resume(data)
            }
            MSG_TYPE_CANCEL => {
                let data: CancelData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::Cancel(data)
            }
            MSG_TYPE_COMPLETE => {
                let data: CompleteData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::Complete(data)
            }
            MSG_TYPE_HEARTBEAT => {
                let data: HeartbeatData = bincode::deserialize(payload)
                    .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                Message::Heartbeat(data)
            }
            MSG_TYPE_BATCH_CHUNK_ACK => {
                let data: BatchChunkAckData = match bincode::deserialize(payload) {
                    Ok(d) => d,
                    Err(_) => {
                        #[derive(Deserialize)]
                        struct LegacyBatchChunkAckData {
                            transfer_id: Uuid,
                            chunk_ids: Vec<u32>,
                        }
                        let leg: LegacyBatchChunkAckData = bincode::deserialize(payload)
                            .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
                        BatchChunkAckData {
                            transfer_id: leg.transfer_id,
                            chunk_ids: leg.chunk_ids,
                            sum_receiver_verify_us: None,
                        }
                    }
                };
                Message::BatchChunkAck(data)
            }
            other => return Err(ProtocolError::InvalidMessageType(other)),
        };
        Ok(msg)
    }

    /// Serializes the inner message payload to bincode.
    pub fn encode_payload(&self) -> Result<Vec<u8>, ProtocolError> {
        match self {
            Message::Hello(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::TransferOffer(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::TransferAccept(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::TransferReject(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::ChunkData(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::ChunkAck(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::ChunkNack(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::Pause(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::Resume(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::Cancel(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::Complete(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::Heartbeat(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
            Message::BatchChunkAck(d) => {
                bincode::serialize(d).map_err(|e| ProtocolError::SerializationError(e.to_string()))
            }
        }
    }
}

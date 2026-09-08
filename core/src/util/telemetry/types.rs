use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferStage {
    Init,
    Discovery,
    Connection,
    Handshake,
    DiskRead,
    Checksum,
    NetQueue,
    NetSend,
    NetRecv,
    NetAck,
    DiskQueue,
    DiskWrite,
    Finalize,
    Control,
}

impl std::fmt::Display for TransferStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransferStage::Init => write!(f, "INIT"),
            TransferStage::Discovery => write!(f, "DISCOVERY"),
            TransferStage::Connection => write!(f, "CONNECTION"),
            TransferStage::Handshake => write!(f, "HANDSHAKE"),
            TransferStage::DiskRead => write!(f, "DISK_READ"),
            TransferStage::Checksum => write!(f, "CHECKSUM"),
            TransferStage::NetQueue => write!(f, "NET_QUEUE"),
            TransferStage::NetSend => write!(f, "NET_SEND"),
            TransferStage::NetRecv => write!(f, "NET_RECV"),
            TransferStage::NetAck => write!(f, "NET_ACK"),
            TransferStage::DiskQueue => write!(f, "DISK_QUEUE"),
            TransferStage::DiskWrite => write!(f, "DISK_WRITE"),
            TransferStage::Finalize => write!(f, "FINALIZE"),
            TransferStage::Control => write!(f, "CONTROL"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for EventLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventLevel::Debug => write!(f, "DEBUG"),
            EventLevel::Info => write!(f, "INFO"),
            EventLevel::Warn => write!(f, "WARN"),
            EventLevel::Error => write!(f, "ERROR"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferEvent {
    pub timestamp_us: u64,
    pub relative_ms: u64,
    pub stage: TransferStage,
    pub level: EventLevel,
    pub channel: String,
    pub chunk_id: Option<u32>,
    pub duration_us: Option<u64>,
    pub bytes: Option<u64>,
    pub message: String,
    pub details: HashMap<String, String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ChannelMetric {
    pub channel_name: String,
    pub bytes_transferred: u64,
    pub chunks_transferred: u32,
    pub throughput_mbps: f64,
    pub max_in_flight: u32,
    pub avg_socket_write_us: f64,
    pub avg_rtt_ms: f64,
    pub p95_rtt_ms: f64,
    pub nack_count: u64,
    pub disconnect_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BottleneckReport {
    pub transfer_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub role: String,
    pub total_duration_ms: u64,
    pub avg_throughput_mbps: f64,
    pub peak_throughput_mbps: f64,
    pub sender_disk_read_mbps: f64,
    pub sender_disk_read_avg_us: f64,
    pub sender_disk_read_p95_us: f64,
    pub sender_checksum_mbps: f64,
    pub sender_checksum_avg_us: f64,
    pub receiver_disk_write_mbps: f64,
    pub receiver_disk_write_avg_us: f64,
    pub receiver_disk_write_p95_us: f64,
    pub receiver_max_queue_depth: u32,
    pub receiver_finalize_ms: u64,
    pub channels: Vec<ChannelMetric>,
    pub stage_durations_pct: HashMap<String, f64>,
    pub primary_bottleneck: String,
    pub recommendations: Vec<String>,
}

#[derive(Serialize)]
pub struct FullExport<'a> {
    pub report: &'a BottleneckReport,
    pub events: &'a [TransferEvent],
}

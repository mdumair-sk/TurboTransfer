use std::path::PathBuf;
use uuid::Uuid;

use crate::manifest::{TransferRole, TransferStatus};
use crate::transfer::api::{
    cancel_transfer as api_cancel_transfer, enter_receive_mode as api_enter_receive_mode,
    get_devices as api_get_devices, get_progress as api_get_progress,
    get_transfers as api_get_transfers, pause_transfer as api_pause_transfer,
    leave_receive_mode as api_leave_receive_mode,
    resume_transfer as api_resume_transfer, start_transfer as api_start_transfer,
    TransportPreference as ApiTransportPreference,
};

#[derive(uniffi::Enum)]
pub enum FfiTransportPreference {
    Automatic,
    Combined,
    UsbOnly,
    WifiDirectOnly,
}

impl From<FfiTransportPreference> for ApiTransportPreference {
    fn from(pref: FfiTransportPreference) -> Self {
        match pref {
            FfiTransportPreference::Automatic => ApiTransportPreference::Automatic,
            FfiTransportPreference::Combined => ApiTransportPreference::Combined,
            FfiTransportPreference::UsbOnly => ApiTransportPreference::UsbOnly,
            FfiTransportPreference::WifiDirectOnly => ApiTransportPreference::WifiDirectOnly,
        }
    }
}

#[derive(uniffi::Enum)]
pub enum FfiTransferStatus {
    InProgress,
    Completed,
    Paused,
    Failed,
    Cancelled,
}

impl From<TransferStatus> for FfiTransferStatus {
    fn from(status: TransferStatus) -> Self {
        match status {
            TransferStatus::InProgress => FfiTransferStatus::InProgress,
            TransferStatus::Completed => FfiTransferStatus::Completed,
            TransferStatus::Paused => FfiTransferStatus::Paused,
            TransferStatus::Failed => FfiTransferStatus::Failed,
            TransferStatus::Cancelled => FfiTransferStatus::Cancelled,
        }
    }
}

#[derive(uniffi::Enum)]
pub enum FfiTransferRole {
    Sender,
    Receiver,
}

impl From<TransferRole> for FfiTransferRole {
    fn from(role: TransferRole) -> Self {
        match role {
            TransferRole::Sender => FfiTransferRole::Sender,
            TransferRole::Receiver => FfiTransferRole::Receiver,
        }
    }
}

#[derive(uniffi::Record)]
pub struct FfiTransferHandle {
    pub transfer_id: String,
}

#[derive(uniffi::Record)]
pub struct FfiTransferProgress {
    pub transfer_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub bytes_transferred: u64,
    pub percent: f64,
    pub usb_throughput_bps: f64,
    pub wifi_throughput_bps: f64,
    pub aggregate_throughput_bps: f64,
    pub eta_seconds: Option<u64>,
    pub total_chunks: u32,
    pub completed_chunks: u32,
    pub retry_count: u64,
    pub usb_errors: u64,
    pub wifi_errors: u64,
    pub status: FfiTransferStatus,
}

#[derive(uniffi::Record)]
pub struct FfiDeviceInfo {
    pub device_id: String,
    pub device_name: String,
    pub transport: String,
    pub is_connected: bool,
}

#[derive(uniffi::Record)]
pub struct FfiTransferSummary {
    pub transfer_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub status: FfiTransferStatus,
    pub role: FfiTransferRole,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiTransferEvent {
    pub timestamp_us: u64,
    pub relative_ms: u64,
    pub stage: String,
    pub level: String,
    pub channel: String,
    pub chunk_id: Option<u32>,
    pub duration_us: Option<u64>,
    pub bytes: Option<u64>,
    pub message: String,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiChannelMetric {
    pub channel_name: String,
    pub bytes_transferred: u64,
    pub chunks_transferred: u32,
    pub avg_socket_write_us: f64,
    pub avg_rtt_ms: f64,
    pub p95_rtt_ms: f64,
    pub nack_count: u64,
    pub disconnect_count: u64,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiBottleneckReport {
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
    pub channels: Vec<FfiChannelMetric>,
    pub primary_bottleneck: String,
    pub recommendations: Vec<String>,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiTransferLogSummary {
    pub transfer_id: String,
    pub file_name: String,
    pub file_size: u64,
    pub role: String,
    pub avg_throughput_mbps: f64,
    pub primary_bottleneck: String,
    pub log_file_path: String,
    pub json_file_path: String,
}

#[derive(uniffi::Record, Debug, Clone)]
pub struct FfiWifiHotspotInfo {
    pub ssid: String,
    pub passphrase: String,
    pub ip: String,
    pub port: u16,
    pub band: String,
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum FfiTransferError {
    #[error("Transfer error: {msg}")]
    Generic { msg: String },
}

static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();

fn get_runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime for UniFFI")
    })
}

#[uniffi::export]
pub fn init_logger() {
    crate::util::telemetry::init_telemetry_logger();
}

#[uniffi::export]
pub fn get_transfer_logs(transfer_id: String, max_events: Option<u32>) -> Vec<FfiTransferEvent> {
    let id = match Uuid::parse_str(&transfer_id) {
        Ok(i) => i,
        Err(_) => return Vec::new(),
    };
    let limit = max_events.unwrap_or(u32::MAX) as usize;
    if let Some(telemetry) = crate::util::telemetry::get_telemetry(id) {
        telemetry
            .events
            .lock()
            .iter()
            .take(limit)
            .map(|e| FfiTransferEvent {
                timestamp_us: e.timestamp_us,
                relative_ms: e.relative_ms,
                stage: format!("{:?}", e.stage),
                level: format!("{:?}", e.level),
                channel: e.channel.clone(),
                chunk_id: e.chunk_id,
                duration_us: e.duration_us,
                bytes: e.bytes,
                message: e.message.clone(),
            })
            .collect()
    } else {
        Vec::new()
    }
}

#[uniffi::export]
pub fn set_data_directory(path: String) {
    crate::transfer::api::set_custom_data_dir(std::path::PathBuf::from(path));
}

#[uniffi::export]
pub fn get_transfer_log_json(transfer_id: String) -> String {
    let id = match Uuid::parse_str(&transfer_id) {
        Ok(i) => i,
        Err(_) => return "{}".to_string(),
    };
    if let Some(telemetry) = crate::util::telemetry::get_telemetry(id) {
        let report = telemetry.generate_report();
        serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".to_string())
    } else {
        let log_path = crate::transfer::api::default_data_dir().join("logs").join(format!("{}.json", transfer_id));
        std::fs::read_to_string(&log_path).unwrap_or_else(|_| "{}".to_string())
    }
}

#[uniffi::export]
pub fn get_transfer_bottleneck_report(transfer_id: String) -> Option<FfiBottleneckReport> {
    let id = Uuid::parse_str(&transfer_id).ok()?;
    let report = if let Some(telemetry) = crate::util::telemetry::get_telemetry(id) {
        telemetry.generate_report()
    } else {
        let log_path = crate::transfer::api::default_data_dir().join("logs").join(format!("{}.json", transfer_id));
        let content = std::fs::read_to_string(&log_path).ok()?;
        serde_json::from_str::<crate::util::telemetry::BottleneckReport>(&content).ok()?
    };

    Some(FfiBottleneckReport {
        transfer_id: report.transfer_id.to_string(),
        file_name: report.file_name,
        file_size: report.file_size,
        role: format!("{:?}", report.role),
        total_duration_ms: report.total_duration_ms,
        avg_throughput_mbps: report.avg_throughput_mbps,
        peak_throughput_mbps: report.peak_throughput_mbps,
        sender_disk_read_mbps: report.sender_disk_read_mbps,
        sender_disk_read_avg_us: report.sender_disk_read_avg_us,
        sender_disk_read_p95_us: report.sender_disk_read_p95_us,
        sender_checksum_mbps: report.sender_checksum_mbps,
        sender_checksum_avg_us: report.sender_checksum_avg_us,
        receiver_disk_write_mbps: report.receiver_disk_write_mbps,
        receiver_disk_write_avg_us: report.receiver_disk_write_avg_us,
        receiver_disk_write_p95_us: report.receiver_disk_write_p95_us,
        receiver_max_queue_depth: report.receiver_max_queue_depth,
        receiver_finalize_ms: report.receiver_finalize_ms,
        channels: report
            .channels
            .into_iter()
            .map(|c| FfiChannelMetric {
                channel_name: c.channel_name,
                bytes_transferred: c.bytes_transferred,
                chunks_transferred: c.chunks_transferred,
                avg_socket_write_us: c.avg_socket_write_us,
                avg_rtt_ms: c.avg_rtt_ms,
                p95_rtt_ms: c.p95_rtt_ms,
                nack_count: c.nack_count,
                disconnect_count: c.disconnect_count,
            })
            .collect(),
        primary_bottleneck: report.primary_bottleneck,
        recommendations: report.recommendations,
    })
}

#[uniffi::export]
pub fn export_transfer_logs(transfer_id: String, output_dir: Option<String>) -> Result<String, FfiTransferError> {
    let id = Uuid::parse_str(&transfer_id).map_err(|e| FfiTransferError::Generic { msg: e.to_string() })?;
    let out_dir = output_dir.map(PathBuf::from).unwrap_or_else(crate::transfer::api::default_data_dir);
    if let Some(telemetry) = crate::util::telemetry::get_telemetry(id) {
        telemetry
            .export_log_files(&out_dir)
            .map(|(j, _l)| j.to_string_lossy().to_string())
            .map_err(|e| FfiTransferError::Generic { msg: e.to_string() })
    } else {
        let json_path = out_dir.join("logs").join(format!("{}.json", transfer_id));
        if json_path.exists() {
            Ok(json_path.to_string_lossy().to_string())
        } else {
            Err(FfiTransferError::Generic {
                msg: format!("No logs found for transfer {}", transfer_id),
            })
        }
    }
}

#[uniffi::export]
pub fn list_transfer_logs() -> Vec<FfiTransferLogSummary> {
    let logs_dir = crate::transfer::api::default_data_dir().join("logs");
    let mut summaries = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&logs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(report) = serde_json::from_str::<crate::util::telemetry::BottleneckReport>(&content) {
                        let log_path = path.with_extension("log");
                        summaries.push(FfiTransferLogSummary {
                            transfer_id: report.transfer_id.to_string(),
                            file_name: report.file_name,
                            file_size: report.file_size,
                            role: format!("{:?}", report.role),
                            avg_throughput_mbps: report.avg_throughput_mbps,
                            primary_bottleneck: report.primary_bottleneck,
                            log_file_path: log_path.to_string_lossy().to_string(),
                            json_file_path: path.to_string_lossy().to_string(),
                        });
                    }
                }
            }
        }
    }
    summaries
}

#[uniffi::export]
pub fn start_transfer(
    file_path: String,
    file_name: Option<String>,
    device_id: Option<String>,
    transport_pref: FfiTransportPreference,
    address: Option<String>,
) -> Result<FfiTransferHandle, FfiTransferError> {
    let rt = get_runtime();
    let path = PathBuf::from(file_path);
    let dev_id = device_id.and_then(|s| Uuid::parse_str(&s).ok());
    let handle = rt.block_on(async {
        api_start_transfer(path, file_name, dev_id, transport_pref.into(), address).await
    }).map_err(|e| FfiTransferError::Generic {
        msg: e.to_string(),
    })?;

    Ok(FfiTransferHandle {
        transfer_id: handle.transfer_id.to_string(),
    })
}

#[uniffi::export]
pub fn enter_receive_mode(
    address: Option<String>,
    dest_dir: String,
) -> Result<String, FfiTransferError> {
    let rt = get_runtime();
    let dir = PathBuf::from(dest_dir);
    let addr_clone = address.clone();
    let _join_handle = rt.block_on(async {
        api_enter_receive_mode(addr_clone, dir).await
    }).map_err(|e| FfiTransferError::Generic {
        msg: e.to_string(),
    })?;

    let addr = address.unwrap_or_else(|| crate::transfer::api::DEFAULT_LISTEN_ADDR.to_string());
    Ok(format!("Listening on {}", addr))
}

#[uniffi::export]
pub fn stop_receive_mode() -> bool {
    api_leave_receive_mode(None)
}

#[uniffi::export]
pub fn pause_transfer(transfer_id: String) {
    if let Ok(id) = Uuid::parse_str(&transfer_id) {
        api_pause_transfer(id);
    }
}

#[uniffi::export]
pub fn resume_transfer(
    transfer_id: String,
    transport_pref: FfiTransportPreference,
) -> Result<FfiTransferHandle, FfiTransferError> {
    let id = Uuid::parse_str(&transfer_id).map_err(|e| FfiTransferError::Generic {
        msg: format!("Invalid transfer ID: {e}"),
    })?;
    let handle = get_runtime()
        .block_on(api_resume_transfer(Some(id), transport_pref.into(), None))
        .map_err(|e| FfiTransferError::Generic { msg: e.to_string() })?;
    Ok(FfiTransferHandle {
        transfer_id: handle.transfer_id.to_string(),
    })
}

#[uniffi::export]
pub fn cancel_transfer(transfer_id: String) {
    if let Ok(id) = Uuid::parse_str(&transfer_id) {
        api_cancel_transfer(id);
    }
}

#[uniffi::export]
pub fn get_progress(transfer_id: String) -> Option<FfiTransferProgress> {
    let id = Uuid::parse_str(&transfer_id).ok()?;
    let p = api_get_progress(id)?;
    Some(FfiTransferProgress {
        transfer_id: p.transfer_id.to_string(),
        file_name: p.file_name,
        file_size: p.file_size,
        bytes_transferred: p.bytes_transferred,
        percent: p.percent,
        usb_throughput_bps: p.usb_throughput_bps,
        wifi_throughput_bps: p.wifi_throughput_bps,
        aggregate_throughput_bps: p.aggregate_throughput_bps,
        eta_seconds: p.eta_seconds,
        total_chunks: p.total_chunks,
        completed_chunks: p.completed_chunks,
        retry_count: p.retry_count,
        usb_errors: p.usb_errors,
        wifi_errors: p.wifi_errors,
        status: p.status.into(),
    })
}

#[uniffi::export]
pub fn get_devices() -> Vec<FfiDeviceInfo> {
    api_get_devices()
        .into_iter()
        .map(|d| FfiDeviceInfo {
            device_id: d.device_id.to_string(),
            device_name: d.device_name,
            transport: d.transport,
            is_connected: d.is_connected,
        })
        .collect()
}

#[uniffi::export]
pub fn get_transfers() -> Vec<FfiTransferSummary> {
    api_get_transfers()
        .into_iter()
        .map(|t| FfiTransferSummary {
            transfer_id: t.transfer_id.to_string(),
            file_name: t.file_name,
            file_size: t.file_size,
            status: t.status.into(),
            role: t.role.into(),
        })
        .collect()
}

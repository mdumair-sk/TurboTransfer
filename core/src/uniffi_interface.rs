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
pub fn start_transfer(
    file_path: String,
    device_id: Option<String>,
    transport_pref: FfiTransportPreference,
    address: Option<String>,
) -> Result<FfiTransferHandle, FfiTransferError> {
    let rt = get_runtime();
    let path = PathBuf::from(file_path);
    let dev_id = device_id.and_then(|s| Uuid::parse_str(&s).ok());
    let handle = rt.block_on(async {
        api_start_transfer(path, dev_id, transport_pref.into(), address).await
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

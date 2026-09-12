use uuid::Uuid;

pub use crate::benchmark::BenchmarkResult;
use crate::transfer::session::TransferSessionError;

pub use super::discovery::{get_devices, DeviceInfo, TransportPreference};
pub use super::receiver::{enter_receive_mode, leave_receive_mode};
pub use super::registry::{
    cancel_transfer, get_progress, get_transfer_actor_handle, get_transfer_error, get_transfers,
    pause_transfer, record_channel_bytes, register_active_transfer,
    register_active_transfer_with_path, remove_active_transfer, reset_transfer_start_time,
    set_transfer_actor_handle, set_transfer_status, transfer_control_status,
    update_transfer_progress, update_transfer_transport_name, ActiveTransferRecord,
    TransferHandle, TransferProgress, TransferSummary,
};
pub use super::sender::{
    default_data_dir, find_resumable_transfer, prepare_send_mode, resolve_and_connect_transports,
    resolve_and_connect_transports_with_streams, resume_transfer, set_custom_data_dir,
    start_transfer, DEFAULT_LISTEN_ADDR, DEFAULT_LOOPBACK_ADDR, DEFAULT_WIFI_PARALLEL_STREAMS,
};

/// Executes an isolated transport throughput benchmark per TRD §7 and §8.
pub async fn run_benchmark(
    device_id: Option<Uuid>,
    transport_pref: TransportPreference,
    payload_size_mb: u32,
) -> Result<BenchmarkResult, TransferSessionError> {
    crate::benchmark::runner::run_benchmark(device_id, None, transport_pref, Some(payload_size_mb))
        .await
}

/// Executes an isolated transport throughput benchmark to an explicit peer address.
pub async fn run_benchmark_with_address(
    device_id: Option<Uuid>,
    address: Option<&str>,
    transport_pref: TransportPreference,
    payload_size_mb: u32,
) -> Result<BenchmarkResult, TransferSessionError> {
    crate::benchmark::runner::run_benchmark(
        device_id,
        address,
        transport_pref,
        Some(payload_size_mb),
    )
    .await
}

use parking_lot::RwLock;
use std::path::PathBuf;
use uuid::Uuid;

use super::discovery::{get_windows_hotspot_probe_ips, TransportPreference};
use super::registry::{
    get_registry, register_active_transfer_with_path, set_transfer_actor_handle,
    set_transfer_status, update_transfer_progress, update_transfer_transport_name,
    TransferHandle,
};
use super::session::{
    send_file_session_multipath, send_file_session_multipath_ext, SessionOptions,
    TransferSessionError,
};
use crate::manifest::{MetaActor, TransferMeta, TransferRole, TransferStatus};
use crate::transport::{
    TcpTransport, Transport, UsbTransport, UsbTransportConfig, WifiDirectTransport,
};
use crate::util::telemetry::export_and_clean_telemetry;

/// Default loopback TCP address for Milestone 5 / 6 transfers.
pub const DEFAULT_LOOPBACK_ADDR: &str = "127.0.0.1:9876";

/// Default listen address for Milestone 6 real network transfers.
pub const DEFAULT_LISTEN_ADDR: &str = "0.0.0.0:9876";

pub const DEFAULT_WIFI_PARALLEL_STREAMS: usize = 4;

static CUSTOM_DATA_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

pub fn set_custom_data_dir(path: PathBuf) {
    let _ = std::fs::create_dir_all(&path);
    *CUSTOM_DATA_DIR.write() = Some(path);
}

/// Returns the default metadata storage directory (§12).
pub fn default_data_dir() -> PathBuf {
    if let Some(custom) = &*CUSTOM_DATA_DIR.read() {
        let _ = std::fs::create_dir_all(custom);
        return custom.clone();
    }
    if let Ok(dir) = std::env::var("TURBOTRANSFER_DATA_DIR") {
        let p = PathBuf::from(dir);
        let _ = std::fs::create_dir_all(&p);
        return p;
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = PathBuf::from(appdata).join("turbotransfer");
        let _ = std::fs::create_dir_all(&p);
        p
    } else {
        #[cfg(target_os = "android")]
        {
            let candidates = [
                PathBuf::from("/storage/emulated/0/Download/TurboTransfer"),
                PathBuf::from("/sdcard/Download/TurboTransfer"),
                PathBuf::from("/data/data/com.turbotransfer/files"),
                PathBuf::from("/data/user/0/com.turbotransfer/files"),
            ];
            for c in &candidates {
                if std::fs::create_dir_all(c).is_ok() {
                    return c.clone();
                }
            }
        }
        let p = std::env::temp_dir().join("turbotransfer");
        let _ = std::fs::create_dir_all(&p);
        p
    }
}

/// Finds a resumable transfer metadata on disk by ID or returns the most recent incomplete one (§7, §14).
pub fn find_resumable_transfer(target_id: Option<Uuid>) -> Option<(PathBuf, TransferMeta)> {
    let dir = default_data_dir();
    let entries = std::fs::read_dir(dir).ok()?;

    let mut candidate: Option<(PathBuf, TransferMeta, String)> = None;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json")
            || path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with(".meta.json"))
        {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(meta) = serde_json::from_str::<TransferMeta>(&content) {
                    if let Some(tid) = target_id {
                        if meta.transfer_id == tid {
                            return Some((path, meta));
                        }
                    } else if meta.status != TransferStatus::Completed {
                        let is_newer = candidate.as_ref().map_or(true, |(_, _, ts)| {
                            meta.created_at.as_str() > ts.as_str()
                        });
                        if is_newer {
                            candidate = Some((path, meta.clone(), meta.created_at));
                        }
                    }
                }
            }
        }
    }

    candidate.map(|(p, m, _)| (p, m))
}

/// Prepares Android and PC for sending files from PC to Android:
/// 1. Removes any stale ADB reverse tunnel on port 9876.
/// 2. Sets up ADB forward tunnels on ports 9876 (data) and 9875 (hotspot control channel).
/// 3. Autostarts the Android app into Receive Mode (switches to tab 1, starts hotspot, starts receiver).
/// 4. Discovers hotspot credentials and connects Windows Wi-Fi to the Android hotspot.
pub fn prepare_send_mode() {
    #[cfg(not(target_os = "android"))]
    {
        crate::util::runtime::spawn_task(async {
            log::info!("[SendMode] Starting auto Android receive trigger and Wi-Fi discovery...");
            let mut target_serial = None;
            if let Ok(devices) = UsbTransport::list_adb_devices() {
                for dev in devices {
                    if dev.state == "device" {
                        let _ = UsbTransport::remove_adb_reverse(&dev.serial, 9876);
                        let _ = UsbTransport::setup_adb_forward(&dev.serial, 9876, 9876);
                        let _ = UsbTransport::setup_adb_forward(&dev.serial, 9875, 9875);
                        let _ = UsbTransport::trigger_android_receive(&dev.serial);
                        target_serial = Some(dev.serial);
                        break;
                    }
                }
            }

            if let Some(config) = WifiDirectTransport::discover_android_hotspot(target_serial.as_deref()).await {
                log::info!("[SendMode] Discovered hotspot: SSID='{}', associating Windows WLAN...", config.ssid);
                #[cfg(target_os = "windows")]
                {
                    if let Err(e) = WifiDirectTransport::associate_wlan_windows(&config).await {
                        log::warn!("[SendMode] Failed to associate Windows WLAN: {}", e);
                    } else {
                        log::info!("[SendMode] Successfully associated Windows WLAN with '{}'!", config.ssid);
                    }
                }
            }
        });
    }
}

/// Connects all available transport channels according to the preference and network environment.
pub async fn resolve_and_connect_transports(
    transport_pref: TransportPreference,
    address: Option<&str>,
) -> Result<(Vec<(Box<dyn Transport>, bool)>, Vec<String>), TransferSessionError> {
    resolve_and_connect_transports_with_streams(transport_pref, address, None).await
}

pub async fn resolve_and_connect_transports_with_streams(
    transport_pref: TransportPreference,
    address: Option<&str>,
    wifi_stream_count: Option<usize>,
) -> Result<(Vec<(Box<dyn Transport>, bool)>, Vec<String>), TransferSessionError> {
    let stream_count = wifi_stream_count
        .unwrap_or(DEFAULT_WIFI_PARALLEL_STREAMS)
        .max(1);
    let addr_default = DEFAULT_LOOPBACK_ADDR.to_string();
    let addr = address.unwrap_or(&addr_default);
    let mut transports: Vec<(Box<dyn Transport>, bool)> = Vec::new();
    let mut transport_names: Vec<String> = Vec::new();

    #[cfg(not(target_os = "android"))]
    {
        if let Ok(devices) = UsbTransport::list_adb_devices() {
            for dev in devices {
                if dev.state == "device" {
                    let _ = UsbTransport::remove_adb_reverse(&dev.serial, 9876);
                    let _ = UsbTransport::setup_adb_forward(&dev.serial, 9876, 9876);
                    let _ = UsbTransport::setup_adb_forward(&dev.serial, 9875, 9875);
                    if !UsbTransport::is_receiver_listening(&dev.serial, 9876) {
                        let _ = UsbTransport::trigger_android_receive(&dev.serial);
                        for _ in 0..10 {
                            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                            if UsbTransport::is_receiver_listening(&dev.serial, 9876) {
                                break;
                            }
                        }
                    }
                    #[cfg(target_os = "windows")]
                    {
                        if let Some(config) = WifiDirectTransport::probe_hotspot_control_channel(std::time::Duration::from_millis(250)).await {
                            let cur_ssid = WifiDirectTransport::get_current_windows_wifi_ssid();
                            if cur_ssid.as_deref() != Some(&config.ssid) {
                                let _ = WifiDirectTransport::associate_wlan_windows(&config).await;
                            }
                        }
                    }
                    break;
                }
            }
        }
    }

    match transport_pref {
        TransportPreference::UsbOnly => {
            let clean_addr = addr.split(['#', '?']).next().unwrap_or(addr).trim();
            if let Ok(t) = TcpTransport::connect(clean_addr).await {
                transports.push((Box::new(t), true));
                transport_names.push("USB (ADB Tunnel)".to_string());
            } else {
                let config = UsbTransportConfig::new(9876, 9876);
                let t = UsbTransport::connect(config).await?;
                transports.push((Box::new(t), true));
                transport_names.push("USB (ADB Tunnel)".to_string());
            }
        }
        TransportPreference::WifiDirectOnly => {
            if let Some(explicit_addr) = address {
                for stream_idx in 1..=stream_count {
                    if let Ok(transport) = TcpTransport::connect(explicit_addr).await {
                        transports.push((Box::new(transport), false));
                        transport_names
                            .push(format!("5 GHz Wi-Fi Direct (Stream #{})", stream_idx));
                    }
                }
                if transports.is_empty() {
                    let transport = TcpTransport::connect(explicit_addr).await?;
                    transports.push((Box::new(transport), false));
                    transport_names.push("5 GHz Wi-Fi Direct".to_string());
                }
            } else {
                let config = WifiDirectTransport::discover_android_hotspot(None)
                    .await
                    .ok_or_else(|| {
                        TransferSessionError::Rejected(
                            "No Android Local-Only Hotspot was discovered over USB control channel"
                                .into(),
                        )
                    })?;
                let target_ip = if config.target_ip.is_empty() {
                    WifiDirectTransport::resolve_windows_default_gateway()
                        .unwrap_or_else(|_| "10.18.163.130".to_string())
                } else {
                    config.target_ip.clone()
                };
                let target_addr = format!("{}:{}", target_ip, config.port);
                for stream_idx in 1..=stream_count {
                    if let Ok(t) = TcpTransport::connect(&target_addr).await {
                        transports.push((Box::new(t), false));
                        transport_names.push(format!(
                            "5 GHz Local-Only Hotspot (Stream #{})",
                            stream_idx
                        ));
                    }
                }
                if transports.is_empty() {
                    let transport = WifiDirectTransport::connect(config).await?;
                    transports.push((Box::new(transport), false));
                    transport_names.push("5 GHz Local-Only Hotspot".to_string());
                }
            }
        }
        TransportPreference::Combined => {
            let mut usb_connected = false;
            let mut wifi_connected = false;

            if let Some(explicit_addr) = address {
                for single_addr in explicit_addr.split(',') {
                    let trimmed = single_addr.trim();
                    if !trimmed.is_empty() {
                        let is_usb = trimmed.contains("127.0.0.1")
                            || trimmed.contains("localhost")
                            || trimmed.to_lowercase().contains("usb")
                            || trimmed.starts_with("10.125.")
                            || trimmed.starts_with("10.104.")
                            || trimmed.starts_with("192.168.42.");
                        let clean_addr = trimmed.split(['#', '?']).next().unwrap_or(trimmed).trim();
                        if is_usb {
                            let is_adb_tunnel = clean_addr.contains("127.0.0.1")
                                || clean_addr.contains("localhost");
                            let usb_stream_count = if is_adb_tunnel { 1 } else { 2 };
                            for s_idx in 1..=usb_stream_count {
                                if let Ok(t) = TcpTransport::connect(clean_addr).await {
                                    transports.push((Box::new(t), true));
                                    let label = if is_adb_tunnel {
                                        "USB (ADB Tunnel)".to_string()
                                    } else {
                                        format!("USB (RNDIS Stream #{})", s_idx)
                                    };
                                    transport_names.push(label);
                                    usb_connected = true;
                                }
                            }
                        } else {
                            for stream_idx in 1..=stream_count {
                                if let Ok(t) = TcpTransport::connect(clean_addr).await {
                                    transports.push((Box::new(t), false));
                                    transport_names.push(format!(
                                        "5 GHz Wi-Fi Direct (Stream #{})",
                                        stream_idx
                                    ));
                                    wifi_connected = true;
                                }
                            }
                        }
                    }
                }
            }

            // 1. Connect USB channel if not already connected
            if !usb_connected {
                if let Ok(t) = TcpTransport::connect(DEFAULT_LOOPBACK_ADDR).await {
                    transports.push((Box::new(t), true));
                    transport_names.push("USB (ADB Tunnel)".to_string());
                } else {
                    let usb_config = UsbTransportConfig::new(9876, 9876);
                    if let Ok(t) = UsbTransport::connect(usb_config).await {
                        transports.push((Box::new(t), true));
                        transport_names.push("USB (ADB Tunnel)".to_string());
                    }
                }
            }

            // 2. Connect Wi-Fi Direct channel with bonded sockets if not already connected
            let is_explicit_loopback = address
                .map(|a| a.contains("127.0.0.1") || a.contains("localhost") || a.to_lowercase().contains("usb"))
                .unwrap_or(false);
            if !wifi_connected && !is_explicit_loopback {
                let probe_ips = get_windows_hotspot_probe_ips();
                for hotspot_ip in &probe_ips {
                    let mut connected_any = false;
                    for stream_idx in 1..=stream_count {
                        if let Ok(t) = tokio::time::timeout(
                            tokio::time::Duration::from_millis(500),
                            TcpTransport::connect(hotspot_ip),
                        )
                        .await
                        {
                            if let Ok(transport) = t {
                                transports.push((Box::new(transport), false));
                                transport_names.push(format!(
                                    "5 GHz Wi-Fi Direct (Stream #{})",
                                    stream_idx
                                ));
                                connected_any = true;
                            }
                        }
                    }
                    if connected_any {
                        break;
                    }
                }
            }

            if transports.is_empty() {
                return Err(TransferSessionError::Transport(
                    crate::transport::TransportError::Disconnected(
                        "Failed to connect over either USB or Wi-Fi Direct".into(),
                    ),
                ));
            }
        }
        TransportPreference::Automatic => {
            if let Some(explicit_addr) = address {
                for single_addr in explicit_addr.split(',') {
                    let trimmed = single_addr.trim();
                    if !trimmed.is_empty() {
                        let is_usb = trimmed.contains("127.0.0.1")
                            || trimmed.contains("localhost")
                            || trimmed.to_lowercase().contains("usb")
                            || trimmed.starts_with("10.125.")
                            || trimmed.starts_with("10.104.")
                            || trimmed.starts_with("192.168.42.");
                        let clean_addr = trimmed.split(['#', '?']).next().unwrap_or(trimmed).trim();
                        if is_usb {
                            if let Ok(Ok(t)) = tokio::time::timeout(
                                tokio::time::Duration::from_millis(800),
                                TcpTransport::connect(clean_addr),
                            )
                            .await
                            {
                                transports.push((Box::new(t), true));
                                transport_names.push("USB (ADB Tunnel)".to_string());
                            }
                        } else {
                            for stream_idx in 1..=stream_count {
                                if let Ok(Ok(t)) = tokio::time::timeout(
                                    tokio::time::Duration::from_millis(800),
                                    TcpTransport::connect(clean_addr),
                                )
                                .await
                                {
                                    transports.push((Box::new(t), false));
                                    transport_names.push(format!(
                                        "5 GHz Wi-Fi Direct (Stream #{})",
                                        stream_idx
                                    ));
                                }
                            }
                        }
                    }
                }
            }

            if transports.is_empty() {
                #[cfg(target_os = "android")]
                {
                    // Probe USB reverse tunnel
                    if let Ok(t) = tokio::time::timeout(
                        tokio::time::Duration::from_millis(800),
                        TcpTransport::connect(addr),
                    )
                    .await
                    {
                        if let Ok(transport) = t {
                            transports.push((Box::new(transport), true));
                            transport_names.push("USB ADB Reverse Tunnel".to_string());
                        }
                    }
                    // Probe Wi-Fi Direct / Hotspot gateway and ARP peers
                    let mut probe_ips: Vec<String> = vec![
                        "10.18.163.1:9876".to_string(),
                        "10.18.163.2:9876".to_string(),
                        "10.18.163.130:9876".to_string(),
                        "10.78.112.40:9876".to_string(),
                        "192.168.43.1:9876".to_string(),
                        "192.168.43.2:9876".to_string(),
                        "192.168.137.1:9876".to_string(),
                        "192.168.1.19:9876".to_string(),
                    ];
                    if let Ok(arp_content) = std::fs::read_to_string("/proc/net/arp") {
                        for line in arp_content.lines().skip(1) {
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            if let Some(ip) = parts.first() {
                                if !ip.is_empty() && ip.contains('.') {
                                    let addr = format!("{}:9876", ip);
                                    if !probe_ips.contains(&addr) {
                                        probe_ips.push(addr);
                                    }
                                }
                            }
                        }
                    }

                    for hotspot_ip in &probe_ips {
                        let mut connected_any = false;
                        for stream_idx in 1..=stream_count {
                            if let Ok(t) = tokio::time::timeout(
                                tokio::time::Duration::from_millis(500),
                                TcpTransport::connect(hotspot_ip),
                            )
                            .await
                            {
                                if let Ok(transport) = t {
                                    transports.push((Box::new(transport), false));
                                    transport_names.push(format!(
                                        "5 GHz Wi-Fi Direct (Stream #{})",
                                        stream_idx
                                    ));
                                    connected_any = true;
                                }
                            }
                        }
                        if connected_any {
                            break;
                        }
                    }
                }

                #[cfg(not(target_os = "android"))]
                {
                    let usb_config = UsbTransportConfig::new(9876, 9876);
                    if let Ok(t) = UsbTransport::connect(usb_config).await {
                        transports.push((Box::new(t), true));
                        transport_names.push("USB (ADB Tunnel)".to_string());
                    } else if let Ok(t) = TcpTransport::connect(addr).await {
                        transports.push((Box::new(t), true));
                        transport_names.push("USB Tunnel".to_string());
                    }

                    // If USB is already connected or probe hotspot, connect Wi-Fi with bonded streams
                    let probe_ips = get_windows_hotspot_probe_ips();
                    for hotspot_ip in &probe_ips {
                        let mut connected_any = false;
                        for stream_idx in 1..=stream_count {
                            if let Ok(t) = tokio::time::timeout(
                                tokio::time::Duration::from_millis(500),
                                TcpTransport::connect(hotspot_ip),
                            )
                            .await
                            {
                                if let Ok(transport) = t {
                                    transports.push((Box::new(transport), false));
                                    transport_names.push(format!(
                                        "5 GHz Wi-Fi Direct (Stream #{})",
                                        stream_idx
                                    ));
                                    connected_any = true;
                                }
                            }
                        }
                        if connected_any {
                            break;
                        }
                    }
                }
            }

            if transports.is_empty() {
                if let Some(explicit_addr) = address {
                    let fallback_addrs: Vec<&str> = explicit_addr
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .collect();
                    for fallback_addr in &fallback_addrs {
                        if let Ok(Ok(t)) = tokio::time::timeout(
                            tokio::time::Duration::from_millis(800),
                            TcpTransport::connect(fallback_addr),
                        )
                        .await
                        {
                            let is_usb = fallback_addr.contains("127.0.0.1")
                                || fallback_addr.contains("localhost");
                            transports.push((Box::new(t), is_usb));
                            transport_names.push("TCP Transport".to_string());
                            break;
                        }
                    }
                } else {
                    // Fallback to loopback only if running in test environment or loopback server is active
                    if let Ok(Ok(t)) = tokio::time::timeout(
                        tokio::time::Duration::from_millis(400),
                        TcpTransport::connect(DEFAULT_LOOPBACK_ADDR),
                    )
                    .await
                    {
                        transports.push((Box::new(t), true));
                        transport_names.push("Loopback TCP".to_string());
                    }
                }
            }
        }
    }

    if transports.is_empty() {
        return Err(TransferSessionError::Transport(
            crate::transport::TransportError::Disconnected(
                "Failed to establish connection on any transport".into(),
            ),
        ));
    }

    Ok((transports, transport_names))
}

/// Starts a file transfer to a peer over `TcpTransport` or `UsbTransport` (§6, §7, §8).
pub async fn start_transfer(
    file_path: PathBuf,
    custom_file_name: Option<String>,
    device_id: Option<Uuid>,
    transport_pref: TransportPreference,
    address: Option<String>,
) -> Result<TransferHandle, TransferSessionError> {
    let sender_id = Uuid::new_v4();
    let _target_device_id = device_id.unwrap_or_else(Uuid::new_v4);
    let transfer_id = Uuid::new_v4();
    let file_name = custom_file_name.clone().unwrap_or_else(|| {
        let resolved_path = std::fs::read_link(&file_path).unwrap_or_else(|_| file_path.clone());
        resolved_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string()
    });
    let file_size = match std::fs::metadata(&file_path) {
        Ok(m) => m.len(),
        Err(e) => return Err(TransferSessionError::Io(e)),
    };
    // Consult saved calibration config for device pair
    let peer_key = address
        .as_deref()
        .or_else(|| device_id.map(|_| ""))
        .unwrap_or("");
    let saved_cal = if let Some(did) = device_id {
        crate::benchmark::get_saved_calibration(&did.to_string())
            .or_else(|| crate::benchmark::get_saved_calibration(peer_key))
    } else {
        crate::benchmark::get_saved_calibration(peer_key)
    };

    let (wifi_stream_override, chunk_size_override, wifi_window_preset) = if let Some(cal) = &saved_cal {
        (
            cal.config.wifi_stream_count,
            cal.config.chunk_size_bytes,
            cal.config.wifi_window_preset,
        )
    } else {
        (None, None, None)
    };

    let is_high_speed = transport_pref == TransportPreference::UsbOnly
        || transport_pref == TransportPreference::Combined;
    let chunk_size = chunk_size_override
        .unwrap_or_else(|| crate::chunk::select_optimal_chunk_size(file_size, is_high_speed));
    let plan = crate::chunk::calculate_chunk_plan(file_size, chunk_size);
    let total_chunks = plan.len().max(1) as u32;

    // Register active transfer in registry immediately so UI/API can track progress from initiation
    register_active_transfer_with_path(
        transfer_id,
        file_name.clone(),
        file_size,
        TransferRole::Sender,
        total_chunks,
        match transport_pref {
            TransportPreference::UsbOnly => "USB (Connecting...)".to_string(),
            TransportPreference::WifiDirectOnly => "Wi-Fi Direct (Connecting...)".to_string(),
            TransportPreference::Combined => "Multipath (Connecting...)".to_string(),
            TransportPreference::Automatic => "Connecting...".to_string(),
        },
        Some(file_path.clone()),
        0,
        0,
    );

    // Create initial TransferMeta and spawn MetaActor so resumable meta.json exists on disk immediately
    let mut initial_meta = TransferMeta::new(
        transfer_id,
        Uuid::new_v4(),
        file_name.clone(),
        file_size,
        chunk_size,
        total_chunks,
        TransferRole::Sender,
        _target_device_id,
    );
    initial_meta.source_file_path = Some(file_path.to_string_lossy().to_string());
    let meta_path = default_data_dir().join(format!("{}.meta.json", transfer_id));
    let (actor_handle, _actor_join) = MetaActor::spawn(meta_path, initial_meta, 100);
    set_transfer_actor_handle(transfer_id, actor_handle);

    let (transports, transport_names) = match resolve_and_connect_transports_with_streams(
        transport_pref,
        address.as_deref(),
        wifi_stream_override,
    )
    .await
    {
        Ok(res) => res,
        Err(e) => {
            log::error!("start_transfer connection failure: {}", e);
            if let Some(telemetry) = crate::util::telemetry::get_telemetry(transfer_id) {
                telemetry.mark_failed(&e.to_string());
                let data_dir = default_data_dir();
                export_and_clean_telemetry(transfer_id, &data_dir);
            }
            set_transfer_status(transfer_id, TransferStatus::Failed, Some(e.to_string()));
            return Err(e);
        }
    };

    let transport_name = if transport_names.len() > 1 {
        format!("{} (Multipath Active)", transport_names.join(" + "))
    } else {
        transport_names
            .into_iter()
            .next()
            .unwrap_or_else(|| "TCP Transport".to_string())
    };
    log::info!(
        "start_transfer connected successfully via {}",
        transport_name
    );

    update_transfer_transport_name(transfer_id, transport_name);

    let session_opts = SessionOptions {
        purpose: crate::benchmark::TransferPurpose::Normal,
        wifi_window_preset,
    };

    crate::util::runtime::spawn_task(async move {
        let res = send_file_session_multipath_ext(
            sender_id,
            "TurboSender",
            &file_path,
            chunk_size,
            transfer_id,
            transports,
            custom_file_name.as_deref(),
            session_opts,
        )
        .await;

        match res {
            Ok(()) => {
                set_transfer_status(transfer_id, TransferStatus::Completed, None);
            }
            Err(TransferSessionError::Paused | TransferSessionError::Cancelled) => {
                // The public control operation already recorded the terminal
                // state. Do not overwrite it with a transport failure.
            }
            Err(e) => {
                set_transfer_status(transfer_id, TransferStatus::Failed, Some(e.to_string()));
            }
        }
    });

    Ok(TransferHandle { transfer_id })
}

/// Resumes a paused or interrupted transfer from its persisted `meta.json` (§7, §14).
pub async fn resume_transfer(
    transfer_id: Option<Uuid>,
    transport_pref: TransportPreference,
    address: Option<String>,
) -> Result<TransferHandle, TransferSessionError> {
    let (meta_path, meta) = find_resumable_transfer(transfer_id)
        .ok_or_else(|| TransferSessionError::Rejected("No resumable transfer found".into()))?;

    let tid = meta.transfer_id;
    let file_name = meta.file_name.clone();
    let file_size = meta.file_size;
    let role = meta.role;
    let total_chunks = meta.total_chunks;
    let chunk_size = meta.chunk_size;

    // Calculate initial completed chunks and bytes from meta.completed_ranges
    let mut initial_chunks_done = 0u32;
    let mut initial_bytes_done = 0u64;
    for &(start, end) in &meta.completed_ranges {
        let count = end.saturating_sub(start) + 1;
        initial_chunks_done += count;
        for cid in start..=end {
            let chunk_len = if cid == total_chunks.saturating_sub(1) {
                let remainder = (file_size % chunk_size as u64) as u32;
                if remainder > 0 {
                    remainder
                } else {
                    chunk_size
                }
            } else {
                chunk_size
            };
            initial_bytes_done += chunk_len as u64;
        }
    }

    if role == TransferRole::Receiver {
        set_transfer_status(tid, TransferStatus::InProgress, None);
        update_transfer_progress(tid, initial_bytes_done, initial_chunks_done);
        return Ok(TransferHandle { transfer_id: tid });
    }

    // Sender: resolve source file path
    let registry = get_registry();
    let resolved_path = {
        let map = registry.transfers.lock();
        map.get(&tid).and_then(|r| r.source_file_path.clone())
    }
    .or_else(|| meta.source_file_path.as_ref().map(PathBuf::from))
    .or_else(|| {
        let direct = PathBuf::from(&meta.file_name);
        if direct.exists() {
            Some(direct)
        } else {
            let cand1 = default_data_dir().join(&meta.file_name);
            if cand1.exists() {
                Some(cand1)
            } else {
                #[cfg(target_os = "android")]
                {
                    let cand2 = PathBuf::from("/storage/emulated/0/Download").join(&meta.file_name);
                    if cand2.exists() {
                        return Some(cand2);
                    }
                    let cand3 = PathBuf::from("/sdcard/Download").join(&meta.file_name);
                    if cand3.exists() {
                        return Some(cand3);
                    }
                }
                None
            }
        }
    })
    .unwrap_or_else(|| PathBuf::from(&meta.file_name));

    if !resolved_path.exists() {
        return Err(TransferSessionError::Rejected(format!(
            "Source file not found for resume: {:?}. Original file name: {}",
            resolved_path, meta.file_name
        )));
    }

    // Start MetaActor loading from existing meta_path
    let (actor_handle, _join) = MetaActor::spawn(meta_path, meta.clone(), 100);

    // Register / update active transfer preserving completed progress
    register_active_transfer_with_path(
        tid,
        file_name.clone(),
        file_size,
        role,
        total_chunks,
        "Resuming...".to_string(),
        Some(resolved_path.clone()),
        initial_bytes_done,
        initial_chunks_done,
    );
    set_transfer_actor_handle(tid, actor_handle);

    // Connect transports with full multi-channel resolution
    let (transports, transport_names) =
        resolve_and_connect_transports(transport_pref, address.as_deref()).await?;
    let transport_name = if transport_names.len() > 1 {
        format!("{} (Multipath Active)", transport_names.join(" + "))
    } else {
        transport_names
            .into_iter()
            .next()
            .unwrap_or_else(|| "TCP Transport".to_string())
    };
    update_transfer_transport_name(tid, transport_name);

    let sender_id = Uuid::new_v4();
    let file_path = resolved_path;
    let custom_name = meta.file_name.clone();

    crate::util::runtime::spawn_task(async move {
        let res = send_file_session_multipath(
            sender_id,
            "TurboSender",
            &file_path,
            chunk_size,
            tid,
            transports,
            Some(&custom_name),
        )
        .await;

        match res {
            Ok(()) => {
                set_transfer_status(tid, TransferStatus::Completed, None);
            }
            Err(TransferSessionError::Paused | TransferSessionError::Cancelled) => {}
            Err(e) => {
                set_transfer_status(tid, TransferStatus::Failed, Some(e.to_string()));
            }
        }
    });

    Ok(TransferHandle { transfer_id: tid })
}

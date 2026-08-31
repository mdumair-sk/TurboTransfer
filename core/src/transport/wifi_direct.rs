use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use log::{debug, error, info, warn};

use super::{Transport, TransportError, TransportKind, TransportStatus};
use crate::protocol::{encode_frame_parts, FrameReader, Message};

/// Default heartbeat failure timeout (15s per TRD §9 & implementation prompt 7b).
pub const DEFAULT_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(15);

/// Default reconnect retry interval (3s per TRD §9 & implementation prompt 7b).
pub const DEFAULT_RECONNECT_INTERVAL: Duration = Duration::from_secs(3);

/// Configuration parameters for establishing a Wi-Fi Direct / Local Hotspot transport link.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiDirectConfig {
    /// SSID of the Android AP / P2P group (e.g. "AndroidShare_7566").
    pub ssid: String,
    /// WPA2 passphrase for the AP.
    pub passphrase: String,
    /// Target IP of the Android device / Gateway (e.g. "10.18.163.130" or "192.168.43.1").
    pub target_ip: String,
    /// TCP listening port on Android (e.g. 9876).
    pub port: u16,
    /// Local wireless network interface name on Windows (defaults to "Wi-Fi").
    pub interface_name: String,
    /// Timeout duration of frame silence before marking the link as Disconnected.
    pub heartbeat_timeout: Duration,
    /// Retry interval for reconnection attempts.
    pub reconnect_interval: Duration,
}

impl WifiDirectConfig {
    /// Creates a new configuration with standard defaults.
    pub fn new(ssid: impl Into<String>, passphrase: impl Into<String>, target_ip: impl Into<String>, port: u16) -> Self {
        Self {
            ssid: ssid.into(),
            passphrase: passphrase.into(),
            target_ip: target_ip.into(),
            port,
            interface_name: "Wi-Fi".to_string(),
            heartbeat_timeout: DEFAULT_HEARTBEAT_TIMEOUT,
            reconnect_interval: DEFAULT_RECONNECT_INTERVAL,
        }
    }
}

/// Production Wi-Fi Direct transport implementing the shared `Transport` trait (§9).
///
/// Features:
/// - Windows WLAN profile generation and association via `netsh`.
/// - Pure Win32 compatibility (Approach 1B: zero UWP dependencies).
/// - 15-second heartbeat silence failure detector.
/// - 3-second reconnect retry loop state machine.
pub struct WifiDirectTransport {
    config: WifiDirectConfig,
    reader: Option<FrameReader<ReadHalf<TcpStream>>>,
    writer: Option<WriteHalf<TcpStream>>,
    local_addr: Option<SocketAddr>,
    peer_addr: Option<SocketAddr>,
    status: TransportStatus,
    last_frame_received: Arc<Mutex<Instant>>,
    bytes_sent: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
    cleaned_up: Arc<AtomicBool>,
}

impl WifiDirectTransport {
    /// Connects to an Android Wi-Fi Direct / Local Hotspot AP using the specified configuration.
    ///
    /// On Windows, this associates the wireless adapter to the SSID via native WLAN profiles
    /// and establishes a low-latency TCP socket connection.
    pub async fn connect(#[allow(unused_mut)] mut config: WifiDirectConfig) -> Result<Self, TransportError> {
        info!(
            "Initiating Wi-Fi Direct transport connection to SSID='{}', target={}:{}",
            config.ssid, config.target_ip, config.port
        );

        #[cfg(target_os = "windows")]
        {
            Self::associate_wlan_windows(&config).await?;
            if config.target_ip.is_empty() {
                config.target_ip = Self::resolve_windows_default_gateway()?;
            }
        }

        #[cfg(not(target_os = "windows"))]
        if config.target_ip.is_empty() {
            return Err(TransportError::Other(
                "Wi-Fi hotspot gateway must be supplied on this platform".into(),
            ));
        }

        let addr = format!("{}:{}", config.target_ip, config.port);
        debug!("Connecting TCP stream to Android Wi-Fi endpoint at {}", addr);

        let stream = match TcpStream::connect(&addr).await {
            Ok(s) => {
                crate::transport::tcp::configure_tcp_stream(&s);
                s
            }
            Err(e) => {
                warn!("Initial TCP connect to Wi-Fi endpoint {} failed: {}", addr, e);
                return Err(TransportError::Disconnected(format!(
                    "Failed to connect to Wi-Fi endpoint {}: {}",
                    addr, e
                )));
            }
        };

        let local_addr = stream.local_addr().ok();
        let peer_addr = stream.peer_addr().ok();
        let (read_half, write_half) = tokio::io::split(stream);

        info!(
            "Wi-Fi Direct transport connected successfully: local={:?}, peer={:?}",
            local_addr, peer_addr
        );

        Ok(Self {
            config,
            reader: Some(FrameReader::new(read_half)),
            writer: Some(write_half),
            local_addr,
            peer_addr,
            status: TransportStatus::Connected,
            last_frame_received: Arc::new(Mutex::new(Instant::now())),
            bytes_sent: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
            cleaned_up: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Creates a mock or directly connected `WifiDirectTransport` from an existing `TcpStream`
    /// (useful for loopback tests, simulated networks, and in-memory test harnesses).
    pub fn from_stream(stream: TcpStream, config: WifiDirectConfig) -> Self {
        crate::transport::tcp::configure_tcp_stream(&stream);
        let local_addr = stream.local_addr().ok();
        let peer_addr = stream.peer_addr().ok();
        let (read_half, write_half) = tokio::io::split(stream);

        Self {
            config,
            reader: Some(FrameReader::new(read_half)),
            writer: Some(write_half),
            local_addr,
            peer_addr,
            status: TransportStatus::Connected,
            last_frame_received: Arc::new(Mutex::new(Instant::now())),
            bytes_sent: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
            cleaned_up: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Returns the active configuration.
    pub fn config(&self) -> &WifiDirectConfig {
        &self.config
    }

    /// Returns the remote peer socket address if connected.
    pub fn peer_addr(&self) -> Option<SocketAddr> {
        self.peer_addr
    }

    /// Checks if the connection has been silent for longer than `heartbeat_timeout` (15s per TRD §9).
    /// If timed out, marks the transport as `Disconnected`.
    pub async fn check_heartbeat_liveness(&mut self) -> bool {
        if self.status != TransportStatus::Connected {
            return false;
        }

        let elapsed = {
            let last = self.last_frame_received.lock().await;
            last.elapsed()
        };

        if elapsed > self.config.heartbeat_timeout {
            warn!(
                "Wi-Fi Direct heartbeat timeout: elapsed {:.2}s exceeds threshold {:.2}s -> marking Disconnected",
                elapsed.as_secs_f32(),
                self.config.heartbeat_timeout.as_secs_f32()
            );
            self.status = TransportStatus::Disconnected;
            false
        } else {
            true
        }
    }

    /// Reconnect state machine: retries joining the known SSID and reconnecting TCP socket
    /// every `reconnect_interval` (default 3s) until successful or max attempts reached.
    pub async fn reconnect(&mut self, max_attempts: u32) -> Result<(), TransportError> {
        info!(
            "Starting Wi-Fi Direct reconnection retry loop for SSID='{}' (max_attempts={}, interval={:.1}s)",
            self.config.ssid, max_attempts, self.config.reconnect_interval.as_secs_f32()
        );

        self.status = TransportStatus::Connecting;
        let mut attempts = 0;

        while attempts < max_attempts {
            attempts += 1;
            debug!("Reconnection attempt {}/{}", attempts, max_attempts);

            #[cfg(target_os = "windows")]
            {
                let _ = Self::associate_wlan_windows(&self.config).await;
            }

            let addr = format!("{}:{}", self.config.target_ip, self.config.port);
            match TcpStream::connect(&addr).await {
                Ok(stream) => {
                    crate::transport::tcp::configure_tcp_stream(&stream);
                    self.local_addr = stream.local_addr().ok();
                    self.peer_addr = stream.peer_addr().ok();

                    let (read_half, write_half) = tokio::io::split(stream);
                    self.reader = Some(FrameReader::new(read_half));
                    self.writer = Some(write_half);
                    self.status = TransportStatus::Connected;

                    {
                        let mut last = self.last_frame_received.lock().await;
                        *last = Instant::now();
                    }

                    info!(
                        "Wi-Fi Direct reconnected successfully on attempt {}/{}, peer={:?}",
                        attempts, max_attempts, self.peer_addr
                    );
                    return Ok(());
                }
                Err(e) => {
                    debug!("Reconnection attempt {}/{} failed: {}", attempts, max_attempts, e);
                    if attempts < max_attempts {
                        tokio::time::sleep(self.config.reconnect_interval).await;
                    }
                }
            }
        }

        self.status = TransportStatus::Failed;
        Err(TransportError::Disconnected(format!(
            "Reconnection failed after {} attempts",
            max_attempts
        )))
    }

    /// Discovers active Android 5 GHz Hotspot credentials via the USB control channel (port 9875).
    /// If Android has not started the hotspot yet, it triggers it via ADB and retries polling.
    pub async fn discover_android_hotspot(#[allow(unused_variables)] serial: Option<&str>) -> Option<WifiDirectConfig> {
        #[cfg(target_os = "windows")]
        {
            let target_serial = match serial {
                Some(s) => Some(s.to_string()),
                None => {
                    if let Ok(devices) = crate::transport::usb::UsbTransport::list_adb_devices() {
                        devices.into_iter().find(|d| d.state == "device").map(|d| d.serial)
                    } else {
                        None
                    }
                }
            };

            if let Some(ref ser) = target_serial {
                let _ = crate::transport::usb::UsbTransport::setup_adb_forward(ser, 9875, 9875);
                let _ = crate::transport::usb::UsbTransport::trigger_android_hotspot(ser);
            }

            // Retry loop (up to 8 attempts across 4 seconds) to allow Android to grant hotspot reservation
            for attempt in 1..=8 {
                let stream_res = tokio::time::timeout(Duration::from_millis(500), TcpStream::connect("127.0.0.1:9875")).await;
                if let Ok(Ok(stream)) = stream_res {
                    use tokio::io::AsyncBufReadExt;
                    let mut reader = tokio::io::BufReader::new(stream);
                    let mut line = String::new();
                    if let Ok(Ok(_)) = tokio::time::timeout(Duration::from_millis(600), reader.read_line(&mut line)).await {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                            if let (Some(ssid), Some(passphrase)) = (val.get("ssid").and_then(|s| s.as_str()), val.get("passphrase").and_then(|s| s.as_str())) {
                                if !ssid.is_empty() {
                                    let ip = val.get("ip").and_then(|s| s.as_str()).unwrap_or("").to_string();
                                    let port = val.get("port").and_then(|p| p.as_u64()).unwrap_or(9876) as u16;
                                    let config = WifiDirectConfig::new(ssid, passphrase, ip, port);
                                    info!("Discovered Android Direct Hotspot: SSID='{}', IP='{}:{}'", config.ssid, config.target_ip, config.port);
                                    return Some(config);
                                }
                            }
                        }
                    }
                }

                if attempt < 8 {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
        None
    }

    /// Android Local-Only Hotspot addresses are vendor-assigned. Once Windows
    /// has joined the SSID, use the active default route on the Wi-Fi adapter.
    #[cfg(target_os = "windows")]
    pub fn resolve_windows_default_gateway() -> Result<String, TransportError> {
        use std::process::Command;

        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '0.0.0.0/0' | Where-Object { $_.NextHop -ne '0.0.0.0' } | Sort-Object { if ($_.InterfaceAlias -like '*Wi-Fi*' -or $_.InterfaceAlias -like '*Wireless*' -or $_.InterfaceAlias -like '*WLAN*') { 0 } else { 1 } }, RouteMetric | Select-Object -First 1 -ExpandProperty NextHop",
            ])
            .output()
            .map_err(|e| TransportError::Other(format!("Failed to resolve hotspot gateway: {e}")))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let gateway = stdout
            .lines()
            .map(str::trim)
            .find(|line| line.parse::<std::net::Ipv4Addr>().is_ok())
            .ok_or_else(|| TransportError::Other("No active IPv4 default gateway after hotspot association".into()))?;
        Ok(gateway.to_string())
    }

    #[cfg(target_os = "windows")]
    pub fn resolve_windows_all_gateways() -> Vec<String> {
        use std::process::Command;

        let output = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Get-NetRoute -AddressFamily IPv4 -DestinationPrefix '0.0.0.0/0' | Where-Object { $_.NextHop -ne '0.0.0.0' } | Sort-Object { if ($_.InterfaceAlias -like '*Wi-Fi*' -or $_.InterfaceAlias -like '*Wireless*' -or $_.InterfaceAlias -like '*WLAN*') { 0 } else { 1 } }, RouteMetric | Select-Object -ExpandProperty NextHop",
            ])
            .output();

        if let Ok(out) = output {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .map(str::trim)
                .filter(|line| line.parse::<std::net::Ipv4Addr>().is_ok())
                .map(|s| s.to_string())
                .collect()
        } else {
            Vec::new()
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn resolve_windows_default_gateway() -> Result<String, TransportError> {
        Err(TransportError::Other("Not on windows".into()))
    }

    #[cfg(not(target_os = "windows"))]
    pub fn resolve_windows_all_gateways() -> Vec<String> {
        Vec::new()
    }

    /// Generates the XML profile and triggers Windows WLAN association via `netsh`.
    #[cfg(target_os = "windows")]
    pub async fn associate_wlan_windows(config: &WifiDirectConfig) -> Result<(), TransportError> {
        use std::process::Command;

        let ssid = &config.ssid;
        let passphrase = &config.passphrase;

        let profile_xml = format!(
            r#"<?xml version="1.0"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
    <name>{ssid}</name>
    <SSIDConfig>
        <SSID>
            <name>{ssid}</name>
        </SSID>
        <nonBroadcast>true</nonBroadcast>
    </SSIDConfig>
    <connectionType>ESS</connectionType>
    <connectionMode>manual</connectionMode>
    <MSM>
        <security>
            <authEncryption>
                <authentication>WPA2PSK</authentication>
                <encryption>AES</encryption>
                <useOneX>false</useOneX>
            </authEncryption>
            <sharedKey>
                <keyType>passPhrase</keyType>
                <protected>false</protected>
                <keyMaterial>{passphrase}</keyMaterial>
            </sharedKey>
        </security>
    </MSM>
</WLANProfile>"#
        );

        let temp_file = std::env::temp_dir().join(format!("tt_wlan_{}.xml", ssid));
        let _ = std::fs::write(&temp_file, &profile_xml);
        let temp_str = temp_file.to_string_lossy();

        // 1. Add profile
        let _ = Command::new("netsh")
            .args(["wlan", "add", "profile", &format!("filename={}", temp_str), "user=current"])
            .output();

        let _ = std::fs::remove_file(&temp_file);

        // 2. Connect to direct SSID
        let _ = Command::new("netsh")
            .args(["wlan", "connect", &format!("name={}", ssid), &format!("ssid={}", ssid)])
            .output();

        // Wait brief grace period for association
        tokio::time::sleep(Duration::from_millis(1500)).await;

        Ok(())
    }

    /// Queries the currently active Windows Wi-Fi SSID (e.g. Home Wi-Fi) to allow restoration on exit.
    #[cfg(target_os = "windows")]
    pub fn get_current_windows_wifi_ssid() -> Option<String> {
        use std::process::Command;
        let output = Command::new("netsh")
            .args(["wlan", "show", "interfaces"])
            .output()
            .ok()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("SSID") && !trimmed.starts_with("BSSID") {
                if let Some(pos) = trimmed.find(':') {
                    let ssid = trimmed[pos + 1..].trim().to_string();
                    if !ssid.is_empty() {
                        return Some(ssid);
                    }
                }
            }
        }
        None
    }

    #[cfg(not(target_os = "windows"))]
    pub fn get_current_windows_wifi_ssid() -> Option<String> {
        None
    }

    /// Reconnects Windows to the specified Wi-Fi profile/SSID.
    pub fn reconnect_windows_wifi(ssid: &str) -> Result<(), TransportError> {
        #[cfg(target_os = "windows")]
        {
            use std::process::Command;
            let _ = Command::new("netsh")
                .args(["wlan", "connect", &format!("name={}", ssid), &format!("ssid={}", ssid)])
                .output();
            info!("Requested Windows Wi-Fi reconnection to '{}'", ssid);
        }
        let _ = ssid;
        Ok(())
    }

    /// Cleans up the temporary Wi-Fi profile from the OS registry.
    pub fn cleanup_profile(&self) {
        if self.cleaned_up.swap(true, Ordering::SeqCst) {
            return;
        }

        #[cfg(target_os = "windows")]
        {
            use std::process::Command;
            let _ = Command::new("netsh")
                .args(["wlan", "delete", "profile", &format!("name={}", self.config.ssid), &format!("interface={}", self.config.interface_name)])
                .output();
            debug!("Cleaned up temporary WLAN profile '{}' via netsh", self.config.ssid);
        }
    }
}

impl Drop for WifiDirectTransport {
    fn drop(&mut self) {
        self.cleanup_profile();
    }
}

#[async_trait]
impl Transport for WifiDirectTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::WifiDirect
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
            return Err(TransportError::Disconnected(format!(
                "Cannot send frame: Wi-Fi Direct transport is in state {}",
                self.status
            )));
        }

        let writer = self.writer.as_mut().ok_or_else(|| {
            TransportError::Disconnected("Wi-Fi Direct writer is unavailable".into())
        })?;

        let (header, maybe_payload) = encode_frame_parts(msg)?;
        let payload = maybe_payload.unwrap_or(&[]);
        let frame_len = (header.len() + payload.len()) as u64;

        if let Err(e) = super::vectored::write_all_vectored(writer, &header, payload).await {
            self.status = TransportStatus::Disconnected;
            error!("Wi-Fi Direct socket write error -> marked Disconnected: {}", e);
            return Err(TransportError::Disconnected(format!(
                "Wi-Fi Direct socket write failed: {}",
                e
            )));
        }

        if let Err(e) = writer.flush().await {
            self.status = TransportStatus::Disconnected;
            error!("Wi-Fi Direct socket flush error -> marked Disconnected: {}", e);
            return Err(TransportError::Disconnected(format!(
                "Wi-Fi Direct socket flush failed: {}",
                e
            )));
        }

        self.bytes_sent.fetch_add(frame_len, Ordering::Relaxed);
        Ok(())
    }

    async fn receive_frame(&mut self) -> Result<Option<Message>, TransportError> {
        if self.status != TransportStatus::Connected {
            return Err(TransportError::Disconnected(format!(
                "Cannot receive frame: Wi-Fi Direct transport is in state {}",
                self.status
            )));
        }

        let reader = self.reader.as_mut().ok_or_else(|| {
            TransportError::Disconnected("Wi-Fi Direct reader is unavailable".into())
        })?;

        match reader.read_frame_with_length().await {
            Ok(Some((msg, frame_len))) => {
                // Update heartbeat liveness timestamp
                {
                    let mut last = self.last_frame_received.lock().await;
                    *last = Instant::now();
                }

                self.bytes_received.fetch_add(frame_len as u64, Ordering::Relaxed);
                Ok(Some(msg))
            }
            Ok(None) => {
                self.status = TransportStatus::Disconnected;
                info!("Wi-Fi Direct connection closed by remote peer (EOF)");
                Ok(None)
            }
            Err(e) => {
                self.status = TransportStatus::Disconnected;
                warn!("Wi-Fi Direct read frame error -> marked Disconnected: {}", e);
                Err(TransportError::from(e))
            }
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.status = TransportStatus::Disconnected;
        if let Some(mut writer) = self.writer.take() {
            let _ = writer.shutdown().await;
        }
        self.reader = None;
        self.cleanup_profile();
        info!("Wi-Fi Direct transport closed cleanly");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{HeartbeatData, Message};
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn test_wifi_direct_status_lifecycle_and_framing() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(addr).await.unwrap();
        let (server_stream, _) = listener.accept().await.unwrap();

        let config = WifiDirectConfig::new("TestSSID", "TestPass", "127.0.0.1", addr.port());
        let mut client_transport = WifiDirectTransport::from_stream(client_stream, config.clone());
        let mut server_transport = WifiDirectTransport::from_stream(server_stream, config);

        assert_eq!(client_transport.kind(), TransportKind::WifiDirect);
        assert_eq!(client_transport.status(), TransportStatus::Connected);
        assert!(client_transport.is_connected());

        // Send Heartbeat frame
        let hb = Message::Heartbeat(HeartbeatData {
            sequence: 12345678,
        });

        client_transport.send_frame(&hb).await.unwrap();
        assert!(client_transport.bytes_sent() > 0);

        let received = server_transport.receive_frame().await.unwrap().unwrap();
        match received {
            Message::Heartbeat(msg) => assert_eq!(msg.sequence, 12345678),
            _ => panic!("Expected Heartbeat message"),
        }

        // Close transport
        client_transport.close().await.unwrap();
        assert_eq!(client_transport.status(), TransportStatus::Disconnected);
        assert!(!client_transport.is_connected());

        // Subsequent sends must fail
        let send_result = client_transport.send_frame(&hb).await;
        assert!(send_result.is_err());
    }

    #[tokio::test]
    async fn test_wifi_direct_heartbeat_timeout_failure_detection() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(addr).await.unwrap();
        let (_server_stream, _) = listener.accept().await.unwrap();

        let mut config = WifiDirectConfig::new("TestSSID", "TestPass", "127.0.0.1", addr.port());
        // Set short heartbeat timeout for test
        config.heartbeat_timeout = Duration::from_millis(50);

        let mut transport = WifiDirectTransport::from_stream(client_stream, config);
        assert_eq!(transport.status(), TransportStatus::Connected);

        // Initially within threshold
        assert!(transport.check_heartbeat_liveness().await);

        // Sleep past threshold
        tokio::time::sleep(Duration::from_millis(70)).await;

        // check_heartbeat_liveness detects silence and marks Disconnected
        let is_alive = transport.check_heartbeat_liveness().await;
        assert!(!is_alive);
        assert_eq!(transport.status(), TransportStatus::Disconnected);

        // Subsequent sends must be rejected
        let hb = Message::Heartbeat(HeartbeatData {
            sequence: 9999,
        });
        let res = transport.send_frame(&hb).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_wifi_direct_reconnect_retry_state_machine() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(addr).await.unwrap();
        let (_server_stream, _) = listener.accept().await.unwrap();

        let mut config = WifiDirectConfig::new("TestSSID", "TestPass", "127.0.0.1", addr.port());
        config.reconnect_interval = Duration::from_millis(20);

        let mut transport = WifiDirectTransport::from_stream(client_stream, config);
        // Force Disconnected
        transport.status = TransportStatus::Disconnected;
        assert!(!transport.is_connected());

        // Spawn background listener to accept reconnection
        let accept_handle = tokio::spawn(async move {
            let (reconnected_stream, _) = listener.accept().await.unwrap();
            reconnected_stream
        });

        // Trigger reconnect
        let res = transport.reconnect(3).await;
        assert!(res.is_ok());
        assert_eq!(transport.status(), TransportStatus::Connected);
        assert!(transport.is_connected());

        let _ = accept_handle.await.unwrap();
    }
}

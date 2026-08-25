use async_trait::async_trait;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;
use uuid::Uuid;

use super::{Transport, TransportError, TransportKind, TransportStatus};
use crate::protocol::{encode_frame, FrameReader, HelloData, Message};

/// Default reconnect poll interval (2s per TRD §8).
pub const DEFAULT_USB_RECONNECT_INTERVAL: Duration = Duration::from_secs(2);

/// Default handshake timeout when probing for TurboTransfer service (3s).
pub const DEFAULT_USB_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);

/// Information about a connected ADB device parsed from `adb devices -l` (§8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdbDeviceInfo {
    pub serial: String,
    pub state: String,
    pub product: Option<String>,
    pub model: Option<String>,
    pub device: Option<String>,
}

impl AdbDeviceInfo {
    /// Parses the raw output of `adb devices -l` into a list of `AdbDeviceInfo`.
    pub fn parse_adb_devices_output(output: &str) -> Vec<Self> {
        let mut devices = Vec::new();
        for line in output.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("List of devices") || trimmed.starts_with('*') {
                continue;
            }

            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }

            let serial = parts[0].to_string();
            let state = parts[1].to_string();

            let mut product = None;
            let mut model = None;
            let mut device = None;

            for part in &parts[2..] {
                if let Some(val) = part.strip_prefix("product:") {
                    product = Some(val.to_string());
                } else if let Some(val) = part.strip_prefix("model:") {
                    model = Some(val.to_string());
                } else if let Some(val) = part.strip_prefix("device:") {
                    device = Some(val.to_string());
                }
            }

            devices.push(AdbDeviceInfo {
                serial,
                state,
                product,
                model,
                device,
            });
        }
        devices
    }
}

/// Configuration parameters for establishing a USB transport over an ADB tunnel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbTransportConfig {
    /// Specific device serial number, or `None` to auto-select the first verified device.
    pub device_serial: Option<String>,
    /// Local forwarded TCP port on Windows (default 9876).
    pub local_port: u16,
    /// Remote TCP port on Android device (default 9876).
    pub remote_port: u16,
    /// Polling interval for detecting device reconnection.
    pub reconnect_interval: Duration,
    /// Timeout for `Hello` handshake verification.
    pub handshake_timeout: Duration,
}

impl Default for UsbTransportConfig {
    fn default() -> Self {
        Self {
            device_serial: None,
            local_port: 9876,
            remote_port: 9876,
            reconnect_interval: DEFAULT_USB_RECONNECT_INTERVAL,
            handshake_timeout: DEFAULT_USB_HANDSHAKE_TIMEOUT,
        }
    }
}

impl UsbTransportConfig {
    pub fn new(local_port: u16, remote_port: u16) -> Self {
        Self {
            local_port,
            remote_port,
            ..Default::default()
        }
    }

    pub fn with_serial(mut self, serial: impl Into<String>) -> Self {
        self.device_serial = Some(serial.into());
        self
    }
}

/// Production USB transport implementing the shared `Transport` trait over ADB forward (§8).
///
/// Features:
/// - Windows host enumerates and filters ADB devices.
/// - Verifies TurboTransfer service via framed `Message::Hello` handshake.
/// - Immediate failure detection on socket errors.
/// - 2-second reconnect polling state machine.
/// - Automatic tunnel forwarding and cleanup on drop.
pub struct UsbTransport {
    config: UsbTransportConfig,
    active_serial: Option<String>,
    peer_info: Option<HelloData>,
    reader: Option<FrameReader<ReadHalf<TcpStream>>>,
    writer: Option<WriteHalf<TcpStream>>,
    local_addr: Option<SocketAddr>,
    peer_addr: Option<SocketAddr>,
    status: TransportStatus,
    bytes_sent: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
    cleaned_up: Arc<AtomicBool>,
}

impl UsbTransport {
    /// Connects to an ADB-forwarded TCP tunnel directly using the specified configuration.
    pub async fn connect(config: UsbTransportConfig) -> Result<Self, TransportError> {
        let devices = Self::list_adb_devices()?;
        let target_device = match &config.device_serial {
            Some(serial) => devices.into_iter().find(|d| d.serial == *serial && d.state == "device"),
            None => devices.into_iter().find(|d| d.state == "device"),
        };

        let active_serial = if let Some(dev) = target_device {
            let _ = Self::setup_adb_forward(&dev.serial, config.local_port, config.remote_port);
            // Verify and trigger Android receiver if not yet listening
            if !Self::is_receiver_listening(&dev.serial, config.remote_port) {
                let _ = Self::trigger_android_receive(&dev.serial);
                for _ in 0..10 {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    if Self::is_receiver_listening(&dev.serial, config.remote_port) {
                        break;
                    }
                }
            }
            Some(dev.serial)
        } else {
            None
        };

        let target_addr = format!("127.0.0.1:{}", config.local_port);
        let stream = match TcpStream::connect(&target_addr).await {
            Ok(s) => {
                crate::transport::tcp::configure_tcp_stream(&s);
                s
            }
            Err(e) => {
                return Err(TransportError::Disconnected(format!(
                    "Failed to connect to USB tunnel endpoint {}: {}",
                    target_addr, e
                )));
            }
        };

        let local_addr = stream.local_addr().ok();
        let peer_addr = stream.peer_addr().ok();
        let (read_half, write_half) = tokio::io::split(stream);

        Ok(Self {
            config,
            active_serial,
            peer_info: None,
            reader: Some(FrameReader::new(read_half)),
            writer: Some(write_half),
            local_addr,
            peer_addr,
            status: TransportStatus::Connected,
            bytes_sent: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
            cleaned_up: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Scans for ADB devices, sets up port forwarding, performs `Hello` handshake,
    /// and returns an established `UsbTransport`.
    pub async fn discover_and_connect(
        config: UsbTransportConfig,
        local_hello: &HelloData,
    ) -> Result<Self, TransportError> {
        info!("Discovering ADB devices for USB transport...");

        let devices = Self::list_adb_devices()?;
        let target_device = match &config.device_serial {
            Some(serial) => devices.into_iter().find(|d| d.serial == *serial && d.state == "device"),
            None => devices.into_iter().find(|d| d.state == "device"),
        };

        let device = target_device.ok_or_else(|| {
            TransportError::Disconnected("No active ADB device in 'device' state found".into())
        })?;

        info!(
            "Selected ADB device: serial='{}', model={:?}",
            device.serial, device.model
        );

        Self::setup_adb_forward(&device.serial, config.local_port, config.remote_port)?;

        let target_addr = format!("127.0.0.1:{}", config.local_port);
        debug!("Connecting TCP stream over ADB tunnel to {}", target_addr);

        let stream = match TcpStream::connect(&target_addr).await {
            Ok(s) => {
                crate::transport::tcp::configure_tcp_stream(&s);
                s
            }
            Err(e) => {
                let _ = Self::remove_adb_forward(&device.serial, config.local_port);
                return Err(TransportError::Disconnected(format!(
                    "Failed to connect to forwarded ADB port {}: {}",
                    target_addr, e
                )));
            }
        };

        let local_addr = stream.local_addr().ok();
        let peer_addr = stream.peer_addr().ok();
        let (read_half, mut write_half) = tokio::io::split(stream);
        let mut reader = FrameReader::new(read_half);

        // Perform framed Hello handshake (§6.1, §8)
        let hello_msg = Message::Hello(local_hello.clone());
        let hello_frame = encode_frame(&hello_msg)?;

        write_half.write_all(&hello_frame).await.map_err(|e| {
            let _ = Self::remove_adb_forward(&device.serial, config.local_port);
            TransportError::Disconnected(format!("Failed to send Hello handshake: {}", e))
        })?;
        write_half.flush().await.map_err(|e| {
            let _ = Self::remove_adb_forward(&device.serial, config.local_port);
            TransportError::Disconnected(format!("Failed to flush Hello handshake: {}", e))
        })?;

        let peer_info = match tokio::time::timeout(config.handshake_timeout, reader.read_frame()).await {
            Ok(Ok(Some(Message::Hello(peer)))) => {
                info!(
                    "USB Hello handshake verified: peer_name='{}', peer_id={}",
                    peer.device_name, peer.device_id
                );
                peer
            }
            Ok(Ok(Some(other))) => {
                let _ = Self::remove_adb_forward(&device.serial, config.local_port);
                return Err(TransportError::Protocol(crate::protocol::ProtocolError::InvalidMessageType(
                    other.message_type(),
                )));
            }
            Ok(Ok(None)) => {
                let _ = Self::remove_adb_forward(&device.serial, config.local_port);
                return Err(TransportError::Disconnected("Peer closed connection during handshake".into()));
            }
            Ok(Err(e)) => {
                let _ = Self::remove_adb_forward(&device.serial, config.local_port);
                return Err(TransportError::Protocol(e));
            }
            Err(_) => {
                let _ = Self::remove_adb_forward(&device.serial, config.local_port);
                return Err(TransportError::Timeout("Hello handshake timed out".into()));
            }
        };

        Ok(Self {
            config,
            active_serial: Some(device.serial),
            peer_info: Some(peer_info),
            reader: Some(reader),
            writer: Some(write_half),
            local_addr,
            peer_addr,
            status: TransportStatus::Connected,
            bytes_sent: Arc::new(AtomicU64::new(hello_frame.len() as u64)),
            bytes_received: Arc::new(AtomicU64::new(64)),
            cleaned_up: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Creates a mock or directly connected `UsbTransport` from an existing `TcpStream`
    /// (used for tests and in-memory verification harnesses).
    pub fn from_stream(stream: TcpStream, config: UsbTransportConfig) -> Self {
        crate::transport::tcp::configure_tcp_stream(&stream);
        let local_addr = stream.local_addr().ok();
        let peer_addr = stream.peer_addr().ok();
        let (read_half, write_half) = tokio::io::split(stream);

        Self {
            config,
            active_serial: None,
            peer_info: Some(HelloData {
                device_id: Uuid::new_v4(),
                device_name: "Mock-Android".to_string(),
                protocol_version: 1,
            }),
            reader: Some(FrameReader::new(read_half)),
            writer: Some(write_half),
            local_addr,
            peer_addr,
            status: TransportStatus::Connected,
            bytes_sent: Arc::new(AtomicU64::new(0)),
            bytes_received: Arc::new(AtomicU64::new(0)),
            cleaned_up: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Resolves the most appropriate ADB binary path.
    pub fn get_adb_path() -> std::path::PathBuf {
        // 1. Check C:\adb\adb.exe
        let c_adb = std::path::PathBuf::from(r"C:\adb\adb.exe");
        if c_adb.is_file() {
            return c_adb;
        }

        // 2. Check ANDROID_HOME / ANDROID_SDK_ROOT
        if let Ok(android_home) = std::env::var("ANDROID_HOME").or_else(|_| std::env::var("ANDROID_SDK_ROOT")) {
            let pt_adb = std::path::PathBuf::from(android_home)
                .join("platform-tools")
                .join(if cfg!(windows) { "adb.exe" } else { "adb" });
            if pt_adb.is_file() {
                return pt_adb;
            }
        }

        // 3. Check LOCALAPPDATA / WinGet platform-tools
        if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
            let winget_adb = std::path::PathBuf::from(local_appdata)
                .join("Microsoft")
                .join("WinGet")
                .join("Packages")
                .join("Google.PlatformTools_Microsoft.Winget.Source_8wekyb3d8bbwe")
                .join("platform-tools")
                .join("adb.exe");
            if winget_adb.is_file() {
                return winget_adb;
            }
        }

        // 4. Default to adb in PATH
        std::path::PathBuf::from("adb")
    }

    /// Lists connected ADB devices by running `adb devices -l`.
    pub fn list_adb_devices() -> Result<Vec<AdbDeviceInfo>, TransportError> {
        let adb_path = Self::get_adb_path();
        let output = Command::new(&adb_path)
            .args(["devices", "-l"])
            .output()
            .map_err(|e| TransportError::Other(format!("Failed to execute '{:?} devices -l': {}", adb_path, e)))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(AdbDeviceInfo::parse_adb_devices_output(&stdout))
    }

    /// Sets up ADB forward rule for a specific device.
    pub fn setup_adb_forward(serial: &str, local_port: u16, remote_port: u16) -> Result<(), TransportError> {
        let adb_path = Self::get_adb_path();
        let output = Command::new(&adb_path)
            .args([
                "-s",
                serial,
                "forward",
                &format!("tcp:{}", local_port),
                &format!("tcp:{}", remote_port),
            ])
            .output()
            .map_err(|e| TransportError::Other(format!("Failed to execute '{:?} forward': {}", adb_path, e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TransportError::Other(format!("adb forward failed: {}", stderr)));
        }

        debug!("Set up adb forward tcp:{} -> tcp:{} for {}", local_port, remote_port, serial);
        Ok(())
    }

    /// Removes ADB forward rule for a specific device.
    pub fn remove_adb_forward(serial: &str, local_port: u16) -> Result<(), TransportError> {
        let adb_path = Self::get_adb_path();
        let _ = Command::new(&adb_path)
            .args(["-s", serial, "forward", "--remove", &format!("tcp:{}", local_port)])
            .output();
        debug!("Removed adb forward tcp:{} for {}", local_port, serial);
        Ok(())
    }

    /// Sets up ADB reverse rule so Android can connect to Windows host.
    pub fn setup_adb_reverse(serial: &str, remote_port: u16, local_port: u16) -> Result<(), TransportError> {
        let adb_path = Self::get_adb_path();
        let output = Command::new(&adb_path)
            .args([
                "-s",
                serial,
                "reverse",
                &format!("tcp:{}", remote_port),
                &format!("tcp:{}", local_port),
            ])
            .output()
            .map_err(|e| TransportError::Other(format!("Failed to execute '{:?} reverse': {}", adb_path, e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("adb reverse returned non-zero status: {}", stderr);
        } else {
            debug!("Set up adb reverse tcp:{} -> tcp:{} for {}", remote_port, local_port, serial);
        }
        Ok(())
    }

    /// Configures all required ADB reverse and forward tunnels for data and direct hotspot discovery.
    pub fn setup_default_adb_tunnels(serial: &str) -> Result<(), TransportError> {
        let _ = Self::setup_adb_reverse(serial, 9876, 9876);
        let _ = Self::setup_adb_forward(serial, 9875, 9875);
        let _ = Self::remove_adb_forward(serial, 9876);
        Ok(())
    }

    /// Triggers the Android app to spin up its 5 GHz Local-Only Hotspot via ADB broadcast.
    pub fn trigger_android_hotspot(serial: &str) -> Result<(), TransportError> {
        let adb_path = Self::get_adb_path();
        let _ = Command::new(&adb_path)
            .args(["-s", serial, "shell", "am", "broadcast", "-a", "com.turbotransfer.START_HOTSPOT"])
            .output();
        debug!("Triggered START_HOTSPOT broadcast on device {}", serial);
        Ok(())
    }

    /// Triggers the Android app to enter Receive mode via ADB broadcast.
    pub fn trigger_android_receive(serial: &str) -> Result<(), TransportError> {
        let adb_path = Self::get_adb_path();
        let _ = Command::new(&adb_path)
            .args(["-s", serial, "shell", "am", "broadcast", "-a", "com.turbotransfer.ENTER_RECEIVE"])
            .output();
        debug!("Triggered ENTER_RECEIVE broadcast on device {}", serial);
        Ok(())
    }

    /// Triggers the Android app to stop hotspot via ADB broadcast.
    pub fn trigger_android_stop_hotspot(serial: &str) -> Result<(), TransportError> {
        let adb_path = Self::get_adb_path();
        let _ = Command::new(&adb_path)
            .args(["-s", serial, "shell", "am", "broadcast", "-a", "com.turbotransfer.STOP_HOTSPOT"])
            .output();
        debug!("Triggered STOP_HOTSPOT broadcast on device {}", serial);
        Ok(())
    }

    /// Removes ADB reverse rule for a specific device.
    pub fn remove_adb_reverse(serial: &str, remote_port: u16) -> Result<(), TransportError> {
        let adb_path = Self::get_adb_path();
        let _ = Command::new(&adb_path)
            .args(["-s", serial, "reverse", "--remove", &format!("tcp:{}", remote_port)])
            .output();
        debug!("Removed adb reverse tcp:{} for {}", remote_port, serial);
        Ok(())
    }

    /// Kills any running ADB server process.
    pub fn kill_adb_server() -> Result<(), TransportError> {
        let adb_path = Self::get_adb_path();
        let _ = Command::new(&adb_path).arg("kill-server").output();
        info!("Executed 'adb kill-server'");
        Ok(())
    }

    /// Resets the ADB server (kills existing instance, starts fresh server).
    pub fn reset_adb_server() -> Result<(), TransportError> {
        let adb_path = Self::get_adb_path();
        let _ = Command::new(&adb_path).arg("kill-server").output();
        std::thread::sleep(Duration::from_millis(200));
        let _ = Command::new(&adb_path).arg("start-server").output();
        info!("Executed ADB server reset (kill-server -> start-server)");
        Ok(())
    }

    /// Starts USB RNDIS tethering on connected Android device via ADB.
    pub fn start_usb_tethering(serial: Option<&str>) -> Result<(), TransportError> {
        let adb_path = Self::get_adb_path();
        let mut cmd1 = Command::new(&adb_path);
        if let Some(s) = serial {
            cmd1.args(["-s", s]);
        }
        cmd1.args(["shell", "cmd", "connectivity", "tether", "start-tethering", "usb"]);
        let _ = cmd1.output();

        let mut cmd2 = Command::new(&adb_path);
        if let Some(s) = serial {
            cmd2.args(["-s", s]);
        }
        cmd2.args(["shell", "svc", "usb", "setFunctions", "rndis"]);
        let _ = cmd2.output();

        info!("Triggered Android USB tethering start");
        Ok(())
    }

    /// Stops USB tethering on connected Android device and returns USB mode to MTP.
    pub fn stop_usb_tethering(serial: Option<&str>) -> Result<(), TransportError> {
        let adb_path = Self::get_adb_path();
        let mut cmd1 = Command::new(&adb_path);
        if let Some(s) = serial {
            cmd1.args(["-s", s]);
        }
        cmd1.args(["shell", "cmd", "connectivity", "tether", "stop-tethering", "usb"]);
        let _ = cmd1.output();

        let mut cmd2 = Command::new(&adb_path);
        if let Some(s) = serial {
            cmd2.args(["-s", s]);
        }
        cmd2.args(["shell", "svc", "usb", "setFunctions", "mtp"]);
        let _ = cmd2.output();

        info!("Triggered Android USB tethering stop");
        Ok(())
    }

    /// Probes if the target ADB device is actively running a TurboTransfer receiver on the specified port.
    pub fn is_receiver_listening(serial: &str, port: u16) -> bool {
        let _ = Self::setup_adb_forward(serial, port, port);
        std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            Duration::from_millis(150),
        ).is_ok()
    }

    /// Returns the verified peer device info if handshake completed.
    pub fn peer_info(&self) -> Option<&HelloData> {
        self.peer_info.as_ref()
    }

    /// Returns the active ADB serial number.
    pub fn active_serial(&self) -> Option<&str> {
        self.active_serial.as_deref()
    }

    /// Reconnect loop: polls `adb devices` every `reconnect_interval` (2s per TRD §8)
    /// and re-establishes the tunnel and handshake.
    pub async fn reconnect(
        &mut self,
        max_attempts: u32,
        local_hello: &HelloData,
    ) -> Result<(), TransportError> {
        info!(
            "Starting USB reconnection polling loop (max_attempts={}, interval={:.1}s)",
            max_attempts,
            self.config.reconnect_interval.as_secs_f32()
        );

        self.status = TransportStatus::Connecting;
        let mut attempts = 0;

        while attempts < max_attempts {
            attempts += 1;
            debug!("USB Reconnection poll attempt {}/{}", attempts, max_attempts);

            if let Ok(new_transport) = Self::discover_and_connect(self.config.clone(), local_hello).await {
                let mut new_transport = std::mem::ManuallyDrop::new(new_transport);
                self.active_serial = new_transport.active_serial.take();
                self.peer_info = new_transport.peer_info.take();
                self.reader = new_transport.reader.take();
                self.writer = new_transport.writer.take();
                self.local_addr = new_transport.local_addr;
                self.peer_addr = new_transport.peer_addr;
                self.status = TransportStatus::Connected;

                info!(
                    "USB transport reconnected successfully on attempt {}/{}",
                    attempts, max_attempts
                );
                return Ok(());
            }

            if attempts < max_attempts {
                tokio::time::sleep(self.config.reconnect_interval).await;
            }
        }

        self.status = TransportStatus::Failed;
        Err(TransportError::Disconnected(format!(
            "USB reconnection failed after {} polling attempts",
            max_attempts
        )))
    }

    /// Cleans up the forwarded ADB port rule.
    pub fn cleanup_tunnel(&self) {
        if self.cleaned_up.swap(true, Ordering::SeqCst) {
            return;
        }

        if let Some(serial) = &self.active_serial {
            let _ = Self::remove_adb_forward(serial, self.config.local_port);
        }
    }
}

impl Drop for UsbTransport {
    fn drop(&mut self) {
        self.cleanup_tunnel();
    }
}

#[async_trait]
impl Transport for UsbTransport {
    fn kind(&self) -> TransportKind {
        TransportKind::Usb
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
                "Cannot send frame: USB transport is in state {}",
                self.status
            )));
        }

        let writer = self.writer.as_mut().ok_or_else(|| {
            TransportError::Disconnected("USB transport writer is unavailable".into())
        })?;

        let frame = encode_frame(msg)?;
        let frame_len = frame.len() as u64;

        if let Err(e) = writer.write_all(&frame).await {
            self.status = TransportStatus::Disconnected;
            error!("USB socket write error -> marked Disconnected: {}", e);
            return Err(TransportError::Disconnected(format!(
                "USB socket write failed: {}",
                e
            )));
        }

        if let Err(e) = writer.flush().await {
            self.status = TransportStatus::Disconnected;
            error!("USB socket flush error -> marked Disconnected: {}", e);
            return Err(TransportError::Disconnected(format!(
                "USB socket flush failed: {}",
                e
            )));
        }

        self.bytes_sent.fetch_add(frame_len, Ordering::Relaxed);
        Ok(())
    }

    async fn receive_frame(&mut self) -> Result<Option<Message>, TransportError> {
        if self.status != TransportStatus::Connected {
            return Err(TransportError::Disconnected(format!(
                "Cannot receive frame: USB transport is in state {}",
                self.status
            )));
        }

        let reader = self.reader.as_mut().ok_or_else(|| {
            TransportError::Disconnected("USB transport reader is unavailable".into())
        })?;

        match reader.read_frame().await {
            Ok(Some(msg)) => {
                self.bytes_received.fetch_add(64, Ordering::Relaxed);
                Ok(Some(msg))
            }
            Ok(None) => {
                self.status = TransportStatus::Disconnected;
                info!("USB connection closed by peer (EOF) -> marked Disconnected");
                Ok(None)
            }
            Err(e) => {
                self.status = TransportStatus::Disconnected;
                warn!("USB read frame error -> marked Disconnected: {}", e);
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
        self.cleanup_tunnel();
        info!("USB transport closed cleanly");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{HeartbeatData, Message};
    use tokio::net::TcpListener;

    #[test]
    fn test_adb_devices_output_parser() {
        let sample_output = r#"
List of devices attached
b9b2c03f	device product:OnePlus13s model:PJZ110 device:OnePlus13s transport_id:1
emulator-5554	offline transport_id:2
192.168.1.50:5555	unauthorized transport_id:3
"#;

        let devices = AdbDeviceInfo::parse_adb_devices_output(sample_output);
        assert_eq!(devices.len(), 3);

        assert_eq!(devices[0].serial, "b9b2c03f");
        assert_eq!(devices[0].state, "device");
        assert_eq!(devices[0].product.as_deref(), Some("OnePlus13s"));
        assert_eq!(devices[0].model.as_deref(), Some("PJZ110"));

        assert_eq!(devices[1].serial, "emulator-5554");
        assert_eq!(devices[1].state, "offline");

        assert_eq!(devices[2].serial, "192.168.1.50:5555");
        assert_eq!(devices[2].state, "unauthorized");
    }

    #[tokio::test]
    async fn test_usb_transport_framing_and_lifecycle() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(addr).await.unwrap();
        let (server_stream, _) = listener.accept().await.unwrap();

        let config = UsbTransportConfig::new(addr.port(), addr.port());
        let mut client = UsbTransport::from_stream(client_stream, config.clone());
        let mut server = UsbTransport::from_stream(server_stream, config);

        assert_eq!(client.kind(), TransportKind::Usb);
        assert_eq!(client.status(), TransportStatus::Connected);
        assert!(client.is_connected());

        // Send Heartbeat
        let hb = Message::Heartbeat(HeartbeatData { sequence: 42 });
        client.send_frame(&hb).await.unwrap();
        assert!(client.bytes_sent() > 0);

        let received = server.receive_frame().await.unwrap().unwrap();
        match received {
            Message::Heartbeat(msg) => assert_eq!(msg.sequence, 42),
            _ => panic!("Expected Heartbeat message"),
        }

        // Close
        client.close().await.unwrap();
        assert_eq!(client.status(), TransportStatus::Disconnected);
        assert!(!client.is_connected());

        // Subsequent sends must fail
        assert!(client.send_frame(&hb).await.is_err());
    }

    #[tokio::test]
    #[ignore]
    async fn test_usb_transport_reconnect_retry_state_machine() {
        let mut config = UsbTransportConfig::new(9999, 9999);
        config.device_serial = Some("mock-nonexistent-serial".to_string());
        config.reconnect_interval = Duration::from_millis(10);
        config.handshake_timeout = Duration::from_millis(50);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(addr).await.unwrap();
        let (_server_stream, _) = listener.accept().await.unwrap();

        let mut transport = UsbTransport::from_stream(client_stream, config);
        transport.status = TransportStatus::Disconnected;
        assert!(!transport.is_connected());

        let local_hello = HelloData {
            device_id: Uuid::new_v4(),
            device_name: "TestPC".to_string(),
            protocol_version: 1,
        };

        // When adb devices are unavailable in unit test, reconnect fails gracefully after max attempts
        let res = transport.reconnect(2, &local_hello).await;
        assert!(res.is_err());
        assert_eq!(transport.status(), TransportStatus::Failed);
    }
}

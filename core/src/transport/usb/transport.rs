use async_trait::async_trait;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::TcpStream;

use super::adb::AdbDeviceInfo;
use crate::protocol::{
    encode_frame, encode_frame_parts, FrameReader, HelloData, Message,
};
use crate::transport::{
    Transport, TransportError, TransportKind, TransportReadHalf, TransportStatus,
    TransportWriteHalf,
};

/// Configuration parameters for establishing a USB transport over an ADB tunnel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsbTransportConfig {
    /// Specific device serial number, or `None` to auto-select the first verified device.
    pub device_serial: Option<String>,
    /// Local forwarded TCP port on Windows (default 9876).
    pub local_port: u16,
    /// Remote TCP port the Android receiver is listening on (default 9876).
    pub remote_port: u16,
    /// Maximum time to wait for initial Hello handshake before failing.
    pub handshake_timeout: Duration,
    /// Timeout for socket writes during data transmission.
    pub write_timeout: Duration,
    /// Reconnect polling interval (default 2 seconds per TRD §8).
    pub reconnect_interval: Duration,
}

impl Default for UsbTransportConfig {
    fn default() -> Self {
        Self {
            device_serial: None,
            local_port: 9876,
            remote_port: 9876,
            handshake_timeout: Duration::from_secs(5),
            write_timeout: Duration::from_secs(10),
            reconnect_interval: Duration::from_secs(2),
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

/// Bidirectional USB Transport implemented via ADB TCP port forwarding tunnel (§8).
pub struct UsbTransport {
    config: UsbTransportConfig,
    active_serial: Option<String>,
    peer_info: Option<HelloData>,
    reader: Option<FrameReader<ReadHalf<TcpStream>>>,
    writer: Option<WriteHalf<TcpStream>>,
    local_addr: Option<SocketAddr>,
    peer_addr: Option<SocketAddr>,
    pub(crate) status: TransportStatus,
    bytes_sent: Arc<AtomicU64>,
    bytes_received: Arc<AtomicU64>,
    cleaned_up: Arc<AtomicBool>,
}

impl UsbTransport {
    // Delegated static ADB helpers for backward compatibility
    pub fn get_adb_path() -> std::path::PathBuf {
        super::adb::get_adb_path()
    }

    pub fn run_adb_cmd(args: &[&str]) -> Result<std::process::Output, TransportError> {
        super::adb::run_adb_cmd(args)
    }

    pub fn list_adb_devices() -> Result<Vec<AdbDeviceInfo>, TransportError> {
        super::adb::list_adb_devices()
    }

    pub fn setup_adb_forward(serial: &str, local_port: u16, remote_port: u16) -> Result<(), TransportError> {
        super::adb::setup_adb_forward(serial, local_port, remote_port)
    }

    pub fn remove_adb_forward(serial: &str, local_port: u16) -> Result<(), TransportError> {
        super::adb::remove_adb_forward(serial, local_port)
    }

    pub fn setup_adb_reverse(serial: &str, remote_port: u16, local_port: u16) -> Result<(), TransportError> {
        super::adb::setup_adb_reverse(serial, remote_port, local_port)
    }

    pub fn remove_adb_reverse(serial: &str, remote_port: u16) -> Result<(), TransportError> {
        super::adb::remove_adb_reverse(serial, remote_port)
    }

    pub fn setup_receive_adb_tunnels(serial: &str) -> Result<(), TransportError> {
        super::adb::setup_receive_adb_tunnels(serial)
    }

    pub fn setup_default_adb_tunnels(serial: &str) -> Result<(), TransportError> {
        super::adb::setup_default_adb_tunnels(serial)
    }

    pub fn cleanup_all_default_adb_tunnels(serial: Option<&str>) {
        super::adb::cleanup_all_default_adb_tunnels(serial)
    }

    pub fn trigger_android_hotspot(serial: &str) -> Result<(), TransportError> {
        super::adb::trigger_android_hotspot(serial)
    }

    pub fn trigger_android_receive(serial: &str) -> Result<(), TransportError> {
        super::adb::trigger_android_receive(serial)
    }

    pub fn trigger_android_stop_receive(serial: &str) -> Result<(), TransportError> {
        super::adb::trigger_android_stop_receive(serial)
    }

    pub fn trigger_android_stop_hotspot(serial: &str) -> Result<(), TransportError> {
        super::adb::trigger_android_stop_hotspot(serial)
    }

    pub fn kill_adb_server() -> Result<(), TransportError> {
        super::adb::kill_adb_server()
    }

    pub fn reset_adb_server() -> Result<(), TransportError> {
        super::adb::reset_adb_server()
    }

    pub fn start_usb_tethering(serial: Option<&str>) -> Result<(), TransportError> {
        super::adb::start_usb_tethering(serial)
    }

    pub fn stop_usb_tethering(serial: Option<&str>) -> Result<(), TransportError> {
        super::adb::stop_usb_tethering(serial)
    }

    pub fn is_receiver_listening(serial: &str, port: u16) -> bool {
        super::adb::is_receiver_listening(serial, port)
    }

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
                device_id: uuid::Uuid::new_v4(),
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

    pub fn peer_info(&self) -> Option<&HelloData> {
        self.peer_info.as_ref()
    }

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

    /// Cleans up all ADB tunnel rules associated with this transport instance.
    pub fn cleanup_tunnel(&self) {
        if self.cleaned_up.swap(true, Ordering::SeqCst) {
            return;
        }

        if let Some(serial) = &self.active_serial {
            let _ = Self::remove_adb_forward(serial, self.config.local_port);
            let _ = Self::remove_adb_reverse(serial, self.config.remote_port);
            if self.config.local_port == 9876 {
                let _ = Self::remove_adb_forward(serial, 9875);
            }
        }
    }

    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr
    }

    pub fn peer_addr(&self) -> Option<SocketAddr> {
        self.peer_addr
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

        let (header, maybe_payload) = encode_frame_parts(msg)?;
        let payload = maybe_payload.unwrap_or(&[]);
        let frame_len = (header.len() + payload.len()) as u64;

        if let Err(e) = crate::transport::vectored::write_all_vectored(writer, &header, payload).await {
            self.status = TransportStatus::Disconnected;
            error!("USB socket write error -> marked Disconnected: {}", e);
            return Err(TransportError::Disconnected(format!(
                "USB socket write failed: {}",
                e
            )));
        }

        if !matches!(
            msg,
            Message::ChunkData(_) | Message::ChunkAck(_) | Message::BatchChunkAck(_)
        ) {
            if let Err(e) = writer.flush().await {
                self.status = TransportStatus::Disconnected;
                error!("USB socket flush error -> marked Disconnected: {}", e);
                return Err(TransportError::Disconnected(format!(
                    "USB socket flush failed: {}",
                    e
                )));
            }
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

        match reader.read_frame_with_length().await {
            Ok(Some((msg, frame_len))) => {
                self.bytes_received.fetch_add(frame_len as u64, Ordering::Relaxed);
                Ok(Some(msg))
            }
            Ok(None) => {
                self.status = TransportStatus::Disconnected;
                Ok(None)
            }
            Err(e) => {
                self.status = TransportStatus::Disconnected;
                Err(TransportError::from(e))
            }
        }
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.status = TransportStatus::Disconnected;
        if let Some(mut writer) = self.writer.take() {
            let _ = writer.shutdown().await;
        }
        self.cleanup_tunnel();
        Ok(())
    }

    fn split_boxed(
        mut self: Box<Self>,
    ) -> Result<(Box<dyn TransportWriteHalf>, Box<dyn TransportReadHalf>), TransportError> {
        let is_conn = Arc::new(AtomicBool::new(self.status == TransportStatus::Connected));
        let write_half = UsbWriteHalf {
            writer: self.writer.take(),
            bytes_sent: self.bytes_sent.clone(),
            status: is_conn.clone(),
            active_serial: self.active_serial.clone(),
            config: self.config.clone(),
            cleaned_up: self.cleaned_up.clone(),
        };
        let read_half = UsbReadHalf {
            reader: self.reader.take(),
            bytes_received: self.bytes_received.clone(),
            status: is_conn,
        };
        Ok((Box::new(write_half), Box::new(read_half)))
    }
}

/// Write half of a decoupled USB transport (§8, §9, §10).
pub struct UsbWriteHalf {
    writer: Option<WriteHalf<TcpStream>>,
    bytes_sent: Arc<AtomicU64>,
    status: Arc<AtomicBool>,
    active_serial: Option<String>,
    config: UsbTransportConfig,
    cleaned_up: Arc<AtomicBool>,
}

impl UsbWriteHalf {
    pub fn cleanup_tunnel(&self) {
        if self.cleaned_up.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Some(serial) = &self.active_serial {
            let _ = UsbTransport::remove_adb_forward(serial, self.config.local_port);
            let _ = UsbTransport::remove_adb_reverse(serial, self.config.remote_port);
        }
    }
}

impl Drop for UsbWriteHalf {
    fn drop(&mut self) {
        self.cleanup_tunnel();
    }
}

#[async_trait]
impl TransportWriteHalf for UsbWriteHalf {
    async fn send_frame(&mut self, msg: &Message) -> Result<(), TransportError> {
        if !self.status.load(Ordering::Relaxed) {
            return Err(TransportError::Disconnected(
                "Cannot send frame: USB write half is disconnected".into(),
            ));
        }

        let writer = self.writer.as_mut().ok_or_else(|| {
            TransportError::Disconnected("USB transport writer is unavailable".into())
        })?;

        let (header, maybe_payload) = encode_frame_parts(msg)?;
        let payload = maybe_payload.unwrap_or(&[]);
        let frame_len = (header.len() + payload.len()) as u64;

        if let Err(e) = crate::transport::vectored::write_all_vectored(writer, &header, payload).await {
            self.status.store(false, Ordering::Relaxed);
            error!("USB socket write error -> marked Disconnected: {}", e);
            return Err(TransportError::Disconnected(format!(
                "USB socket write failed: {}",
                e
            )));
        }

        if !matches!(
            msg,
            Message::ChunkData(_) | Message::ChunkAck(_) | Message::BatchChunkAck(_)
        ) {
            if let Err(e) = writer.flush().await {
                self.status.store(false, Ordering::Relaxed);
                error!("USB socket flush error -> marked Disconnected: {}", e);
                return Err(TransportError::Disconnected(format!(
                    "USB socket flush failed: {}",
                    e
                )));
            }
        }

        self.bytes_sent.fetch_add(frame_len, Ordering::Relaxed);
        Ok(())
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.status.store(false, Ordering::Relaxed);
        if let Some(mut writer) = self.writer.take() {
            let _ = writer.shutdown().await;
        }
        self.cleanup_tunnel();
        Ok(())
    }

    fn bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }
}

/// Read half of a decoupled USB transport (§8, §9, §10).
pub struct UsbReadHalf {
    reader: Option<FrameReader<ReadHalf<TcpStream>>>,
    bytes_received: Arc<AtomicU64>,
    status: Arc<AtomicBool>,
}

#[async_trait]
impl TransportReadHalf for UsbReadHalf {
    async fn receive_frame(&mut self) -> Result<Option<Message>, TransportError> {
        if !self.status.load(Ordering::Relaxed) {
            return Err(TransportError::Disconnected(
                "Cannot receive frame: USB read half is disconnected".into(),
            ));
        }

        let reader = self.reader.as_mut().ok_or_else(|| {
            TransportError::Disconnected("USB transport reader is unavailable".into())
        })?;

        match reader.read_frame_with_length().await {
            Ok(Some((msg, frame_len))) => {
                self.bytes_received.fetch_add(frame_len as u64, Ordering::Relaxed);
                Ok(Some(msg))
            }
            Ok(None) => {
                self.status.store(false, Ordering::Relaxed);
                Ok(None)
            }
            Err(e) => {
                self.status.store(false, Ordering::Relaxed);
                Err(TransportError::from(e))
            }
        }
    }

    fn bytes_received(&self) -> u64 {
        self.bytes_received.load(Ordering::Relaxed)
    }
}

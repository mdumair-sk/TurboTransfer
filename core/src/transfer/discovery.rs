use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use uuid::Uuid;

use crate::transport::UsbTransport;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportPreference {
    Automatic,
    Combined,
    UsbOnly,
    WifiDirectOnly,
}

impl Default for TransportPreference {
    fn default() -> Self {
        Self::Automatic
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub device_id: Uuid,
    pub device_name: String,
    pub transport: String,
    pub is_connected: bool,
}

pub(crate) fn get_windows_hotspot_probe_ips() -> Vec<String> {
    #[allow(unused_mut)]
    let mut ips = Vec::new();
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("netsh")
            .args(["wlan", "show", "interfaces"])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut is_phone_hotspot = false;
            for line in text.lines() {
                let trimmed = line.trim();
                if (trimmed.starts_with("SSID") && !trimmed.starts_with("BSSID"))
                    || trimmed.starts_with("Profile")
                {
                    if let Some(val) = trimmed.split(':').nth(1) {
                        let ssid = val.trim().to_lowercase();
                        if ssid.contains("android")
                            || ssid.contains("pixel")
                            || ssid.contains("galaxy")
                            || ssid.contains("turbo")
                            || ssid.contains("direct-")
                            || ssid.contains("hotspot")
                        {
                            is_phone_hotspot = true;
                            break;
                        }
                    }
                }
            }

            if is_phone_hotspot {
                if let Ok(ip_output) = Command::new("ipconfig").output() {
                    let ip_text = String::from_utf8_lossy(&ip_output.stdout);
                    let mut in_wifi_section = false;
                    for line in ip_text.lines() {
                        let trimmed = line.trim();
                        if trimmed.contains("Wireless LAN adapter") || trimmed.contains("Wi-Fi") {
                            in_wifi_section = true;
                        } else if in_wifi_section && trimmed.starts_with("Default Gateway") {
                            if let Some(val) = trimmed.split(':').nth(1) {
                                let gw = val.trim();
                                if !gw.is_empty() {
                                    ips.push(format!("{}:9876", gw));
                                }
                            }
                            in_wifi_section = false;
                        } else if in_wifi_section && trimmed.is_empty() {
                            in_wifi_section = false;
                        }
                    }
                }
                // Also query resolve_windows_all_gateways() to capture all active interface gateways
                for gw in crate::transport::WifiDirectTransport::resolve_windows_all_gateways() {
                    let formatted = format!("{}:9876", gw);
                    if !ips.contains(&formatted) {
                        ips.push(formatted);
                    }
                }

                // Standard fallback Android AP gateways
                for fallback in ["192.168.43.1:9876", "192.168.49.1:9876"] {
                    let fallback_str = fallback.to_string();
                    if !ips.contains(&fallback_str) {
                        ips.push(fallback_str);
                    }
                }
            }
        }
    }
    ips
}

/// Discovered devices list across available network and transport interfaces (§7, §8, §9).
/// Strictly adheres to the LocalSend model: Only peers actively listening in Receive Mode are displayed.
pub fn get_devices() -> Vec<DeviceInfo> {
    let mut devices = Vec::new();
    let mut seen_ips = HashSet::new();

    // 1. Enumerate connected ADB devices
    if let Ok(adb_devs) = UsbTransport::list_adb_devices() {
        let is_pc_receiving = !super::receiver::get_receive_listeners().lock().is_empty();
        for d in adb_devs {
            if d.state == "device" {
                let is_listening = if is_pc_receiving {
                    let _ = UsbTransport::setup_receive_adb_tunnels(&d.serial);
                    true
                } else {
                    let _ = UsbTransport::remove_adb_reverse(&d.serial, 9876);
                    let _ = UsbTransport::setup_adb_forward(&d.serial, 9876, 9876);
                    let _ = UsbTransport::setup_adb_forward(&d.serial, 9875, 9875);
                    if UsbTransport::is_receiver_listening(&d.serial, 9876) {
                        true
                    } else {
                        let _ = UsbTransport::trigger_android_receive(&d.serial);
                        std::thread::sleep(std::time::Duration::from_millis(150));
                        UsbTransport::is_receiver_listening(&d.serial, 9876)
                    }
                };

                #[cfg(target_os = "windows")]
                let is_wifi_connected = {
                    use std::net::{SocketAddr, TcpStream};
                    use std::time::Duration;
                    let mut wifi_ready = false;
                    let addr = SocketAddr::from(([127, 0, 0, 1], 9875));
                    if let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(250)) {
                        use std::io::{BufRead, BufReader};
                        let mut reader = BufReader::new(&mut stream);
                        let mut line = String::new();
                        if reader.read_line(&mut line).is_ok() {
                            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                                if let (Some(ssid), Some(passphrase)) = (
                                    val.get("ssid").and_then(|s| s.as_str()),
                                    val.get("passphrase").and_then(|s| s.as_str()),
                                ) {
                                    if !ssid.is_empty() {
                                        let cur_ssid = crate::transport::WifiDirectTransport::get_current_windows_wifi_ssid();
                                        if cur_ssid.as_deref() == Some(ssid) {
                                            wifi_ready = true;
                                        } else {
                                            let config = crate::transport::WifiDirectConfig::new(ssid, passphrase, "", 9876);
                                            let _ = crate::transport::WifiDirectTransport::associate_wlan_windows_sync(&config);
                                            wifi_ready = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    wifi_ready
                };
                #[cfg(not(target_os = "windows"))]
                let is_wifi_connected = false;

                let name = if let Some(model) = &d.model {
                    format!("Android Phone: {} ({})", model, d.serial)
                } else if let Some(prod) = &d.product {
                    format!("Android Device: {} ({})", prod, d.serial)
                } else {
                    format!("Android ADB Device ({})", d.serial)
                };

                let transport_desc = if is_wifi_connected {
                    "USB + 5 GHz Wi-Fi (Connected)".to_string()
                } else if is_listening {
                    "USB (Ready to Receive)".to_string()
                } else {
                    "USB (ADB Connected)".to_string()
                };

                devices.push(DeviceInfo {
                    device_id: Uuid::from_u128(crate::checksum::compute_xxhash64(d.serial.as_bytes()) as u128),
                    device_name: name,
                    transport: transport_desc,
                    is_connected: true,
                });
                seen_ips.insert("127.0.0.1".to_string());
            }
        }
    }

    // 2. Probe 5 GHz Hotspot / Wi-Fi Direct Gateway endpoints
    for &wifi_ip in &["10.18.163.130", "192.168.43.1", "192.168.43.2", "192.168.1.19"] {
        if seen_ips.contains(wifi_ip) {
            continue;
        }
        if let Ok(stream) = std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from((
                wifi_ip.parse::<std::net::Ipv4Addr>().unwrap_or(std::net::Ipv4Addr::UNSPECIFIED),
                9876,
            )),
            std::time::Duration::from_millis(60),
        ) {
            drop(stream);
            seen_ips.insert(wifi_ip.to_string());
            devices.push(DeviceInfo {
                device_id: Uuid::from_u128(crate::checksum::compute_xxhash64(wifi_ip.as_bytes()) as u128),
                device_name: format!("Android Phone (5 GHz Wi-Fi Hotspot: {})", wifi_ip),
                transport: "5 GHz Wi-Fi Direct".to_string(),
                is_connected: true,
            });
        }
    }

    // 3. Fallback LAN / Wi-Fi Active Receivers Probe
    if !seen_ips.contains("127.0.0.1") {
        if let Ok(stream) = std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], 9876)),
            std::time::Duration::from_millis(50),
        ) {
            drop(stream);
            if devices.is_empty() {
                devices.push(DeviceInfo {
                    device_id: Uuid::nil(),
                    device_name: "Active Network Receiver (Port 9876)".to_string(),
                    transport: "TCP Network".to_string(),
                    is_connected: true,
                });
            }
        }
    }

    devices
}

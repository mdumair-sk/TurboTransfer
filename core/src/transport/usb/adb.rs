use log::{debug, warn};
#[cfg(not(target_os = "android"))]
use log::info;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
#[cfg(not(target_os = "android"))]
use std::process::Command;
#[cfg(not(target_os = "android"))]
use std::time::Duration;

use crate::transport::TransportError;

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
            if trimmed.is_empty()
                || trimmed.starts_with("List of devices")
                || trimmed.starts_with('*')
            {
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

/// Resolves the most appropriate ADB binary path.
pub fn get_adb_path() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        PathBuf::from("adb")
    }

    #[cfg(not(target_os = "android"))]
    {
        // 1. Check C:\adb\adb.exe
        let c_adb = PathBuf::from(r"C:\adb\adb.exe");
        if c_adb.is_file() {
            return c_adb;
        }

        // 2. Check ANDROID_HOME / ANDROID_SDK_ROOT
        if let Ok(android_home) =
            std::env::var("ANDROID_HOME").or_else(|_| std::env::var("ANDROID_SDK_ROOT"))
        {
            let pt_adb = PathBuf::from(android_home)
                .join("platform-tools")
                .join(if cfg!(windows) { "adb.exe" } else { "adb" });
            if pt_adb.is_file() {
                return pt_adb;
            }
        }

        // 3. Check LOCALAPPDATA / WinGet platform-tools
        if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
            let winget_adb = PathBuf::from(local_appdata)
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
        PathBuf::from("adb")
    }
}

/// Executes an ADB command with detached stdio and no window to prevent deadlocks and hanging on Windows.
pub fn run_adb_cmd(args: &[&str]) -> Result<std::process::Output, TransportError> {
    #[cfg(target_os = "android")]
    {
        let _ = args;
        Err(TransportError::Other(
            "Host ADB CLI execution is unsupported on Android runtime".into(),
        ))
    }

    #[cfg(not(target_os = "android"))]
    {
        let adb_path = get_adb_path();
        let mut cmd = Command::new(&adb_path);
        cmd.args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        cmd.output().map_err(|e| {
            TransportError::Other(format!(
                "Failed to execute '{:?} {:?}': {}",
                adb_path, args, e
            ))
        })
    }
}

/// Lists connected ADB devices by running `adb devices -l`.
pub fn list_adb_devices() -> Result<Vec<AdbDeviceInfo>, TransportError> {
    #[cfg(target_os = "android")]
    {
        Ok(Vec::new())
    }

    #[cfg(not(target_os = "android"))]
    {
        let output = run_adb_cmd(&["devices", "-l"])?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        Ok(AdbDeviceInfo::parse_adb_devices_output(&stdout))
    }
}

/// Sets up ADB forward rule for a specific device.
pub fn setup_adb_forward(
    serial: &str,
    local_port: u16,
    remote_port: u16,
) -> Result<(), TransportError> {
    let output = run_adb_cmd(&[
        "-s",
        serial,
        "forward",
        &format!("tcp:{}", local_port),
        &format!("tcp:{}", remote_port),
    ])?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TransportError::Other(format!(
            "adb forward failed: {}",
            stderr
        )));
    }

    debug!(
        "Set up adb forward tcp:{} -> tcp:{} for {}",
        local_port, remote_port, serial
    );
    Ok(())
}

/// Removes ADB forward rule for a specific device.
pub fn remove_adb_forward(serial: &str, local_port: u16) -> Result<(), TransportError> {
    let _ = run_adb_cmd(&[
        "-s",
        serial,
        "forward",
        "--remove",
        &format!("tcp:{}", local_port),
    ]);
    debug!("Removed adb forward tcp:{} for {}", local_port, serial);
    Ok(())
}

/// Sets up ADB reverse rule so Android can connect to Windows host.
pub fn setup_adb_reverse(
    serial: &str,
    remote_port: u16,
    local_port: u16,
) -> Result<(), TransportError> {
    let output = run_adb_cmd(&[
        "-s",
        serial,
        "reverse",
        &format!("tcp:{}", remote_port),
        &format!("tcp:{}", local_port),
    ])?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("adb reverse returned non-zero status: {}", stderr);
    } else {
        debug!(
            "Set up adb reverse tcp:{} -> tcp:{} for {}",
            remote_port, local_port, serial
        );
    }
    Ok(())
}

/// Configures ADB reverse tunnel for receiving files from Android and forward tunnel for hotspot metadata.
pub fn setup_receive_adb_tunnels(serial: &str) -> Result<(), TransportError> {
    // Stop any stale receive listener on Android phone so port 9876 is free for ADB reverse
    let _ = trigger_android_stop_receive(serial);
    // Ensure no conflicting forward rule on 9876 exists
    let _ = remove_adb_forward(serial, 9876);
    let _ = setup_adb_reverse(serial, 9876, 9876);
    let _ = setup_adb_forward(serial, 9875, 9875);
    Ok(())
}

/// Configures all required ADB reverse and forward tunnels for data and direct hotspot discovery.
pub fn setup_default_adb_tunnels(serial: &str) -> Result<(), TransportError> {
    setup_receive_adb_tunnels(serial)
}

/// Removes all default ADB forward and reverse tunnels for a device (or all devices if serial is None).
pub fn cleanup_all_default_adb_tunnels(serial: Option<&str>) {
    let serials: Vec<String> = if let Some(s) = serial {
        vec![s.to_string()]
    } else {
        list_adb_devices()
            .unwrap_or_default()
            .into_iter()
            .filter(|d| d.state == "device")
            .map(|d| d.serial)
            .collect()
    };

    for s in &serials {
        let _ = remove_adb_forward(s, 9876);
        let _ = remove_adb_forward(s, 9875);
        let _ = remove_adb_reverse(s, 9876);
        debug!("Cleaned up all default ADB tunnels for device {}", s);
    }
}

fn trigger_android_action(serial: &str, action: &str) -> Result<(), TransportError> {
    let _ = run_adb_cmd(&[
        "-s", serial, "shell", "am", "start",
        "-n", "com.turbotransfer/.MainActivity",
        "-a", action,
    ]);
    let _ = run_adb_cmd(&[
        "-s", serial, "shell", "am", "broadcast",
        "-a", action, "-p", "com.turbotransfer",
        "--receiver-include-background",
    ]);
    debug!("Triggered {} on device {}", action, serial);
    Ok(())
}

/// Triggers the Android app to spin up its 5 GHz Local-Only Hotspot via ADB broadcast.
pub fn trigger_android_hotspot(serial: &str) -> Result<(), TransportError> {
    trigger_android_action(serial, "com.turbotransfer.START_HOTSPOT")
}

/// Triggers the Android app to enter Receive mode via ADB broadcast and foreground launch.
pub fn trigger_android_receive(serial: &str) -> Result<(), TransportError> {
    trigger_android_action(serial, "com.turbotransfer.ENTER_RECEIVE")
}
/// Triggers the Android app to stop Receive mode via ADB broadcast so port 9876 is released.
pub fn trigger_android_stop_receive(serial: &str) -> Result<(), TransportError> {
    let _ = run_adb_cmd(&[
        "-s",
        serial,
        "shell",
        "am",
        "broadcast",
        "-a",
        "com.turbotransfer.STOP_RECEIVE",
    ]);
    debug!("Triggered STOP_RECEIVE broadcast on device {}", serial);
    Ok(())
}

/// Triggers the Android app to stop hotspot via ADB broadcast.
pub fn trigger_android_stop_hotspot(serial: &str) -> Result<(), TransportError> {
    let _ = run_adb_cmd(&[
        "-s",
        serial,
        "shell",
        "am",
        "broadcast",
        "-a",
        "com.turbotransfer.STOP_HOTSPOT",
    ]);
    debug!("Triggered STOP_HOTSPOT broadcast on device {}", serial);
    Ok(())
}

/// Removes ADB reverse rule for a specific device.
pub fn remove_adb_reverse(serial: &str, remote_port: u16) -> Result<(), TransportError> {
    let _ = run_adb_cmd(&[
        "-s",
        serial,
        "reverse",
        "--remove",
        &format!("tcp:{}", remote_port),
    ]);
    debug!("Removed adb reverse tcp:{} for {}", remote_port, serial);
    Ok(())
}

/// Kills any running ADB server process.
pub fn kill_adb_server() -> Result<(), TransportError> {
    #[cfg(target_os = "android")]
    {
        Ok(())
    }

    #[cfg(not(target_os = "android"))]
    {
        let adb_path = get_adb_path();
        let _ = Command::new(&adb_path).arg("kill-server").output();
        info!("Executed 'adb kill-server'");
        Ok(())
    }
}

/// Resets the ADB server (kills existing instance, starts fresh server).
pub fn reset_adb_server() -> Result<(), TransportError> {
    #[cfg(target_os = "android")]
    {
        Ok(())
    }

    #[cfg(not(target_os = "android"))]
    {
        let adb_path = get_adb_path();
        let _ = Command::new(&adb_path).arg("kill-server").output();
        std::thread::sleep(Duration::from_millis(200));
        let _ = Command::new(&adb_path).arg("start-server").output();
        info!("Executed ADB server reset (kill-server -> start-server)");
        Ok(())
    }
}

/// Starts USB RNDIS tethering on connected Android device via ADB.
pub fn start_usb_tethering(serial: Option<&str>) -> Result<(), TransportError> {
    #[cfg(target_os = "android")]
    {
        let _ = serial;
        Ok(())
    }

    #[cfg(not(target_os = "android"))]
    {
        let adb_path = get_adb_path();
        let mut cmd1 = Command::new(&adb_path);
        if let Some(s) = serial {
            cmd1.args(["-s", s]);
        }
        cmd1.args([
            "shell",
            "cmd",
            "connectivity",
            "tether",
            "start-tethering",
            "usb",
        ]);
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
}

/// Stops USB tethering on connected Android device and returns USB mode to MTP.
pub fn stop_usb_tethering(serial: Option<&str>) -> Result<(), TransportError> {
    #[cfg(target_os = "android")]
    {
        let _ = serial;
        Ok(())
    }

    #[cfg(not(target_os = "android"))]
    {
        let adb_path = get_adb_path();
        let mut cmd1 = Command::new(&adb_path);
        if let Some(s) = serial {
            cmd1.args(["-s", s]);
        }
        cmd1.args([
            "shell",
            "cmd",
            "connectivity",
            "tether",
            "stop-tethering",
            "usb",
        ]);
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
}

/// Probes if the target ADB device is actively running a TurboTransfer receiver on the specified port.
pub fn is_receiver_listening(serial: &str, port: u16) -> bool {
    #[cfg(target_os = "android")]
    {
        let _ = (serial, port);
        false
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = setup_adb_forward(serial, port, port);
        match std::net::TcpStream::connect_timeout(
            &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            Duration::from_millis(150),
        ) {
            Ok(stream) => {
                let _ = stream.shutdown(std::net::Shutdown::Both);
                true
            }
            Err(_) => false,
        }
    }
}

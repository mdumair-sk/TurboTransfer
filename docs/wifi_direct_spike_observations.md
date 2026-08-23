# Wi-Fi Direct Transport Spike Observations & Architecture Decision

**Date:** 2026-08-22  
**Milestone:** 7a (Spike)  
**Test Environment:** Windows 11 Host (Qualcomm Atheros QCA61x4A 802.11ac) & OnePlus 13s / CPH2723 (Android 16, ARM64)

---

## 1. Executive Summary

TRD §9 flagged the Wi-Fi Direct Windows-side implementation for an empirical spike to determine whether Windows can connect to an Android-created peer-to-peer network as a standard legacy Wi-Fi client (Approach 1) or requires UWP `Windows.Devices.WiFiDirect` APIs (Approach 2).

### Final Verdict
* **`WifiP2pManager.createGroup()` (Wi-Fi Direct P2P Group):** **NO-GO for standard Win32.**
* **`WifiManager.startLocalOnlyHotspot()` (Local-Only Hotspot):** **GO (100% Proven & Recommended).**

Using Android's `startLocalOnlyHotspot()` achieves all requirements of TRD §9 with zero UWP dependencies, allowing the Windows client to remain a lightweight native Win32 binary.

---

## 2. Empirical Findings & Comparison

| Characteristic | `WifiP2pManager.createGroup()` | `WifiManager.startLocalOnlyHotspot()` |
|---|---|---|
| **Android API** | `android.net.wifi.p2p.WifiP2pManager` | `android.net.wifi.WifiManager` |
| **Wi-Fi Framing** | Wi-Fi Direct P2P Action frames & P2P IEs | Standard 802.11 Beacon frames (WPA2-PSK) |
| **Windows Win32 Discovery** | ❌ Fails (`RSSI: 255`, Network not available) | ✅ Instant discovery (< 1.5s association) |
| **Windows UWP Needed?** | Yes (`Windows.Devices.WiFiDirect`) | ❌ No (Standard Win32 / `netsh` / `WlanConnect`) |
| **Cellular Data Sharing** | None | None (Strictly local peer-to-peer AP) |
| **TCP Socket Reliability** | Inaccessible via Win32 | 100% (10/10 packets, avg RTT ~9.6ms) |

---

## 3. Why `WifiP2pManager` Failed with Win32

When Android creates a P2P Autonomous Group Owner:
1. Android's Wi-Fi HAL advertises the SSID (`DIRECT-xx-...`) using Wi-Fi Direct Information Elements (P2P IEs).
2. Standard 802.11 station scans on Windows (used by `netsh wlan connect` and Win32 WLAN APIs) do not transmit P2P discovery probe requests.
3. Windows event logs consistently record:
   ```text
   Event ID: 8002 (WLAN AutoConfig)
   Profile Name: DIRECT-TurboTransfer
   Failure Reason: The specific network is not available.
   RSSI: 255
   ```
4. Joining an Android P2P GO directly is only possible if Windows implements full Wi-Fi Direct protocol negotiation via UWP `Windows.Devices.WiFiDirect`.

---

## 4. Why `startLocalOnlyHotspot()` is the Winning Solution

Android's `WifiManager.startLocalOnlyHotspot()` was introduced specifically for local, high-speed device-to-device transfers:
* **Standard 802.11 Access Point**: Broadcasts standard WPA2 beacons that any OS Wi-Fi client can discover and join.
* **Isolated Local Link**: Operates without enabling cellular data sharing or routing internet traffic.
* **Non-Admin Windows Association**: Windows joins seamlessly using a standard temporary WLAN profile via `netsh` or Win32 `WlanConnect` without requiring Administrator elevation.

---

## 5. Live Test Benchmarks & Network Characteristics

### Wi-Fi Band Configuration (5 GHz vs 2.4 GHz)
* **Default Behavior**: AOSP defaults `startLocalOnlyHotspot()` to 2.4 GHz (802.11n) for legacy client compatibility.
* **5 GHz Configuration**: On Android 11+ (API 30+ / Android 16), passing a `SoftApConfiguration` with `setBand(SoftApConfiguration.BAND_5GHZ)` forces the AP onto the 5 GHz band (802.11ac).
* **Link Speed**: Qualcomm Atheros 802.11ac 2x2 MIMO Wi-Fi establishes an **866.7 Mbps** link rate.

### Throughput Benchmarks (Real TCP Payload Transfer)

| Payload Size | Wi-Fi Band | Elapsed Time | Transfer Speed (MB/s) | Transfer Speed (Mbps) |
|---|---|---|---|---|
| **50 MB** | 2.4 GHz (802.11n) | 7.38 s | **6.78 MB/s** | 56.9 Mbps |
| **100 MB** | **5 GHz (802.11ac)** | 2.74 s | **36.46 MB/s** | 305.8 Mbps |
| **250 MB** | **5 GHz (802.11ac)** | 5.75 s | **43.46 MB/s** | 364.6 Mbps (Peak) |
| **500 MB (0.5 GB)** | **5 GHz (802.11ac)** | 13.07 s | **38.26 MB/s** | 321.0 Mbps (Sustained) |

* **5 GHz Speedup**: **~6.4x faster** than 2.4 GHz.
* **Ping Latency over 5 GHz**: **5.6 ms - 9.6 ms** avg roundtrip.
* **Packet Loss / Errors**: **0%** over hundreds of megabytes streamed.

### Gateway IP Resolution
Android acts as the Default Gateway for the Wi-Fi interface. The Windows client can resolve the Android target IP by:
1. Reading the control-plane handshake packet sent over ADB/USB.
2. Querying the default gateway of the associated Wi-Fi adapter (`(Get-NetIPConfiguration).IPv4DefaultGateway.NextHop`).

---

## 6. Single-Adapter Internet Behavior

If the Windows PC has only one Wi-Fi adapter and is not connected to Ethernet or USB tethering:
* Switching the Wi-Fi adapter from the home router to the Android local AP drops external internet access for the duration of the transfer.
* **Mitigation**: TurboTransfer operates over multipath. The USB/ADB tunnel remains active continuously; the Wi-Fi link is engaged on-demand for heavy chunk transfer and cleanly released upon transfer completion or idle timeout.

---

## 7. Architecture Recommendation for Milestone 7b

1. **Android Side**:
   * Use `WifiManager.startLocalOnlyHotspot()` to start the AP on-demand.
   * Expose generated SSID, passphrase, and gateway IP over the existing ADB control plane.
   * Listen on TCP port for incoming chunk data streams.
2. **Windows Side**:
   * Retain pure Rust Win32 implementation (no UWP / C++/WinRT bridge).
   * Implement `WifiDirectTransport` by generating a temporary WLAN profile and associating via Win32 WLAN APIs (`WlanConnect` / `netsh`).
   * Connect TCP sockets directly to the Android gateway IP.

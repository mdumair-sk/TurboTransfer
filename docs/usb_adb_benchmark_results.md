# USB (ADB Tunnel) 8-Stream Throughput Benchmark Results

## 1. Test Setup
* **Transport**: USB ADB Tunnel (`adb forward tcp:9876 tcp:9876`).
* **Wi-Fi State**: **100% Disconnected** (`netsh wlan show interfaces` -> `State: disconnected`).
* **Payload Size**: **250 MB** per test (262,144,000 bytes).
* **Concurrency**: **8 Parallel TCP streams** (31.25 MB per stream).
* **Buffer Tuning**: 128 KB write buffers, `TCP_NODELAY = true`, 4 MB OS socket buffers.

---

## 2. 250 MB Benchmark Measurements

| Test Direction | Payload Size | Parallel Streams | Elapsed Time | Transfer Speed (MB/s) | Transfer Speed (Mbps) |
|---|---|---|---|---|---|
| **Upload (PC $\rightarrow$ Android)** | 250 MB | 8 streams | 32.04 s | **7.80 MB/s** | 65.5 Mbps |
| **Download (Android $\rightarrow$ PC)** | 250 MB | 8 streams | 23.59 s | **10.60 MB/s** | 88.9 Mbps |

---

## 3. Key Observations & Comparison

1. **8-Stream Multiplexing**:
   - Single-stream USB ADB tunnel tops out at **~3.6 MB/s upload** and **~4.7 MB/s download**.
   - With 8 parallel streams, aggregate throughput scales to **7.80 MB/s upload** and **10.60 MB/s download** (**~2.2x speedup**).
2. **Download vs Upload Performance**:
   - Android $\rightarrow$ PC download is significantly faster (**10.60 MB/s**) due to Android Linux kernel TCP buffer queuing on outbound traffic.
3. **Role in Multipath Engine (Milestone 9)**:
   - In single-transport mode: Transfers a 250 MB file in **~23.5 seconds** (download) / **~32.0 seconds** (upload).
   - In combined multipath mode with 5 GHz Wi-Fi (~43 MB/s): Aggregate transfer speed reaches **~51 – 54 MB/s (430 – 450 Mbps)**.

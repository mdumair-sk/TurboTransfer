# TurboTransfer Logging Architecture & Strategy

To ensure high observability and rapid debugging across multipath operations, TurboTransfer implements structured, leveled logging for all meaningful events in the system.

---

## 1. Logging Levels & Meaningful Events

| Log Level | Purpose & Included Events |
|---|---|
| **`ERROR`** | Unrecoverable failures, checksum corruptions, unexpected socket drops, `meta.json` write errors, failed handshake transitions. |
| **`WARN`** | Transport disconnects/reconnects, chunk retry attempts (`ChunkNack`), frame decoding retries, high error rate on a transport path. |
| **`INFO`** | Session lifecycle: transfer initiated, handshake success, transfer state change (Pause/Resume/Cancel/Complete), device discovery, transport connected/disconnected. |
| **`DEBUG`** | Chunk queue assignments, adaptive scheduler bandwidth reweighting, per-chunk ack receipt, frame header encoding/decoding details. |
| **`TRACE`** | Raw socket bytes sent/received, low-level buffer pool allocations. |

---

## 2. Platform Log Targets & File Persistence

### **Windows Host (TUI & CLI)**
- **Console Log Output**: Configurable via `RUST_LOG` environment variable (default: `info`).
- **File Persistence**: Persisted automatically to standard app data directory:
  - Windows: `%APPDATA%\turbotransfer\logs\turbo.log` (with log file rotation).
- **TUI Dedicated Debug Screen**: A dedicated **Log View Screen** in `turbo-tui` displaying real-time scrolling logs filtered by level (`INFO`, `WARN`, `ERROR`, `DEBUG`).

### **Android App**
- **Logcat Integration**: Rust `log` macros seamlessly routed to Android Native `Logcat` (`android_logger` crate) under log tag `TurboTransferCore`.
- **App Storage Persistence**: Saved to app-private cache directory:
  - `/data/data/com.turbotransfer/cache/logs/turbo.log`

---

## 3. Dedicated Log Viewing in the App

1. **TUI View Screen**: Press `L` or navigate to **Transfer Details -> Debug Logs** in `turbo-tui` to view live filtered events.
2. **CLI Debug Mode**: Run `turbo --debug` or `RUST_LOG=debug turbo send <file>`.
3. **Log File Export**: API call `export_logs()` available via `Transfer API` to bundle logs for troubleshooting.

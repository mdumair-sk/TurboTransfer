# TurboTransfer — Benchmark & Calibration: Implementation Plan

Audience: coding agent implementing this directly in the repo. Written against current `main`. All file paths, function names, and constants below are verified against the actual codebase.

## 0. Decisions locked in

- **Two features**:
  - **Benchmark**: Real speed test over an ephemeral payload. Single-direction push from whichever device initiates it (run on Phone to benchmark Phone $\to$ PC; run on PC to benchmark PC $\to$ Phone).
  - **Calibration**: High-impact parameter search (Wi-Fi streams $\to$ chunk size $\to$ window preset) for outgoing transfers, tuning the device pair's baseline settings.
- **Default payload size**: **250 MB** per test transfer.
  - *Rationale*: At 50–100 MB/s, 250 MB transfers in 2.5–5 seconds. This is long enough for TCP Slow Start to finish and for the pipeline to operate in sustained steady-state saturation ($\approx$ 85%+ of the test is pure steady-state goodput), accurately mimicking real-world transfers of large files without wasting battery or storage.
- **Calibration search space & budget**:
  - Wi-Fi stream count: `{2, 3, 4}` (3 runs)
  - Chunk size: `{1 MiB, 2 MiB, 4 MiB}` (3 runs)
  - Wi-Fi window preset: `{Balanced, Aggressive, Max}` (3 runs)
  - Confirmation run: 1 run at winning config
  - **Total**: 10 runs $\times$ 250 MB = **2.5 GB total data transferred**, completing in **$\approx$ 40–50 seconds**.
  - *Thermal & RF Safety*: Completing in under 50 seconds avoids thermal throttling on mobile SoCs (meaning candidate #9 runs under the same hardware conditions as candidate #1) and finishes within a stationary Wi-Fi channel window.
  - *Why keep Max*: On top-tier links (e.g. Snapdragon 8 Elite + Wi-Fi 6/6E/7 with high Bandwidth-Delay Product), `Aggressive` (max 48) might throttle throughput where `Max` (max 64) allows the pipeline to hit line rate. Testing `Max` adds only 1 run (+250 MB, ~4s).
- **Architecture**:
  - Pure **push model**: the device starting the benchmark or calibration acts as the sender, pushing to the peer's existing `enter_receive_mode` listener. No complex bidirectional RPC or reverse-trigger command protocols required.
  - All core logic lives in `core/`. TUI/CLI and Android call the same UniFFI / Core Transfer API — no duplicated transfer or calibration algorithms in Kotlin.

---

## 1. What already exists — reuse this, don't rebuild it

- `core/src/transfer/api.rs`:
  - `BenchmarkResult` struct (thin, will expand with real telemetry fields).
  - `run_benchmark()` (currently an honest stub returning `Err("Live benchmarks are not implemented yet")`).
- `tui/src/ui/benchmark.rs` + `benchmark_results.rs`: Real UI shell (transport picker, payload-size picker). Needs the real backend wired, replacing the synthetic mock in `tui/src/app.rs`.
- `core/src/transfer/session.rs`: `send_file_session_multipath` / receiver path — the actual multipath transfer engine. Benchmark and calibration both invoke this directly.
- `core/src/scheduler/window.rs`: `WindowController::with_thresholds(min, max, initial, backpressure_us, rtt_congestion_us)` already exists. `for_wifi()` uses `(12, 32, 16, 400_000.0, 1_500_000.0)` — this is the `Balanced` preset baseline.
- `core/src/manifest/actor.rs` (`MetaActor`), `find_resumable_transfer`, and persistent `meta.json`: All are **bypassed** for benchmark/calibration transfers (detailed in §4).
- `core/src/protocol/messages.rs`: Existing wire-compatibility pattern (`ChunkAckData` explicit payload-length check) — follow this exact pattern for the new `TransferOfferData` field.

---

## 2. New data types (`core/src/benchmark/types.rs`, new file)

```rust
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransferPurpose {
    Normal,
    Benchmark,
    Calibration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowPreset {
    Conservative,
    Balanced,   // Matches current WindowController::for_wifi() defaults
    Aggressive,
    Max,
}

impl WindowPreset {
    /// Returns (min_window, max_window, initial_window, socket_backpressure_threshold_us, rtt_congestion_threshold_us)
    pub fn to_thresholds(self) -> (usize, usize, usize, f64, f64) {
        match self {
            Self::Conservative => (8, 24, 12, 300_000.0, 1_200_000.0),
            Self::Balanced     => (12, 32, 16, 400_000.0, 1_500_000.0), // current for_wifi()
            Self::Aggressive   => (16, 48, 24, 550_000.0, 2_000_000.0),
            Self::Max          => (24, 64, 32, 700_000.0, 2_500_000.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferConfigOverride {
    pub wifi_stream_count: Option<usize>,         // None = default (3)
    pub chunk_size_bytes: Option<u32>,            // None = select_optimal_chunk_size()
    pub wifi_window_preset: Option<WindowPreset>, // None = WindowPreset::Balanced
}

impl Default for TransferConfigOverride {
    fn default() -> Self {
        Self {
            wifi_stream_count: None,
            chunk_size_bytes: None,
            wifi_window_preset: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub target_device_id: Uuid,
    pub avg_speed_mbps: f64,
    pub peak_speed_mbps: f64,
    pub usb_avg_mbps: f64,
    pub wifi_avg_mbps: f64,
    pub duration_ms: u64,
    pub bytes_transferred: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationCandidateResult {
    pub config: TransferConfigOverride,
    pub avg_speed_mbps: f64,
    pub duration_ms: u64,
    pub sweep_stage: String, // "streams" | "chunk_size" | "window" | "confirmation"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationResult {
    pub target_device_id: Uuid,
    pub best_config: TransferConfigOverride,
    pub best_speed_mbps: f64,
    pub all_candidates: Vec<CalibrationCandidateResult>,
    pub total_duration_ms: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedCalibrationConfig {
    pub device_pair_id: String,
    pub config: TransferConfigOverride,
    pub expected_speed_mbps: f64,
    pub calibrated_at: DateTime<Utc>,
}
```

---

## 3. Protocol change & Wire compatibility

### `TransferOfferData` modification (`core/src/protocol/messages.rs`):
```rust
pub struct TransferOfferData {
    pub transfer_id: Uuid,
    pub file_id: Uuid,
    pub file_name: String,
    pub file_size: u64,
    pub chunk_size: u32,
    pub total_chunks: u32,
    pub checksum_algo: String,
    pub purpose: TransferPurpose, // Appended as the last field
}
```

### Wire compatibility in `decode_payload` (`MSG_TYPE_TRANSFER_OFFER`):
Do **not** rely on `#[serde(default)]` because `bincode` is positional binary framing. Follow the established `ChunkAckData` compatibility pattern:
```rust
MSG_TYPE_TRANSFER_OFFER => {
    // If payload matches pre-purpose serialized length or lacks trailing enum byte,
    // fallback gracefully to TransferPurpose::Normal.
    match bincode::deserialize::<TransferOfferData>(payload) {
        Ok(data) => Message::TransferOffer(data),
        Err(_) => {
            // Deserialize legacy struct without purpose field
            #[derive(Deserialize)]
            struct LegacyTransferOfferData {
                transfer_id: Uuid,
                file_id: Uuid,
                file_name: String,
                file_size: u64,
                chunk_size: u32,
                total_chunks: u32,
                checksum_algo: String,
            }
            let legacy: LegacyTransferOfferData = bincode::deserialize(payload)
                .map_err(|e| ProtocolError::DeserializationError(e.to_string()))?;
            Message::TransferOffer(TransferOfferData {
                transfer_id: legacy.transfer_id,
                file_id: legacy.file_id,
                file_name: legacy.file_name,
                file_size: legacy.file_size,
                chunk_size: legacy.chunk_size,
                total_chunks: legacy.total_chunks,
                checksum_algo: legacy.checksum_algo,
                purpose: TransferPurpose::Normal,
            })
        }
    }
}
```

### Receiver behavior on `purpose`:
- `Normal`: Unchanged production behavior.
- `Benchmark` / `Calibration`:
  1. Skip `find_resumable_transfer` (always treated as a fresh ephemeral session).
  2. Skip `MetaActor::spawn` (no disk I/O for `.meta.json`).
  3. Route file write to OS temp/cache directory (e.g. `context.cacheDir` on Android, `%TEMP%\TurboTransfer` on Windows).
  4. Auto-delete the received temp file as soon as the session completes or aborts.

---

## 4. No-cache guarantees & Ephemeral file generation

To prevent OS page cache or Android buffer cache from skewing measurements across runs:

1. **Fast PRNG Ephemeral Generator** (`core/src/benchmark/ephemeral_file.rs`):
   - Fast, non-cryptographic generator (`xorshift64`) seeded with system time + transfer UUID.
   - Generates non-compressible binary data.
   - Streamed in chunks (1–4 MiB write buffer) to prevent allocating 250 MB in RAM.
   - Encapsulated in an RAII guard:
     ```rust
     pub struct EphemeralFile {
         pub path: PathBuf,
     }
     impl Drop for EphemeralFile {
         fn drop(&mut self) {
             let _ = std::fs::remove_file(&self.path);
         }
     }
     ```
2. **Fresh UUID per run**: Filenames are formatted as `bench_{uuid_v4}.bin`. Combined with freshly generated `transfer_id`, this prevents collision with active session maps.

---

## 5. Feature A: Benchmark mode

Executes a single real 250 MB push transfer to the peer.

### `run_benchmark(peer_device_id, transport_pref, size_mb) -> BenchmarkResult`:
1. Default payload size = **250 MB** (allow UI cycling: 100, 250, 500, 1000 MB).
2. Generate 250 MB `EphemeralFile`.
3. Invoke `send_file_session_multipath` with `purpose = TransferPurpose::Benchmark` and standard production defaults.
4. Collect session telemetry (duration, avg throughput, peak throughput, USB vs. Wi-Fi split).
5. Ephemeral file is automatically deleted on sender; receiver cleans up its temp file.
6. Return `BenchmarkResult`.

### UI Integration:
- **TUI**: Replace the hardcoded mock throughput in `tui/src/app.rs` (`52.4`, `36.8`, `10.6 Mbps`) with the real `BenchmarkResult` from `core::transfer::api::run_benchmark`.
- **Android**: Add "Network Benchmark" button in Settings, executing `run_benchmark` via UniFFI and rendering the live progress and final result card.

---

## 6. Feature B: Calibration mode

Tuning the sender's outgoing configuration for this specific peer device pair.

### 6.1 Search space (9 runs total)
- **Stage 1: Wi-Fi Stream Count**
  - Test streams: `{2, 3, 4}` (3 runs)
  - Baseline chunk size = auto, window = `Balanced`
  - Winner: `best_streams`
- **Stage 2: Chunk Size**
  - Test chunks: `{1 MiB, 2 MiB, 4 MiB}` (3 runs)
  - Streams fixed at `best_streams`, window = `Balanced`
  - Winner: `best_chunk`
- **Stage 3: Window Preset**
  - Test window: `{Balanced, Aggressive, Max}` (3 runs)
  - Streams fixed at `best_streams`, chunk fixed at `best_chunk`
  - Winner: `best_window`
- **Stage 4: Confirmation**
  - 1 run at winning `{ best_streams, best_chunk, best_window }`
  - Final confirmed speed = average of Stage 3 winning speed & Confirmation speed.

**Total**: 10 runs $\times$ 250 MB = **2.5 GB transferred** ($\approx$ 40–50s wall-clock time).

### 6.2 Progress reporting
UniFFI / callback interface emitted after each candidate run:
```rust
pub struct CalibrationProgressUpdate {
    pub current_step: u32,             // 1 to 10
    pub total_steps: u32,               // 10
    pub stage: String,                  // "streams" | "chunk_size" | "window" | "confirmation"
    pub config_under_test: TransferConfigOverride,
    pub last_result_mbps: Option<f64>,
}
```
UI shows: *"Calibrating 4/10: Streams=3, Chunk=2MB — Last: 72.4 MB/s"*.

### 6.3 Configuration storage & consumption
- **Key**: `device_pair_id = stable_hash(sort([local_device_id, peer_device_id]))`.
- **Storage**:
  - Android: `DataStore` (or JSON in app internal files directory).
  - PC: `%APPDATA%\TurboTransfer\calibration\{pair_id}.json`.
- **Consumption in `start_transfer`**:
  - Check for existing `SavedCalibrationConfig`. If present, apply `wifi_stream_count`, `chunk_size_bytes`, and `wifi_window_preset` as defaults.
  - Runtime dynamic AIMD adaptation (`WindowController::evaluate_and_adjust`) remains active throughout transfers to handle live channel fluctuations.
- **Reset**: Expose `clear_saved_calibration(peer_device_id)` via UI button ("Reset to System Defaults").

---

## 7. Module layout

```
core/src/benchmark/
  mod.rs              -- pub exports (run_benchmark, run_calibration, etc.)
  types.rs            -- Structs, enums, window presets
  ephemeral_file.rs    -- Fast xorshift64 PRNG file generator + RAII cleanup
  runner.rs           -- Single-run executor with TransferPurpose & config override
  calibration.rs       -- 9-run coordinate descent search & progress reporting
  config_store.rs      -- Persistence (load/save/clear SavedCalibrationConfig)
```

---

## 8. UniFFI API surface

```rust
pub fn run_benchmark(
    target_device_id: String,
    transport_pref: TransportPreference,
    size_mb: u32, // Default 250
) -> Result<BenchmarkResult, FfiTransferException>;

pub fn run_calibration(
    target_device_id: String,
    progress_callback: Box<dyn CalibrationProgressCallback>,
) -> Result<CalibrationResult, FfiTransferException>;

pub fn cancel_calibration(target_device_id: String);
pub fn get_saved_calibration(target_device_id: String) -> Option<SavedCalibrationConfig>;
pub fn clear_saved_calibration(target_device_id: String);
```

---

## 9. Implementation phases

1. **Protocol & Bypass Plumbing**:
   - Add `TransferPurpose` to `TransferOfferData`.
   - Update `decode_payload` with backward-compatible legacy fallback.
   - Implement `EphemeralFile` generator in `core/src/benchmark/ephemeral_file.rs`.
   - Update receiver session to bypass `MetaActor` / `.meta.json` and auto-delete temp files when `purpose != Normal`.
2. **Core Benchmark**:
   - Implement `run_benchmark` in `core/src/benchmark/runner.rs`.
   - Wire real telemetry into `BenchmarkResult`.
   - Connect TUI `benchmark.rs` to real `run_benchmark`, removing the synthetic mock in `tui/src/app.rs`.
3. **Core Calibration**:
   - Implement `TransferConfigOverride` thread-through for `DEFAULT_WIFI_PARALLEL_STREAMS`, chunk sizing, and `WindowController`.
   - Implement 10-run calibration search in `core/src/benchmark/calibration.rs` with progress callbacks and cancellation token.
   - Implement `config_store.rs` and hook `start_transfer` to consult saved configs.
4. **Platform Surfaces**:
   - Expose new methods via `uniffi_interface.rs`.
   - Build Android "Network Performance" settings section (Benchmark & Calibration cards).
   - Build TUI Calibration dashboard with live progress updates.
5. **Verification & Clean Build**:
   - Build and test on Snapdragon 8 Elite via `tools/phone-builder.ps1`.
   - Validate clean cleanup of ephemeral files on cancellation or drop.

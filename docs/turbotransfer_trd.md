# TurboTransfer — Technical Requirements Document

**Version:** 1.0
**Status:** Draft for implementation
**Supersedes:** FluxSync (this project replaces it)

---

## 1. Summary

TurboTransfer is a Rust-core, multipath file-transfer system between Android and Windows, using USB (ADB tunnel) and true Wi-Fi Direct simultaneously per transfer. Primary interface is a TUI (`turbo`); a scriptable CLI shares the same core. Architecture: stateful control plane (transfer/device/resume state) + stateless data plane (independently verifiable chunks).

---

## 2. Core Architecture Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Core language | Rust | Single native core shared across Android, Windows, TUI, CLI. Max performance, no logic duplication. |
| Android integration | Rust core compiled as a `cdylib`, exposed via **UniFFI** to Kotlin | UniFFI generates safe Kotlin bindings from Rust, avoids hand-written JNI boilerplate, keeps Android UI thin (Compose only calls into core). |
| Windows integration | Rust core as a native lib, TUI/CLI built directly in Rust (ratatui for TUI) | Same binary family as core; no FFI boundary needed on Windows. |
| USB transport | **ADB tunnel** (`adb reverse`/`forward` + raw TCP socket over the tunnel) | AOA was already tried and missed throughput targets. ADB tunneling is proven (used successfully in FluxSync), gets near-USB-controller-limited throughput, and needs no custom USB descriptor/driver work. Requires ADB debugging enabled on Android. |
| Wi-Fi transport | **True Wi-Fi Direct** (Android as group owner via `WifiP2pManager`), forced regardless of existing LAN | Explicit requirement: no dependency on router/internet, forced P2P even when both devices share a Wi-Fi network. Windows joins as a legacy client over the P2P group's IP range. |
| Device pairing | **None for MVP** — any device discovered on the active transport (Wi-Fi Direct group / ADB-visible USB) is trusted automatically | Reduces MVP scope. Flagged as a security gap — see §11. |
| Concurrent `meta.json` writes | **Single-writer actor pattern**: one task owns all writes to `meta.json`; chunk-completion events from both transports are sent as messages to this actor over a channel, never written directly by transport threads | Same resolution as FluxSync. Avoids file-lock contention/corruption risk under multipath (two transports completing chunks concurrently), keeps write path simple and testable. |

---

## 3. MVP Scope & Development Order

Per required sequencing (doc §48), applied as binding milestones:

1. **Transfer protocol** — wire format for control messages + chunk headers (§6)
2. **Chunk engine** — chunking, checksum, manifest generation
3. **Stateless data path** — chunk send/receive/verify, no persistence
4. **Stateful control plane** — `meta.json` schema, resume state, single-writer actor
5. **Basic direct CLI** — `turbo send`/`turbo receive` over loopback TCP (no real transport yet) to validate 2–4
6. **TCP prototype** — real network transport, Android↔Windows, single path
7. **Wi-Fi Direct** — replace/extend TCP prototype with true P2P group formation
8. **USB** — ADB tunnel transport; **build as an isolated proof-of-concept first** (raw throughput test, Android↔Windows over `adb forward`, no chunking/control-plane integration) before wiring into the full engine
9. **Multipath scheduler** — combine USB + Wi-Fi Direct under one transfer
10. **Resume/retry**
11. **TUI** (ratatui)
12. **Performance optimization**

MVP is considered complete at the end of milestone 11 (functioning TUI, both transports, multipath, resume). Milestone 12 is post-MVP tuning.

**USB POC gate:** Before milestone 9, run the isolated ADB-tunnel POC (milestone 8) and confirm throughput exceeds the AOA baseline. If it doesn't clear a meaningful margin, re-open the USB transport decision before continuing.

---

## 4. Repository Structure

```
turbo/
├── core/                  # Rust, platform-agnostic
│   ├── transfer/          # session lifecycle, transfer API
│   ├── chunk/             # chunking, chunk struct, buffer pool
│   ├── scheduler/         # dynamic multipath scheduling
│   ├── manifest/          # meta.json schema, read/write actor
│   ├── protocol/          # wire format, message (de)serialization
│   └── checksum/          # CRC32C / xxHash64
├── transport/
│   ├── usb/                # ADB tunnel transport
│   └── wifi_direct/        # Wi-Fi Direct transport
├── tui/                    # ratatui, screens/widgets/navigation/input
├── cli/                     # command parsing, shares Transfer API
├── android/                 # Kotlin, UniFFI-generated bindings, Compose UI, WifiP2pManager glue
└── windows/                  # thin platform glue (ADB process invocation, network APIs)
```

Interface rule: TUI and CLI **only** call the Transfer API (§7). Neither implements transfer, transport, or scheduling logic. Android's Kotlin layer only calls UniFFI bindings — no transfer logic in Kotlin.

---

## 5. Data Model

### 5.1 Chunk (stateless, data plane)

```rust
struct Chunk {
    transfer_id: Uuid,
    file_id: Uuid,
    chunk_id: u32,        // sequence index, 0-based
    file_offset: u64,
    payload_length: u32,
    checksum: u64,        // xxHash64 of payload
    payload: Bytes,
}
```

- Self-describing: no chunk depends on any other chunk to be validated or written.
- Idempotent write: receiver checks `(transfer_id, file_id, chunk_id, checksum)` before writing; a duplicate valid chunk is a no-op ack.

### 5.2 `meta.json` (stateful, control plane)

One file per transfer, written only by the single-writer actor.

The schema is OS-agnostic, tracking only the local device's role and the peer's unique ID.

```json
{
  "transfer_id": "uuid",
  "file_id": "uuid",
  "file_name": "movie.mkv",
  "file_size": 13639045120,
  "chunk_size": 67108864,
  "total_chunks": 203,
  "role": "sender",
  "peer_device_id": "uuid",
  "status": "in_progress",
  "completed_ranges": [[0, 731], [733, 1042]],
  "created_at": "iso8601",
  "updated_at": "iso8601",
  "transport_stats": {
    "usb": { "bytes": 0, "errors": 0, "retries": 0 },
    "wifi_direct": { "bytes": 0, "errors": 0, "retries": 0 }
  }
}
```

`completed_ranges` is stored on disk as inclusive `[start, end]` chunk-id pairs (not a bitmap). This format is compact for large chunk counts, human-readable for debugging, and avoids large opaque blobs in JSON. The on-disk representation is always normalized (non-overlapping, non-adjacent, sorted) by the actor before flushing. Resume computes missing = complement of ranges over `[0, total_chunks)`.

### 5.3 Single-writer actor contract

- One Tokio task owns `meta.json` for a given transfer; started when the transfer session starts, stopped when it completes/is cancelled.
- All other tasks (transport receive loops, retry handler, pause/cancel handlers) communicate via an `mpsc` channel: `ChunkCompleted`, `ChunkFailed`, `TransportStatusChanged`, `Pause`, `Cancel`.
- Actor maintains an in-memory `HashSet<u32>` (or `RoaringBitmap` for extreme chunk counts) of completed chunk IDs for O(1) runtime membership checks. On each flush, this set is coalesced into a minimal sorted list of inclusive ranges (adjacent ranges merged) before serializing to `meta.json`.
- Actor batches writes (flush on N events or T ms, whichever first — default N=10, T=250ms) to bound disk I/O under high chunk-completion rates; on `Pause`/`Cancel`/process-exit-signal it flushes immediately and synchronously before returning.
- On restart, actor reads `meta.json` fresh; `completed_ranges` is expanded back into the in-memory set and serves as the sole source of resume truth.

---

## 6. Wire Protocol

### 6.1 Transport-agnostic framing

Every message (control or chunk) over any transport uses the same frame:

```
[4 bytes: message_length (u32 LE)]
[1 byte: message_type]
[N bytes: payload (bincode-encoded)]
```

`message_type`: `0x01 Hello`, `0x02 TransferOffer`, `0x03 TransferAccept`, `0x04 TransferReject`, `0x05 ChunkData`, `0x06 ChunkAck`, `0x07 ChunkNack`, `0x08 Pause`, `0x09 Resume`, `0x0A Cancel`, `0x0B Complete`, `0x0C Heartbeat`.

### 6.2 Handshake sequence

1. **Discovery** (transport-specific, §8/§9) → peer's transfer-service address/socket known.
2. Sender opens control channel, sends `Hello { device_id, device_name, protocol_version }`.
3. Receiver replies `Hello` (its own identity). No pairing check in MVP (§11).
4. Sender sends `TransferOffer { transfer_id, file_name, file_size, chunk_size, total_chunks, checksum_algo }`.
5. Receiver replies `TransferAccept { transfer_id, resume_from: Option<CompletedRanges> }` or `TransferReject { reason }`.
   - If `meta.json` for this `(transfer_id, file_id)` already exists on receiver, `resume_from` is populated and sender skips already-completed chunks.
6. Data plane begins: chunks streamed on one control channel per active transport (see §10 for multipath channel model).
7. Each `ChunkData` is answered with `ChunkAck { chunk_id }` or `ChunkNack { chunk_id, reason }` (checksum mismatch → retry via any available transport, see §10.3).
8. Sender sends `Complete` once all chunks acked; receiver verifies file-level checksum, renames `.part` file to final name, responds with final `ChunkAck`-equivalent completion confirmation.
9. `Heartbeat` sent every 5s on idle control channels to detect silent transport death (distinct from an explicit disconnect event).

### 6.3 Checksum

- Per-chunk: **xxHash64** (fast, non-cryptographic, sufficient given per-chunk verification + eventual file-level check).
- File-level: **CRC32C** over the full reconstructed file, verified after all chunks land, before `.part` → final rename.

---

## 7. Transfer API (Core ↔ Frontend boundary)

Exposed identically to TUI, CLI, and (via UniFFI) Android Kotlin:

```rust
fn start_transfer(file_path: PathBuf, device_id: Uuid, transport_pref: TransportPreference) -> TransferHandle;
fn pause_transfer(transfer_id: Uuid);
fn resume_transfer(transfer_id: Uuid, transport_pref: TransportPreference);
fn cancel_transfer(transfer_id: Uuid);
fn get_progress(transfer_id: Uuid) -> TransferProgress;
fn get_devices() -> Vec<DeviceInfo>;
fn get_transfers() -> Vec<TransferSummary>;  // current, resumable, completed
fn run_benchmark(device_id: Uuid, transport: TransportPreference) -> BenchmarkResult;
fn enter_receive_mode();
fn leave_receive_mode();
```

`TransportPreference`: `Automatic | Combined | UsbOnly | WifiDirectOnly`. Default `Automatic` (resolves to `Combined` when both transports available, else whichever is available).

`TransferProgress` includes: file name/size, bytes transferred, percent, per-transport throughput (instantaneous, smoothed over 2s window), aggregate throughput, ETA, chunk counts (total/completed), retry count, per-transport error count.

---

## 8. USB Transport (ADB Tunnel)

- Windows host runs `adb forward tcp:<local_port> tcp:<device_port>`; Android side runs a TCP listener bound to `<device_port>` (localhost only) started by the TurboTransfer Android service.
- Transport layer treats this as a plain TCP socket once the tunnel is up — same framing as §6.1.
- Discovery: Windows enumerates `adb devices`, filters to devices running the TurboTransfer Android service (verified by attempting the framed `Hello` handshake on the forwarded port).
- Failure detection: socket read/write error or `adb devices` no longer listing the device → transport marked `Disconnected`, in-flight chunks on this transport returned to pending queue (§10.3).
- Reconnect: background poll (every 2s) for `adb devices` re-listing the device → tunnel re-established, transport marked `Connected`, scheduler resumes assigning chunks to it.

**POC requirement (milestone 8):** before integrating into the engine, build a standalone benchmark: raw byte throughput over an `adb forward` tunnel, both directions, no chunking/checksums. Confirm this clears AOA's prior ceiling before committing further engineering time to full integration.

---

## 9. Wi-Fi Direct Transport

- Android: `WifiP2pManager`, device advertises as **group owner**, forced regardless of existing Wi-Fi/internet connectivity (do not defer to normal LAN even if both devices share a router).
- Windows: joins the Android-created P2P group as a legacy client. **Open item — Windows-side implementation needs a spike**: native Win32 Wi-Fi Direct support outside UWP is limited. Two candidate approaches to prototype in milestone 7:
  1. Windows connects to the P2P group's advertised SSID/passphrase as a normal Wi-Fi network (Android's group operates as a standard WPA2 AP under the hood) — avoids needing Windows-side Wi-Fi Direct APIs at all, just standard `netsh`/`WlanConnect` to join the SSID.
  2. UWP `Windows.Devices.WiFiDirect` APIs if the Rust/Windows binary can host a UWP component (adds packaging complexity for a CLI/TUI app).
  - **Recommendation: approach 1.** Treat Android's Wi-Fi Direct group as a regular AP from Windows' perspective — Windows just needs the SSID/PSK, which Android can transmit over the already-connected USB/ADB channel during discovery (or via a first-run pairing exchange, see §11), then join via standard OS network APIs. This sidesteps UWP entirely and keeps Windows a plain Win32 binary.
- Once Windows is on the P2P subnet, the same TCP framing (§6.1) is used over sockets on that network.
- Failure detection: TCP-level (dropped socket, no `Heartbeat` reply within 15s) → transport `Disconnected`, in-flight chunks requeued.
- Reconnect: Android continues advertising the group; Windows retries joining the known SSID every 3s.

---

## 10. Multipath Scheduling

### 10.1 Chunk assignment

- One shared pending-chunk queue (FIFO by `chunk_id`, but not required to complete in order).
- Each connected transport has a worker pulling from the queue when it has capacity (bounded in-flight chunks per transport, default 4).
- No static bandwidth split. Scheduler is **rate-adaptive**: track rolling average throughput per transport (2s window); when a transport finishes a chunk and has queue capacity, pull the next pending chunk immediately — faster transports naturally pull more often. This achieves dynamic proportional allocation without explicit ratio math.

### 10.2 Buffer management

- Bounded buffer pool (default: 8 buffers × chunk size, configurable). Never load full file into memory.
- Reader task (single, disk-bound) fills buffers on demand as transports free up capacity; writer task (single, disk-bound, owned by the same actor pattern as `meta.json` writes — see §5.3) drains completed chunks to the `.part` file in `file_offset` order... **but writes may occur out of order** (see 10.4).

### 10.3 Retry

- `ChunkNack` or transport-level failure mid-chunk → chunk returned to the pending queue, `retries` counter incremented in `meta.json` transport stats.
- Retry may be picked up by **any** currently-connected transport, not necessarily the one it failed on.
- No fixed retry cap for MVP (a stalled transfer is surfaced via `TransferProgress`/TUI, user can cancel); revisit if infinite-retry loops are observed in testing.

### 10.4 Out-of-order writes

- Receiver writes each verified chunk directly to its `file_offset` in the `.part` file (pre-allocated to full size on transfer start, sparse where supported) — no reordering buffer needed. This is why out-of-order chunk arrival (doc §49 test case) is safe by construction.

### 10.5 Transport loss mid-transfer

- Losing one transport: its in-flight (unacked) chunks return to pending queue, remaining transport(s) continue uninterrupted, no pause of the overall transfer.
- Losing all transports: transfer moves to `Paused` state automatically (not `Failed`); `meta.json` flushed synchronously; resumes automatically when any known transport reconnects, or manually via `resume_transfer`.

---

## 11. Security — MVP Gap (explicit, not deferred silently)

- MVP trusts any device that completes discovery + handshake on Wi-Fi Direct or ADB. This means:
  - Wi-Fi Direct: any device that can join the advertised P2P group (Android controls this via P2P group visibility/password, so exposure is bounded by Android's own P2P group settings, not by TurboTransfer).
  - USB/ADB: bounded by ADB authorization (Windows must already be ADB-authorized on the Android device — existing OS-level trust gate).
- No transport-layer encryption for chunk payloads in MVP. Do not use for sensitive data over untrusted networks until pairing + encryption (PIN-based, deferred from FluxSync's design) is implemented.
- **Action item:** revisit before any release beyond internal/personal use — add PIN-pairing + payload encryption using the same deferred design FluxSync had scoped.

---

## 12. Settings (Persisted Config)

| Setting | Options | Default |
|---|---|---|
| Chunk size | 16/32/64/128/256 MiB, custom | 64 MiB (start point; benchmark milestone 12 selects final default) |
| Buffer count | Automatic, 4, 8, 16 | Automatic (= 8) |
| Scheduling | Adaptive, Balanced | Adaptive |
| Transport preference (per-transfer override) | Automatic, Combined, USB only, Wi-Fi Direct only | Automatic |

Config stored as `settings.json` alongside `meta.json` files in the app's data directory (`%APPDATA%/turbotransfer` on Windows, app-private storage on Android).

---

## 13. TUI (ratatui)

- Screens: Main Menu, Send Files (Browse/Recent/Enter path), File Browser, Device Selection, Transport Selection, Transfer Screen, Transfer Details, Receive Files, Incoming Transfer prompt, Devices, Transfers (Current/Resumable/Completed), Resume, Benchmark, Benchmark Results, Settings (Transport/Transfer/Performance/Storage/Security/Interface).
- Navigation: arrows/Enter/Esc/Space/Tab as primary; number keys 1–6 as main-menu shortcuts; P/R/C/D as transfer shortcuts (Pause/Resume/Cancel/Details).
- TUI-local state (`current_screen`, `selected_item`, `navigation_position`, `input_mode`) lives entirely in the TUI layer, is never persisted, and is fully decoupled from transfer state — TUI reconstructs its view purely by polling `get_progress`/`get_transfers`/`get_devices` from the Transfer API.
- Polling interval for live transfer screen: 250ms (matches actor flush interval, avoids showing stale-by-more-than-one-flush data).

## 14. CLI

Commands map 1:1 to Transfer API calls: `turbo send <path> [--device <id>] [--transport combined|usb|wifi_direct|auto]`, `turbo receive`, `turbo discover`, `turbo benchmark [--transport ...]`, `turbo resume [<transfer_id>]`, `turbo cancel <transfer_id>`. No `--transport` flag → `Automatic`.

---

## 15. Testing Matrix (binding acceptance criteria)

| Direction | USB | Wi-Fi Direct | Combined |
|---|---|---|---|
| Android → Windows | ✓ | ✓ | ✓ |
| Windows → Android | ✓ | ✓ | ✓ |

| Scenario | Expected result |
|---|---|
| Wi-Fi disconnects mid-transfer | USB continues, no pause |
| USB disconnects mid-transfer | Wi-Fi continues, no pause |
| Both disconnect | Transfer → `Paused`, auto-resumes on reconnect |
| Wi-Fi reconnects | Pending chunks resume assignment to it |
| USB reconnects | Pending chunks resume assignment to it |
| Duplicate chunk delivery | No-op, safe (idempotent write) |
| Corrupted chunk (checksum fail) | `ChunkNack` → requeued → retried on any transport |
| Process restart mid-transfer | Resumes from `meta.json` `completed_ranges` |
| Out-of-order chunk arrival | Correct final file (direct offset writes) |

---

## 16. Open Items for Follow-Up

1. Windows Wi-Fi Direct join mechanism (§9) — needs a milestone-7 spike to confirm approach 1 (join as standard AP) actually works against Android's P2P group broadcast; approach 2 (UWP) is fallback.
2. USB POC go/no-go gate (§8) before full ADB-tunnel integration.
3. Final chunk size — resolved empirically at milestone 12 via the benchmark matrix (16/32/64/128/256 MiB).
4. Pairing + encryption (§11) — explicitly out of MVP scope, tracked as a required follow-up before broader use.

---

## 17. Observability & Logging Strategy

Structured, leveled logging across Rust core, transport layers, TUI live log viewer, and Android logcat. See detailed specification in [turbotransfer_logging_strategy.md](file:///d:/MyDocuments/Programming/android/Aug26/TurboTransfer/docs/turbotransfer_logging_strategy.md).

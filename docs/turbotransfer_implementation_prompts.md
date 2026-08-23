# TurboTransfer — Implementation Prompts

One prompt per TRD milestone, in the TRD's binding order (§3), plus a scaffolding prompt and one bridge step the TRD doesn't explicitly call out. Paste each into Antigravity in order — don't skip ahead, since later prompts assume earlier ones are already merged into the repo.

Before Prompt 0: make sure `/docs/turbotransfer_trd.md` exists in the repo (use your latest version). Every prompt below points Antigravity at that path.

Practical habit: commit after each prompt's tests pass, before starting the next. Gives you a clean rollback point if a later prompt goes sideways.

## Model guidance (applies to every prompt below unless noted)

Default to **Gemini 3.7 Flash (High)** in Antigravity's model picker. As of this TRD's release, Flash beats Gemini 3.1 Pro on essentially every published coding/agentic benchmark — Pro's remaining edge is narrow and confined to PhD-level-reasoning benchmarks, not code. Flash is also faster, cheaper, has a newer knowledge cutoff (helps with current Rust/Tokio/UniFFI/ratatui APIs), and isn't stuck behind Pro's tighter preview-model quota on the AI Pro plan. I've flagged the handful of spots below where trying Gemini 3.1 Pro (High) as a second opinion is worth the quota.

## Mode guidance

"Agent-assisted" is a sensible default throughout. I've flagged the couple of spots (real-hardware spikes, the USB go/no-go gate) where switching to "Review-driven" is worth it so you can watch each step.

## What I incorporated from your TRD update

Your revised §5.2/§5.3 — `role`/`peer_device_id` instead of `direction`/`device_id`, and the actor's in-memory `HashSet<u32>`/`RoaringBitmap` tracking, coalesced into ranges only at flush time — is folded into Milestone 4 below.

## Assumptions I made (TRD doesn't fully specify these — say if I guessed wrong)

- No dedicated "build the Android app" milestone exists in the TRD's 12. I added a minimal harness (UniFFI + bare Compose UI) between milestones 5 and 6, since milestone 6 onward needs a real Android endpoint. Full Compose UI polish (parity with the TUI's 15 screens) isn't in the TRD's MVP scope — treat it as a natural follow-up ask once milestones 1–11 are done.
- I introduced a `Transport` trait at milestone 6, since §6.1/§8/§9 all converge on "plain TCP once the tunnel/network exists" even though the TRD doesn't name the abstraction directly.
- Real file I/O (pre-allocated `.part` file, direct-offset writes, rename + CRC32C check) first appears at milestone 5, since that's the first milestone that needs to move a real file end-to-end.
- I don't know if you have the old FluxSync codebase to reference, or whether you'll have a Windows PC + Android device on hand at each step. I wrote the USB POC (8a) as clean-room, and leaned on automated/simulated tests wherever the TRD allows — flagging the handful of milestones (7, 8) that fundamentally need real hardware regardless. Tell me if either guess is wrong and I'll adjust the affected prompts.

## Quick reference

| Step | Milestone | Model | Notes |
|---|---|---|---|
| 0 | Scaffolding | Flash (High) | |
| 1 | Wire protocol | Flash (High) | |
| 2 | Chunk engine | Flash (High) | |
| 3 | Stateless data path | Flash (High) | |
| 4 | Control plane / actor | Flash (High) | optional Pro (High) review pass |
| 5 | CLI + Transfer API + file I/O | Flash (High) | |
| 5.5 | Android harness *(added)* | Flash (High) | |
| 6 | TCP prototype + Transport trait | Flash (High) | |
| 7a | Wi-Fi Direct spike | Flash (High), Pro (High) if stuck | Review-driven mode |
| 7b | Wi-Fi Direct integration | Flash (High) | |
| 8a | USB POC | Flash (High) | Review-driven mode — go/no-go gate |
| 8b | USB integration | Flash (High) | |
| 9 | Multipath scheduler | Flash (High) | optional Pro (High) review pass |
| 10 | Resume/retry | Flash (High) | |
| 11a–c | TUI (3 prompts) | Flash (High) | |
| 12 | Perf optimization | Flash (High) | optional, post-MVP |

---

## Milestone 0 — Repo & workspace scaffolding

**Model:** Gemini 3.7 Flash (High)

```
Read /docs/turbotransfer_trd.md in full, especially §2 (architecture decisions) and §4 (repository structure).

Set up the project skeleton exactly per §4:
- A Cargo workspace at the repo root with members: core (submodules transfer, chunk, scheduler, manifest, protocol, checksum — put these as modules inside one `core` lib crate unless you have a good reason not to, since they'll share types heavily), transport/usb, transport/wifi_direct, tui, cli.
- windows/ as a thin crate for Windows-only platform glue (adb process invocation, network join APIs) — depends on core.
- android/ as a separate Gradle/Kotlin project (NOT part of the Cargo workspace) — for now just scaffold an empty Android app module with Kotlin + Jetpack Compose set up, and a placeholder for where UniFFI-generated bindings will land. Don't wire up UniFFI yet.
- core must build as both a normal rlib (for tui/cli/windows) and, later, a cdylib for Android — set the Cargo.toml crate-type list now even though we won't cross-compile for Android until milestone 5.5.

Add a root README with a one-paragraph project description and the milestone list from §3. Add a sensible Rust .gitignore. Use the latest stable Rust edition. Confirm `cargo build` and `cargo test` succeed on the empty workspace before finishing.

Report back: the final directory tree, and any deviation from §4 with your reasoning.
```

---

## Milestone 1 — Wire protocol

**Model:** Gemini 3.7 Flash (High)

```
Read /docs/turbotransfer_trd.md §6 (Wire Protocol) and §4's interface rule.

In core, implement the protocol module:
- The frame format from §6.1: 4-byte LE length prefix, 1-byte message_type, bincode-encoded payload.
- A Rust enum covering all 12 message types (§6.1's list) as typed structs/variants with the fields implied by the handshake sequence in §6.2 (Hello, TransferOffer, TransferAccept, TransferReject, ChunkData, ChunkAck, ChunkNack, Pause, Resume, Cancel, Complete, Heartbeat).
- Encode/decode functions for the full frame (message → bytes, bytes → message).
- An async frame reader that works over any tokio::io::AsyncRead and correctly handles partial reads — the underlying stream may deliver the length prefix and payload in arbitrarily small pieces, so buffer until a full frame is available before decoding.

This is pure protocol/serialization work — no real networking, no file I/O yet.

Write unit tests: round-trip encode/decode for every message type; a test that feeds the frame reader bytes in deliberately tiny chunks (1–3 bytes at a time) to prove partial-read handling is correct; a test for a malformed/truncated frame producing a clean error, not a panic.

Report back what you built and point me to the test file.
```

---

## Milestone 2 — Chunk engine

**Model:** Gemini 3.7 Flash (High)

```
Read /docs/turbotransfer_trd.md §5.1 (Chunk struct) and §6.3 (checksum choices).

In core, implement the chunk module:
- The Chunk struct from §5.1.
- A chunker: given a file path and chunk_size, compute the chunk plan (chunk_id, file_offset, payload_length for every chunk, including a correctly-sized final remainder chunk) without reading the whole file into memory. Must handle a zero-byte file (0 chunks) and a file smaller than one chunk (1 chunk) correctly.
- xxHash64 checksum for a chunk's payload, and CRC32C for a full file (§6.3) — use well-maintained crates for both rather than hand-rolling.
- A manifest generator: given a file path and chunk_size, produce the file_id (new UUID), file_name, file_size, and total_chunks needed to build a TransferOffer message (§6.2 step 4).

Write unit tests: chunk-boundary math for an exact multiple of chunk_size, a file with a remainder, a single-byte file, and a zero-byte file; checksum correctness against known xxHash64/CRC32C reference vectors (don't invent your own expected values); total_chunks formula correctness.

Report back what you built and point me to the test file.
```

---

## Milestone 3 — Stateless data path

**Model:** Gemini 3.7 Flash (High)

```
Read /docs/turbotransfer_trd.md §6.2 (handshake sequence) and §5.1 (idempotent write rule).

Wire the protocol module (milestone 1) and chunk module (milestone 2) together into an in-memory send/receive flow, with NO meta.json and no real transport yet:
- Using tokio::io::duplex (an in-memory bidirectional pipe) to stand in for "a connection," implement the full handshake from §6.2 steps 2–9: Hello exchange, TransferOffer/TransferAccept, streaming ChunkData answered by ChunkAck/ChunkNack, Complete.
- The idempotent-write check from §5.1: given (transfer_id, file_id, chunk_id, checksum), decide whether a chunk is a duplicate no-op. For this milestone, track "already seen" chunks in a plain in-memory set on the receiving side — keep this isolated/swappable, since it gets replaced by the real meta.json-backed actor in milestone 4, not hardcoded into the handshake logic.
- On checksum mismatch, receiver sends ChunkNack and sender retries that chunk.

Write an integration test that transfers a real multi-megabyte file (random bytes) end-to-end over the duplex pipe and asserts the reassembled bytes are identical to the source. Write a second test that deliberately corrupts one chunk's payload in transit and asserts it gets NACK'd and successfully retried. Write a third test that sends the same chunk twice and asserts the second is a no-op per the idempotent rule.

Report back what you built and point me to the test file.
```

---

## Milestone 4 — Stateful control plane (meta.json, single-writer actor)

**Model:** Gemini 3.7 Flash (High) for the build. This is one of two milestones (the other is 9) worth an optional second-opinion pass: once Flash finishes, paste the actor implementation into a fresh Gemini 3.1 Pro (High) turn and ask it specifically to review for race conditions, deadlocks, or any interleaving where a flush could observe a torn/partial state. Not required, but cheap insurance on the trickiest concurrency in the project.

```
Read /docs/turbotransfer_trd.md §5.2 (meta.json schema) and §5.3 (single-writer actor contract) closely — this is the most concurrency-sensitive milestone so far.

In core, implement the manifest module:
- The meta.json schema from §5.2 exactly as written, including the OS-agnostic `role` ("sender"/"receiver") and `peer_device_id` fields — not a hardcoded direction/device_id pair.
- The actor from §5.3, as a single Tokio task that owns all writes to one transfer's meta.json:
  - Receives ChunkCompleted, ChunkFailed, TransportStatusChanged, Pause, Cancel messages over an mpsc channel — no other task ever writes meta.json directly.
  - Maintains completed-chunk tracking in memory as a HashSet<u32> (use a RoaringBitmap instead if you think total_chunks could realistically be large enough to matter — your call, note which you picked and why).
  - Batches disk writes: flush on 10 events or 250ms, whichever comes first. On Pause, Cancel, or a process-exit signal, flush immediately and synchronously before returning.
  - On flush, coalesce the in-memory set into the minimal sorted, non-overlapping, non-adjacent list of inclusive [start,end] ranges before serializing — this coalesced form is what's written to meta.json.
  - On actor startup (including after a restart), read meta.json if it exists and expand completed_ranges back into the in-memory set; this is the sole source of resume truth.

Write unit tests: range-coalescing correctness (adjacent chunks merge into one range, non-adjacent stay separate, out-of-order insertion still produces a correct sorted/merged result, a single-chunk file); actor behavior tests using tokio::test — send 9 ChunkCompleted events and assert no flush yet, send a 10th and assert a flush happened; send Cancel mid-batch (before the count/time threshold) and assert an immediate synchronous flush; a restart-simulation test — run the actor, complete some chunks, drop it, start a fresh actor pointed at the same meta.json path, and assert its in-memory set matches what was flushed.

Report back what you built, which of HashSet/RoaringBitmap you chose and why, and point me to the test file.
```

---

## Milestone 5 — Basic direct CLI (turbo send / turbo receive over loopback TCP)

**Model:** Gemini 3.7 Flash (High)

```
Read /docs/turbotransfer_trd.md §7 (Transfer API), §10.4 (direct offset writes), and §14 (CLI).

This milestone has three parts:

1. In core/transfer, implement the Transfer API from §7 as real (not stubbed) functions, backed by milestones 1–4: start_transfer, pause_transfer, resume_transfer, cancel_transfer, get_progress, get_devices, get_transfers. Single-transport only for now — no scheduler/multipath yet (that's milestone 9) — so start_transfer can ignore transport_pref and just use a loopback TCP connection internally. get_devices/get_transfers can return minimal/placeholder data if there's no real discovery yet; note clearly in your report which functions are fully real vs. placeholder.

2. Implement real file I/O for the receive side: pre-allocate the .part file to full size on transfer start (sparse if the filesystem supports it), write each verified chunk directly to its file_offset (§10.4 — no reordering buffer, safe by construction even for out-of-order retries), verify the CRC32C file-level checksum once all chunks land, then rename .part to the final filename (§6.2 step 8).

3. In cli, implement `turbo send <path>` and `turbo receive` (§14), calling only the Transfer API — no transfer/transport/scheduling logic in the CLI crate itself (interface rule, §4).

Both commands should run over real loopback TCP (127.0.0.1, a real socket — not the in-memory duplex from milestone 3).

Write an integration test that runs turbo send and turbo receive against each other (two async tasks or subprocesses) transferring a real file end-to-end, confirming the output is byte-identical to the source, including its final filename. Then manually verify once yourself: build the CLI, run receive in one terminal and send in another, confirm a real file arrives intact.

Report back what's real vs. placeholder in the Transfer API, and point me to the test file.
```

---

## Milestone 5.5 — Minimal Android test harness *(added — see assumptions above)*

**Model:** Gemini 3.7 Flash (High). UniFFI/Gradle/NDK cross-compilation setup is exactly the kind of "run diagnostics before changing things" work Flash is well-suited to.

```
Read /docs/turbotransfer_trd.md §2 (Android integration row) and §4 (android/ folder description).

This isn't one of the TRD's 12 numbered milestones, but milestone 6 onward describes testing "Android↔Windows," and there's no dedicated "build the Android app" milestone in the TRD — so this is a minimal bridge, not the final Android app.

In android/:
- Wire up UniFFI: generate Kotlin bindings from the core crate (cdylib target), and get a minimal Kotlin app calling into core successfully.
- Build the smallest possible Compose UI that can act as a real transfer endpoint: a screen to trigger send (basic file picker), a screen to trigger receive, and a visible status indicator. This does not need to match the TUI's screen list in §13 — that's a much later, fuller Android UI you're not building yet.
- Cross-compile core for the relevant Android ABI(s), confirm the app builds and runs on a device or emulator, and confirm it can successfully call the real Transfer API functions from milestone 5 (start_transfer, get_progress, etc.) via the UniFFI bindings — you can validate this fully before any real network transport exists, since milestone 5's CLI/API already works over loopback.

Report back: confirmation the UniFFI boundary works, which Android ABI(s)/API level you targeted, and a screenshot or description of the minimal UI.
```

---

## Milestone 6 — TCP prototype + Transport trait

**Model:** Gemini 3.7 Flash (High)

```
Read /docs/turbotransfer_trd.md §8 and §9 — note both real transports describe themselves as "a plain TCP socket once the tunnel/group is up, same framing as §6.1." The TRD doesn't name a shared abstraction explicitly, but that phrasing implies one, so:

Define a Transport trait in core (e.g. core/transport, or inside core/transfer — your call) that both the future WifiDirectTransport (milestone 7) and UsbTransport (milestone 8) will implement: connect/establish, send a frame, receive a frame, report current status (Connected/Disconnected), and a way to surface transport-level errors up to the retry logic in §10.3 (that logic itself isn't built until milestone 9 — for now just make sure the trait exposes what a scheduler would eventually need).

Implement a first concrete implementation, TcpTransport, using real OS sockets — bind to a real interface (not just 127.0.0.1), connect to a peer's IP:port, and carry the framing from milestone 1 over it. This is the "TCP prototype" the TRD asks for at this milestone: it proves the Transport trait shape and the real-network path both work, ahead of the more complex transport-specific discovery logic in milestones 7–8.

Update start_transfer (§7) to optionally use TcpTransport across a real network. Test it for real between your Windows machine and the Android harness from the previous prompt, both directions — this is just proving the shared TCP path both Wi-Fi Direct and USB will build on.

Report back the Transport trait definition you settled on, since milestones 7 and 8 need to match it, and confirm the real Android↔Windows test worked both directions.
```

---

## Milestone 7 — Wi-Fi Direct transport

### 7a — Spike (validate approach 1 before building anything real)

**Model:** Try Gemini 3.7 Flash (High) first — it tends to run diagnostics before changing things, which suits a spike. If it stalls or seems to be guessing rather than getting real signal from the devices, this is one of the few spots worth switching to Gemini 3.1 Pro (High) for a fresh attempt — a genuinely ambiguous platform question, closer to where Pro's narrow edge might help. **Mode: Review-driven** — you'll want to watch this one, it touches real network config on both devices.

```
Read /docs/turbotransfer_trd.md §9 in full — this milestone is explicitly flagged as needing a spike before real integration.

Before writing any production code, validate approach 1 (§9's recommendation) end to end, as cheaply as possible:
1. On Android, use WifiP2pManager to create a P2P group with this device as group owner, and retrieve the resulting SSID/passphrase.
2. On Windows, join that SSID as a normal Wi-Fi network using standard OS network APIs (netsh or the WlanConnect Win32 API) — no UWP.
3. Confirm Windows gets an IP on the P2P group's subnet and can open a raw TCP socket to the Android device (a trivial echo is enough proof — this doesn't need our real framing yet).

This can be scrappy/throwaway code — the goal is a go/no-go answer on approach 1, not production quality. If it doesn't work, stop and report back what failed rather than trying approach 2 (UWP) yourself — that's a bigger decision to make together.

Report back clearly: did approach 1 work? What had to happen on each side (exact APIs/commands used), and anything that felt fragile or device-specific.
```

### 7b — Full integration

**Model:** Gemini 3.7 Flash (High)

```
Read /docs/turbotransfer_trd.md §9 and the Transport trait you defined in milestone 6.

Assuming the milestone-7a spike confirmed approach 1 works, build the real WifiDirectTransport implementing that Transport trait:
- Android: WifiP2pManager group-owner setup wrapped properly (not the throwaway spike code) and exposed via UniFFI so the Rust core can trigger it and read back the SSID/PSK.
- Windows: join logic per what worked in the spike, wired into WifiDirectTransport's connect().
- Failure detection per §9: no Heartbeat reply within 15s → transport marked Disconnected, in-flight chunks requeued (requeue logic itself lands in milestone 9 — for now just make sure the transport correctly reports Disconnected and stops accepting new sends).
- Reconnect per §9: Android keeps advertising the group; Windows retries joining the known SSID every 3s.

Write tests where you can (state-machine tests for Disconnected/reconnect logic don't need real hardware). For the parts that do need real hardware, give me a manual test checklist instead of trying to automate it.

Report back what's automated-tested vs. what needs my manual verification on real devices.
```

---

## Milestone 8 — USB transport (ADB tunnel)

### 8a — Isolated POC (go/no-go gate)

**Model:** Gemini 3.7 Flash (High). **Mode: Review-driven** — this is a go/no-go decision point; per the TRD you should stop and reconsider USB entirely if the numbers are bad, rather than let the agent barrel into 8b regardless.

```
Read /docs/turbotransfer_trd.md §8's "POC requirement" paragraph and §3's "USB POC gate."

Build a standalone benchmark, isolated from the main engine (a separate small binary or example — doesn't need to touch core/transfer): raw byte throughput over an adb forward tcp:<local_port> tcp:<device_port> tunnel, both directions, no chunking, no checksums, no framing — just bytes/sec across the tunnel.

I don't have my old FluxSync AOA throughput number in this doc — [FILL IN YOUR AOA BASELINE HERE, e.g. "AOA topped out around X MB/s"] — compare your result against that and tell me clearly whether it clears a meaningful margin. Per the TRD's own gate: if it doesn't, we need to re-open the USB transport decision before continuing to 8b.

Report back the raw numbers, both directions, and your recommendation on the gate.
```

### 8b — Full integration

**Model:** Gemini 3.7 Flash (High)

```
Read /docs/turbotransfer_trd.md §8 in full and the Transport trait from milestone 6.

Assuming milestone 8a cleared the gate, build the real UsbTransport implementing the Transport trait:
- Windows enumerates adb devices, filters to devices running the TurboTransfer Android service, verified via the framed Hello handshake (milestone 1) on the forwarded port.
- Android runs a localhost-only TCP listener on the service's port, started by the TurboTransfer Android service (this needs a small addition to the android/ harness from milestone 5.5).
- Failure detection: socket error or the device dropping out of adb devices → Disconnected, in-flight chunks returned to the pending queue (queue itself is milestone 9 — for now, correctly report Disconnected and stop new sends).
- Reconnect: poll adb devices every 2s, re-establish the tunnel and mark Connected when the device reappears.

Write automated tests for whatever doesn't need real hardware (discovery-filtering logic, reconnect state machine). Give me a manual checklist for the rest.

Report back what's automated vs. needs my manual verification.
```

---

## Milestone 9 — Multipath scheduler

**Model:** Gemini 3.7 Flash (High) for the build. Same optional Gemini 3.1 Pro (High) second-opinion review as milestone 4 — this is the other spot in the project where a concurrency bug would be the most painful to find later.

```
Read /docs/turbotransfer_trd.md §10 in full — this is where everything built so far (protocol, chunk engine, actor, both real transports) comes together, so it's the second concurrency-heavy milestone (the other was milestone 4).

In core/scheduler, implement:
- §10.1: one shared pending-chunk queue; a worker per connected transport pulling when it has spare capacity (bounded in-flight per transport, default 4); rate-adaptive assignment via rolling 2s-window throughput per transport — no static ratio math, just "pull immediately when you finish and have capacity."
- §10.2: a bounded buffer pool (default 8 × chunk_size, configurable), a single disk-bound reader task filling buffers on demand, a single disk-bound writer task (same actor-ownership pattern as milestone 4's meta.json writer) draining completed chunks to the .part file.
- §10.3: ChunkNack or transport failure mid-chunk returns the chunk to the pending queue and increments the retries counter in meta.json's transport_stats; retry can land on any currently-connected transport, not necessarily the one it failed on. No retry cap for MVP.
- §10.5: losing one transport requeues its in-flight chunks and the remaining transport(s) continue with no pause; losing all transports moves the transfer to Paused (synchronous meta.json flush), auto-resuming when any transport reconnects.

Wire this into the real WifiDirectTransport and UsbTransport from milestones 7–8 via the Transport trait, and into start_transfer/get_progress from milestone 5's Transfer API, replacing the single-transport shortcut it currently uses.

Write automated tests for every row of §15's testing matrix that doesn't strictly require real hardware — simulate transport disconnect/reconnect with a test double implementing the Transport trait that drops/restores its connection on command, rather than physically unplugging anything: Wi-Fi disconnects mid-transfer (USB continues, no pause); USB disconnects (Wi-Fi continues); both disconnect (Paused, auto-resume on reconnect); reconnect reassigns pending chunks; duplicate chunk delivery is a no-op; corrupted chunk gets NACK'd and retried on a different transport; out-of-order chunk arrival still produces a correct final file. Then give me a manual checklist for re-confirming the same scenarios on real hardware.

Report back which §15 rows are automated vs. need my manual confirmation.
```

---

## Milestone 10 — Resume/retry

**Model:** Gemini 3.7 Flash (High)

```
Read /docs/turbotransfer_trd.md §6.2 step 5 (resume_from in TransferAccept), §7 (resume_transfer, get_transfers), and §14 (turbo resume).

This milestone closes the loop on cold resume — surviving a full process restart, not just an in-flight retry (already handled by milestone 9's per-chunk retry).

Implement:
- On TransferOffer, if the receiver already has a meta.json for this (transfer_id, file_id), populate TransferAccept's resume_from with its completed_ranges, and have the sender skip already-completed chunks rather than resending them.
- get_transfers (§7) correctly distinguishing current (actively transferring), resumable (paused/interrupted, meta.json exists with incomplete ranges), and completed transfers.
- resume_transfer (§7) and turbo resume [<transfer_id>] (§14) — resuming a specific transfer, or (with no ID) the most recent resumable one, restarting the actor from milestone 4 pointed at the existing meta.json and continuing from its completed_ranges.

Write a test that starts a transfer, lets some chunks complete, kills the process (or simulates process death by dropping everything without a graceful shutdown), starts a fresh process pointed at the same meta.json, runs resume, and confirms it picks up correctly and produces a byte-identical final file — this is §15's "process restart mid-transfer" row.

Report back what you built and point me to the test file.
```

---

## Milestone 11 — TUI (ratatui)

### 11a — Navigation shell + Main Menu + Settings

**Model:** Gemini 3.7 Flash (High)

```
Read /docs/turbotransfer_trd.md §13 in full.

In tui, build:
- The navigation shell: TUI-local state (current_screen, selected_item, navigation_position, input_mode) kept entirely inside the tui crate, never persisted, fully decoupled from transfer state — the TUI reconstructs its view purely by polling get_progress/get_transfers/get_devices from the Transfer API (interface rule, §4 — no transfer logic in this crate).
- Global key handling: arrows/Enter/Esc/Space/Tab as primary navigation, number keys 1–6 as main-menu shortcuts.
- The Main Menu screen and the Settings screen with its six sub-tabs (Transport/Transfer/Performance/Storage/Security/Interface), backed by the settings.json config from §12 — reading and writing real settings, not placeholders.

Don't build the other screens yet (Send/Receive/Devices/Transfers/Benchmark) — just enough navigation scaffolding that Main Menu and Settings fully work and it's obvious where the next screens plug in.

Report back a short description of the navigation architecture so the next TUI prompts can follow the same pattern.
```

### 11b — Send/Receive/Device/Transport selection flow

**Model:** Gemini 3.7 Flash (High)

```
Read /docs/turbotransfer_trd.md §13, and the navigation shell you built in the previous TUI prompt — follow its established pattern.

Build the send/receive flow screens: Send Files (Browse/Recent/Enter path), File Browser, Device Selection, Transport Selection, Receive Files, Incoming Transfer prompt, and Devices. These call get_devices and start_transfer from the Transfer API — no direct transfer/transport logic in this crate.

Report back which Transfer API calls each screen ended up using.
```

### 11c — Live transfer screens + Benchmark

**Model:** Gemini 3.7 Flash (High)

```
Read /docs/turbotransfer_trd.md §13, especially the polling-interval note.

Build: Transfer Screen (live, polling get_progress every 250ms — matches the actor's flush interval from §5.3 so you're not showing data staler than one flush), Transfer Details, the Transfers screen with its Current/Resumable/Completed tabs (backed by get_transfers), Resume screen, Benchmark and Benchmark Results screens (backed by run_benchmark). Wire up the P/R/C/D transfer shortcuts (Pause/Resume/Cancel/Details) from §13.

This is the last TUI prompt — once it's done, do a full pass: navigate every screen listed in §13 and confirm nothing's missing or unreachable.

Report back a checklist of all 15 screens from §13 confirming each is implemented and reachable.
```

---

## Milestone 12 — Performance optimization (post-MVP, optional for now)

**Model:** Gemini 3.7 Flash (High)

```
This is post-MVP tuning (§3) — only run this once milestones 1–11 are solid and you've actually used the app for real transfers. Read /docs/turbotransfer_trd.md §12 and §16 item 3.

Using the Benchmark screen/run_benchmark from milestone 11, run the chunk-size sweep across 16/32/64/128/256 MiB on your real hardware and transports, and update the default in §12's settings table based on what actually performs best for your setup. Look for other easy wins the benchmark data surfaces (buffer count, in-flight-per-transport default of 4) but don't over-engineer this — it's explicitly the lowest-priority milestone.

Report back the benchmark results and your recommended new defaults.
```

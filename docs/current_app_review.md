# TurboTransfer Current App Review

**Review date:** 2026-08-23  
**Device:** Android `CPH2723` / ADB serial `b9b2c03f`  
**Scope:** README, technical docs, Android implementation, Rust core, build/test state

## Environment status

- The phone is connected and authorized through modern ADB 1.0.41.
- The app package `com.turbotransfer` is installed.
- No transfer tunnel currently exists on port `9876`; only auxiliary reverse rules on `9875` and `9877` were present.
- No source files were changed during this review.

## Highest-priority issues

### 1. Combined transfer is not actually multipath

The API selects one `Box<dyn Transport>` for Automatic/Combined instead of running USB and hotspot workers concurrently. This contradicts the README and TRD claims about simultaneous USB and Wi-Fi aggregation.

Evidence: [core/src/transfer/api.rs](../core/src/transfer/api.rs#L172)

### 2. The Android hotspot receiver discards transfer data

The hotspot TCP server accepts a connection and drains raw bytes, but does not pass the framed transfer protocol to the Rust receiver. It can also contend with Receive mode for port `9876`.

Evidence: [WifiHotspotManager.kt](../android/app/src/main/java/com/turbotransfer/WifiHotspotManager.kt#L206)

### 3. Benchmark results are fabricated constants

`run_benchmark` returns fixed throughput values rather than measuring an actual transport.

Evidence: [core/src/transfer/api.rs](../core/src/transfer/api.rs#L593)

### 4. Pause and Cancel cannot work for current transfers

Every transfer registry record stores `actor_handle: None`, so the Pause and Cancel functions have no actor to signal.

Evidence: [core/src/transfer/api.rs](../core/src/transfer/api.rs#L153)

### 5. Stop Receive only changes the UI

The Android Receive button updates Compose state but does not call a native `leave_receive_mode`. The native join handle is discarded, so the listener cannot be managed or stopped reliably.

Evidence: [MainActivity.kt](../android/app/src/main/java/com/turbotransfer/MainActivity.kt#L254), [core/src/uniffi_interface.rs](../core/src/uniffi_interface.rs#L157)

### 6. Resume loses the original source path

Resume reconstructs the sender source using only `meta.file_name`, not the original full source path. Sender resume will therefore generally fail unless the file happens to be in the current working directory.

Evidence: [core/src/transfer/api.rs](../core/src/transfer/api.rs#L414)

### 7. Exported broadcast receiver has a security boundary problem

The receiver is registered with `Context.RECEIVER_EXPORTED` and accepts a file path and destination address from an external intent. Another app could trigger transfers using paths accessible to TurboTransfer. Make it non-exported or protect it with a signature permission.

Evidence: [MainActivity.kt](../android/app/src/main/java/com/turbotransfer/MainActivity.kt#L84)

### 8. Hotspot addressing and credential handling are fragile

The hotspot gateway is hard-coded to `192.168.43.1`; it should be discovered from the active interface. The unauthenticated control server also exposes hotspot credentials to any client that can reach port `9875`.

Evidence: [WifiHotspotManager.kt](../android/app/src/main/java/com/turbotransfer/WifiHotspotManager.kt#L95)

### 9. Large URI-selected files are duplicated into cache

When a content URI has no direct filesystem path, the app copies the entire file into its cache before transferring it. For multi-gigabyte media this doubles storage usage and can fail due to insufficient app cache space.

Evidence: [UriUtils.kt](../android/app/src/main/java/com/turbotransfer/UriUtils.kt#L59)

## Documentation drift

The README and implementation are currently inconsistent:

- README describes Wi-Fi Direct P2P, while the documented successful spike recommends Android Local-Only Hotspot.
- README claims simultaneous multipath, but the current API selects one transport.
- README describes 64 MiB chunks, while the transfer API uses 4 MiB chunks.
- README shows `encryption_enabled: true`, while the TRD explicitly says payload encryption is deferred from MVP.
- README advertises 42 passing tests; the deterministic Rust tests inspected here contain 31 tests, all of which passed when run directly. Android JVM tests fail because they try to load an Android `.so` as a Windows DLL.

## Recommended improvement order

1. Finish one honest USB-only end-to-end path, then connect the hotspot listener to the same framed protocol.
2. Add a foreground Android service with notification actions for reliable background transfers and lifecycle ownership.
3. Store and wire the `MetaActor` handle so Pause, Cancel, and Resume have real state transitions.
4. Split unit tests from explicitly invoked hardware/live tests. `cargo test --workspace` currently includes device-dependent tests and can stall.
5. Move Android native-boundary tests to instrumentation tests, or mock the Kotlin-facing native interface for JVM tests.
6. Replace the hard-coded hotspot address with active-interface discovery and authenticate/control credential exchange.
7. Stream content URIs through a file descriptor instead of copying multi-gigabyte files into cache.
8. Update the README/TRD to match the proven Local-Only Hotspot architecture, actual chunk size, encryption status, and real test counts.

## Windows and PowerShell quality-of-life improvements

Use a project `scripts/dev.ps1` wrapper that builds the native library, installs the APK, establishes the `adb forward tcp:9876 tcp:9876` tunnel, tails filtered logcat, and runs a smoke test.

For the Visual Studio PowerShell terminal:

```powershell
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new()
```

Use `2>$null` instead of Bash's `2>/dev/null`. PowerShell also does not support Bash brace expansion such as `{a,b}`; use an explicit array of paths instead.

Finally, initialize Git before more milestones. The repository currently has a `.gitignore` that ignores `Cargo.lock`; for this application, committing the lockfile would improve reproducible builds.

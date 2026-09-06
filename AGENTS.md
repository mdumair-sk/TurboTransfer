# 🚨 MANDATORY AI INSTRUCTION: SNAPDRAGON 8 ELITE COMPILATION & TEST POLICY

> **CRITICAL RULES FOR ALL AI MODELS / ASSISTANTS WORKING IN THIS REPOSITORY**
> 
> 1. **MANDATORY ANDROID DEPLOYMENT**: Each time changes are made to `core/` or `android/` files, ALWAYS deploy the Android app to the connected device via:
>    `powershell -ExecutionPolicy Bypass -File .\tools\phone-builder.ps1 deploy`
> 2. **DESKTOP TUI RELEASE BUILD**: Always compile `tui.exe` (`turbotransfer-tui`) in `target/release/` via:
>    `cargo build --release -p turbotransfer-tui`
> 3. **PARALLEL TUI & ANDROID BUILD**: While deploying to Android via `phone-builder.ps1 deploy`, launch `cargo build --release -p turbotransfer-tui` concurrently in parallel to save time (phone builds Android core, PC builds desktop TUI).
> 4. **OFFLOAD RUST COMPILATION**: **DO NOT** run heavy workspace Rust test suites or cross-compilations directly on the host laptop. **ALWAYS** offload native core compilation and test suites to the connected **Snapdragon 8 Elite** Android phone via the ADB bridge script (`tools/phone-builder.ps1`).
---

## ⚡ Why This Rule Exists

* **Host Laptop Hardware**: AMD Ryzen 5 3550H (4 Cores / 8 Threads, Zen+ 12nm, 2019). It overheats, throttles, and takes several minutes to cross-compile Tokio/UniFFI.
* **Phone Hardware**: Qualcomm Snapdragon 8 Elite (2x Prime Oryon @ 4.32 GHz + 6x Perf Oryon @ 3.53 GHz, TSMC 3nm, LPDDR5X @ 8533 Mbps).
* **Speedup**: Compiling natively on the phone takes **0.22s – 2.6s** ($\approx$ **25x faster**) and keeps the laptop cool and responsive.

---

## 🛠️ Mandatory Command Execution Reference for AI

Whenever you modify code and need to verify, build, or test, use these exact commands:

### 1. Compiling `turbotransfer-core` & Updating Android `jniLibs`
**DO NOT RUN**: `cargo build --target aarch64-linux-android -p turbotransfer-core` on the PC.  
**RUN INSTEAD**:
```powershell
powershell -ExecutionPolicy Bypass -File .\tools\phone-builder.ps1 build-core
```
*This automatically syncs modified sources over ADB, compiles natively on the 8 Oryon cores in seconds, and downloads `libturbotransfer_core.so` directly into `android/app/src/main/jniLibs/arm64-v8a/`.*

---

### 2. Running Rust Tests
**DO NOT RUN**: `cargo test` on the PC.  
**RUN INSTEAD**:
```powershell
# Run all tests for turbotransfer-core
powershell -ExecutionPolicy Bypass -File .\tools\phone-builder.ps1 test -Package turbotransfer-core

# Run specific test (e.g. cold_resume, multipath, chunk)
powershell -ExecutionPolicy Bypass -File .\tools\phone-builder.ps1 test -Package turbotransfer-core -TestFilter <test_name>

# Run full workspace tests
powershell -ExecutionPolicy Bypass -File .\tools\phone-builder.ps1 test -Package workspace
```

---

### 3. Parallel Workflow: Android App Deploy & Desktop TUI Build
Whenever modifying `core/` or both core and UI code, run Android deployment and desktop TUI release build **in parallel** to save round-trip time:

1. **Terminal / Background Job 1 (Android Device via Phone Builder)**:
```powershell
powershell -ExecutionPolicy Bypass -File .\tools\phone-builder.ps1 deploy
```
*Builds `turbotransfer-core` natively on Snapdragon 8 Elite, updates `jniLibs` & Kotlin bindings, compiles APK, installs and starts app.*

2. **Terminal / Background Job 2 (Windows PC Host Concurrently)**:
```powershell
cargo build --release -p turbotransfer-tui
```
*Compiles `target\release\tui.exe` on desktop while phone compiles Android artifacts.*

---

### 4. Node Diagnostics & Shell
```powershell
# Check phone builder connectivity and toolchain
powershell -ExecutionPolicy Bypass -File .\tools\phone-builder.ps1 status

# Run clean if build cache is corrupted
powershell -ExecutionPolicy Bypass -File .\tools\phone-builder.ps1 clean
```

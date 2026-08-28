<#
.SYNOPSIS
    TurboTransfer - Snapdragon 8 Elite Remote Compilation & Test Orchestrator
.DESCRIPTION
    Offloads heavy Rust compilation and test suites to the Snapdragon 8 Elite
    via ADB USB/Wi-Fi tunnel using native ARM64 Oryon CPU cores.
.EXAMPLE
    .\tools\phone-builder.ps1 build-core
    .\tools\phone-builder.ps1 test
    .\tools\phone-builder.ps1 test -Package turbotransfer-core
    .\tools\phone-builder.ps1 shell
#>

param (
    [Parameter(Position = 0)]
    [ValidateSet("build-core", "build", "test", "sync", "shell", "clean", "status", "bench")]
    [string]$Command = "build-core",

    [Parameter(Position = 1)]
    [string]$Package = "turbotransfer-core",

    [string]$TestFilter = "",
    [switch]$Release,
    [switch]$ForceSync,
    [int]$Port = 8022,
    [string]$AdbPath = ""
)

$ErrorActionPreference = "Continue"
$ProjectRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$KeyPath = Join-Path $HOME ".ssh\id_turbotransfer"

function Find-Adb {
    if ($AdbPath -and (Test-Path $AdbPath)) { return $AdbPath }
    if (Get-Command adb -ErrorAction SilentlyContinue) { return (Get-Command adb).Source }
    $defaultPaths = @(
        "C:\adb\adb.exe",
        "D:\Android\sdk\platform-tools\adb.exe",
        "$env:LOCALAPPDATA\Android\Sdk\platform-tools\adb.exe",
        "C:\Android\sdk\platform-tools\adb.exe"
    )
    foreach ($p in $defaultPaths) {
        if (Test-Path $p) { return $p }
    }
    return "adb"
}

$ADB = Find-Adb

function Ensure-Tunnel {
    & $ADB forward tcp:$Port tcp:8022 2>$null
}

function Invoke-PhoneSSH {
    param(
        [string]$RemoteCommand,
        [switch]$Interactive
    )
    Ensure-Tunnel

    $sshArgs = @("-p", "$Port", "-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null", "-o", "LogLevel=ERROR")
    if (Test-Path $KeyPath) {
        $sshArgs += @("-i", $KeyPath)
    }

    if ($Interactive) {
        & ssh @sshArgs localhost
    } else {
        $sshArgs += @("localhost", $RemoteCommand)
        & ssh @sshArgs
    }
}

function Sync-SourceCode {
    Write-Host "Syncing source files to Snapdragon 8 Elite..." -ForegroundColor Cyan
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    Ensure-Tunnel

    $sshArgs = @("-p", "$Port", "-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null", "-o", "LogLevel=ERROR")
    if (Test-Path $KeyPath) { $sshArgs += @("-i", $KeyPath) }

    $sshStr = $sshArgs -join ' '
    $tarLine = "tar -czf - --exclude=target --exclude=.git --exclude=android/.gradle --exclude=android/build --exclude=android/app/build --exclude=received_files --exclude=*.apk --exclude=*.log -C `"$ProjectRoot`" core transport tui cli windows Cargo.toml Cargo.lock | ssh $sshStr localhost `"mkdir -p ~/turbotransfer; tar -xzf - -C ~/turbotransfer`""

    cmd.exe /c $tarLine

    $sw.Stop()
    Write-Host "  -> Synced in $($sw.ElapsedMilliseconds) ms" -ForegroundColor Green
}

if ($Command -eq "status") {
    Write-Host "Checking Snapdragon 8 Elite Node Status..." -ForegroundColor Cyan
    Ensure-Tunnel
    Invoke-PhoneSSH -RemoteCommand "uname -m; rustc --version; cargo --version"
}
elseif ($Command -eq "shell") {
    Write-Host "Connecting to Snapdragon 8 Elite interactive shell..." -ForegroundColor Cyan
    Invoke-PhoneSSH -Interactive
}
elseif ($Command -eq "clean") {
    Write-Host "Cleaning target build cache on phone..." -ForegroundColor Yellow
    Invoke-PhoneSSH -RemoteCommand "cd ~/turbotransfer; cargo clean"
    Write-Host "  -> Clean complete" -ForegroundColor Green
}
elseif ($Command -eq "sync") {
    Sync-SourceCode
}
elseif ($Command -eq "build-core") {
    Sync-SourceCode
    Write-Host "`nBuilding 'turbotransfer-core' natively on 8 Snapdragon Oryon Cores..." -ForegroundColor Yellow
    $sw = [System.Diagnostics.Stopwatch]::StartNew()

    Invoke-PhoneSSH -RemoteCommand "cd ~/turbotransfer; cargo build --release -p turbotransfer-core"

    $sw.Stop()
    $elapsedSec = [math]::Round($sw.Elapsed.TotalSeconds, 2)
    Write-Host "Native compilation finished in ${elapsedSec}s!" -ForegroundColor Green

    # Pull libturbotransfer_core.so back into jniLibs
    $destDir = Join-Path $ProjectRoot "android\app\src\main\jniLibs\arm64-v8a"
    if (-not (Test-Path $destDir)) { New-Item -ItemType Directory -Path $destDir -Force | Out-Null }
    $destFile = Join-Path $destDir "libturbotransfer_core.so"

    Write-Host "Pulling compiled shared library into jniLibs/arm64-v8a..." -ForegroundColor Cyan
    $scpArgs = @("-P", "$Port", "-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null", "-o", "LogLevel=ERROR")
    if (Test-Path $KeyPath) { $scpArgs += @("-i", $KeyPath) }
    $scpArgs += @("localhost:turbotransfer/target/release/libturbotransfer_core.so", $destFile)

    & scp @scpArgs
    if (Test-Path $destFile) {
        $fileSizeMB = (Get-Item $destFile).Length / 1MB
        $rounded = [math]::Round($fileSizeMB, 2)
        Write-Host "Successfully updated libturbotransfer_core.so ($rounded MB)" -ForegroundColor Green
    }
}
elseif ($Command -eq "build") {
    Sync-SourceCode
    Write-Host "`nBuilding '$Package' on Snapdragon 8 Elite..." -ForegroundColor Yellow
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $relFlag = if ($Release) { "--release" } else { "" }
    Invoke-PhoneSSH -RemoteCommand "cd ~/turbotransfer; cargo build $relFlag -p $Package"
    $sw.Stop()
    $elapsedSec = [math]::Round($sw.Elapsed.TotalSeconds, 2)
    Write-Host "Build finished in ${elapsedSec}s" -ForegroundColor Green
}
elseif ($Command -eq "test") {
    Sync-SourceCode
    Write-Host "`nExecuting Rust test suite natively on Snapdragon 8 Elite..." -ForegroundColor Yellow
    $sw = [System.Diagnostics.Stopwatch]::StartNew()

    $pkgFlag = if ($Package -eq "workspace") { "--workspace" } else { "-p $Package" }
    $filter = if ($TestFilter) { "-- $TestFilter --nocapture" } else { "-- --nocapture" }
    Invoke-PhoneSSH -RemoteCommand "cd ~/turbotransfer; cargo test $pkgFlag $filter"
    $sw.Stop()
    $elapsedSec = [math]::Round($sw.Elapsed.TotalSeconds, 2)
    Write-Host "`nTest run complete in ${elapsedSec}s" -ForegroundColor Green
}
elseif ($Command -eq "bench") {
    Sync-SourceCode
    Write-Host "`nRunning performance benchmarks on Snapdragon 8 Elite..." -ForegroundColor Yellow
    Invoke-PhoneSSH -RemoteCommand "cd ~/turbotransfer; cargo bench -p $Package"
}

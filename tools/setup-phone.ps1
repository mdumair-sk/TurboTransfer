# ==============================================================================
# TurboTransfer - Snapdragon 8 Elite Phone Builder Setup for Windows
# ==============================================================================
param (
    [string]$AdbPath = ""
)

$ErrorActionPreference = "Continue"

function Find-Adb {
    if ($AdbPath -and (Test-Path $AdbPath)) { return $AdbPath }
    if (Get-Command adb -ErrorAction SilentlyContinue) { return (Get-Command adb).Source }
    $defaultPaths = @(
        "D:\Android\sdk\platform-tools\adb.exe",
        "$env:LOCALAPPDATA\Android\Sdk\platform-tools\adb.exe",
        "C:\Android\sdk\platform-tools\adb.exe"
    )
    foreach ($p in $defaultPaths) {
        if (Test-Path $p) { return $p }
    }
    throw "ADB executable not found. Please specify -AdbPath or ensure it is in PATH."
}

$ADB = Find-Adb
Write-Host "Using ADB: $ADB" -ForegroundColor Cyan

# 1. Check connected devices
$devicesOutput = & $ADB devices
$devices = @($devicesOutput | Where-Object { $_ -match "\tdevice$" })
if ($devices.Count -eq 0) {
    Write-Warning "No authorized ADB devices detected. Please connect your phone via USB with USB Debugging enabled."
    exit 1
}
$devSerial = ($devices[0] -split '\s+')[0]
Write-Host "Detected device: $devSerial" -ForegroundColor Green

# 2. Disable Android Phantom Process Killer to prevent compilation termination
Write-Host "`nOptimizing Android background process limits for heavy compilation..." -ForegroundColor Cyan
try {
    & $ADB shell "device_config put activity_manager max_phantom_processes 2147483647" 2>$null
    & $ADB shell "setprop persist.sys.fflag.override.settings_enable_monitor_phantom_procs false" 2>$null
    Write-Host "  -> Phantom Process Killer disabled (unlimited background compiler processes allowed)" -ForegroundColor Green
} catch {
    Write-Host "  -> Note: Device config adjustment skipped (non-critical)" -ForegroundColor Yellow
}

# 3. Setup SSH Keypair for seamless passwordless remote builds
$sshDir = Join-Path $HOME ".ssh"
if (-not (Test-Path $sshDir)) { New-Item -ItemType Directory -Path $sshDir -Force | Out-Null }
$keyPath = Join-Path $sshDir "id_turbotransfer"
$pubKeyPath = "$keyPath.pub"

if (-not (Test-Path $keyPath)) {
    Write-Host "`nGenerating dedicated SSH keypair for phone builder ($keyPath)..." -ForegroundColor Cyan
    ssh-keygen -t ed25519 -f $keyPath -N '""' -q
    Write-Host "  -> Generated ed25519 keypair" -ForegroundColor Green
}

$pubKeyContent = (Get-Content $pubKeyPath -Raw).Trim()

# 4. Configure ADB Port Forwarding (Port 8022 for Termux SSH)
Write-Host "`nSetting up ADB Port Forward (PC:8022 -> Phone:8022)..." -ForegroundColor Cyan
& $ADB forward tcp:8022 tcp:8022
Write-Host "  -> Port forward established (localhost:8022 -> phone:8022)" -ForegroundColor Green

# 5. Push setup helper script to phone /sdcard/ for easy execution if needed
$toolsDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$setupScript = Join-Path $toolsDir "setup-phone-termux.sh"
if (Test-Path $setupScript) {
    & $ADB push $setupScript /sdcard/Download/setup-phone-termux.sh 2>$null
}

# 6. Instructions for one-time Termux execution
Write-Host "`n=================================================================" -ForegroundColor Magenta
Write-Host "ONE-TIME SETUP ON YOUR PHONE (Inside Termux App):" -ForegroundColor Yellow
Write-Host "=================================================================" -ForegroundColor Magenta
Write-Host "Open Termux on your phone and run these commands:" -ForegroundColor White
Write-Host ""
Write-Host "1. Install build tools and OpenSSH:" -ForegroundColor Cyan
Write-Host "   pkg update -y; pkg install -y rust clang binutils git openssh tar make pkg-config" -ForegroundColor Gray
Write-Host ""
Write-Host "2. Authorize this PC SSH key:" -ForegroundColor Cyan
Write-Host "   mkdir -p ~/.ssh; echo '$pubKeyContent' >> ~/.ssh/authorized_keys; chmod 600 ~/.ssh/authorized_keys" -ForegroundColor Gray
Write-Host ""
Write-Host "3. Start the SSH daemon on your phone:" -ForegroundColor Cyan
Write-Host "   sshd" -ForegroundColor Gray
Write-Host ""
Write-Host "=================================================================" -ForegroundColor Magenta
Write-Host "Once ready, test the connection by running:" -ForegroundColor Green
Write-Host ".\tools\phone-builder.ps1 test" -ForegroundColor White
Write-Host "=================================================================" -ForegroundColor Magenta

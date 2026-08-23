param(
    [int]$Port = 9876,
    [long]$SizeBytes = 104857600 # 100 MB default
)

Write-Host "================================================================" -ForegroundColor Cyan
Write-Host " TurboTransfer - Milestone 8a USB ADB Tunnel POC Benchmark" -ForegroundColor Cyan
Write-Host " Testing bidirectional raw TCP throughput over adb forward" -ForegroundColor Cyan
Write-Host "================================================================" -ForegroundColor Cyan

# 1. Setup ADB forward
Write-Host "[1/3] Setting up ADB port forward (tcp:$Port -> tcp:$Port)..." -ForegroundColor Yellow
adb forward tcp:$Port tcp:$Port
if ($LASTEXITCODE -ne 0) {
    Write-Error "Failed to set up adb forward"
    exit 1
}

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)

# -------------------------------------------------------------
# 2. Upload Benchmark (PC -> Android)
# -------------------------------------------------------------
$mb = [Math]::Round($SizeBytes / 1MB, 2)
Write-Host "`n[2/3] Running UPLOAD Benchmark (PC -> Android, $mb MB)..." -ForegroundColor Yellow

$socket = New-Object System.Net.Sockets.TcpClient
$socket.ReceiveBufferSize = 4 * 1024 * 1024
$socket.SendBufferSize = 4 * 1024 * 1024
$socket.NoDelay = $true

try {
    $socket.Connect("127.0.0.1", $Port)
    $stream = $socket.GetStream()
    $reader = New-Object System.IO.StreamReader($stream, $utf8NoBom)
    $writer = New-Object System.IO.StreamWriter($stream, $utf8NoBom)
    $writer.AutoFlush = $true

    # Send handshake command
    $writer.WriteLine("STREAM_TEST:$SizeBytes")

    $readyLine = $reader.ReadLine()
    if ($readyLine -ne "READY") {
        Write-Error "Expected 'READY', got: '$readyLine'"
        exit 1
    }

    # Stream random binary payload
    $chunkSize = 256 * 1024
    $buffer = New-Object byte[] $chunkSize
    $rng = New-Object System.Random
    $rng.NextBytes($buffer)

    $bytesSent = 0L
    $sw = [System.Diagnostics.Stopwatch]::StartNew()

    while ($bytesSent -lt $SizeBytes) {
        $toSend = [Math]::Min($chunkSize, $SizeBytes - $bytesSent)
        $stream.Write($buffer, 0, $toSend)
        $bytesSent += $toSend
    }
    $stream.Flush()
    $sw.Stop()

    $uploadClientSec = $sw.Elapsed.TotalSeconds
    $uploadClientMBps = ($SizeBytes / 1MB) / $uploadClientSec
    $uploadClientMbps = ($SizeBytes * 8 / 1000000) / $uploadClientSec

    $resultLine = $reader.ReadLine()
    $uploadServerMBps = $uploadClientMBps
    if ($resultLine -and $resultLine.StartsWith("RESULT:")) {
        $parts = $resultLine.Split(":")
        $uploadServerMBps = [double]$parts[1]
    }

    Write-Host "  -> Upload Speed (PC -> Android): $([Math]::Round($uploadServerMBps, 2)) MB/s ($([Math]::Round($uploadServerMBps * 8.3886, 1)) Mbps) in $([Math]::Round($uploadClientSec, 2))s" -ForegroundColor Green
} finally {
    $socket.Close()
}

Start-Sleep -Milliseconds 500

# -------------------------------------------------------------
# 3. Download Benchmark (Android -> PC)
# -------------------------------------------------------------
Write-Host "`n[3/3] Running DOWNLOAD Benchmark (Android -> PC, $mb MB)..." -ForegroundColor Yellow

$socket = New-Object System.Net.Sockets.TcpClient
$socket.ReceiveBufferSize = 4 * 1024 * 1024
$socket.SendBufferSize = 4 * 1024 * 1024
$socket.NoDelay = $true

try {
    $socket.Connect("127.0.0.1", $Port)
    $stream = $socket.GetStream()
    $reader = New-Object System.IO.StreamReader($stream, $utf8NoBom)
    $writer = New-Object System.IO.StreamWriter($stream, $utf8NoBom)
    $writer.AutoFlush = $true

    $writer.WriteLine("STREAM_DOWNLOAD:$SizeBytes")

    $readyLine = $reader.ReadLine()
    if ($readyLine -ne "READY") {
        Write-Error "Expected 'READY', got: '$readyLine'"
        exit 1
    }

    $chunkSize = 256 * 1024
    $buffer = New-Object byte[] $chunkSize
    $bytesReceived = 0L

    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    while ($bytesReceived -lt $SizeBytes) {
        $toRead = [Math]::Min($chunkSize, $SizeBytes - $bytesReceived)
        $read = $stream.Read($buffer, 0, $toRead)
        if ($read -le 0) { break }
        $bytesReceived += $read
    }
    $sw.Stop()

    $downloadSec = $sw.Elapsed.TotalSeconds
    $downloadMBps = ($bytesReceived / 1MB) / $downloadSec
    $downloadMbps = ($bytesReceived * 8 / 1000000) / $downloadSec

    Write-Host "  -> Download Speed (Android -> PC): $([Math]::Round($downloadMBps, 2)) MB/s ($([Math]::Round($downloadMbps, 1)) Mbps) in $([Math]::Round($downloadSec, 2))s" -ForegroundColor Green
} finally {
    $socket.Close()
}

Write-Host "`n================================================================" -ForegroundColor Cyan
Write-Host " ADB TUNNEL POC BENCHMARK SUMMARY ($mb MB)" -ForegroundColor Cyan
Write-Host "  * PC -> Android (Upload)  : $([Math]::Round($uploadServerMBps, 2)) MB/s" -ForegroundColor Cyan
Write-Host "  * Android -> PC (Download): $([Math]::Round($downloadMBps, 2)) MB/s" -ForegroundColor Cyan
Write-Host "================================================================" -ForegroundColor Cyan

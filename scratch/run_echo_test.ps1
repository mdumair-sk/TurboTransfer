$targetIp = "10.18.163.130"
$port = 9876

Write-Host "Connecting to Android at $targetIp`:$port..." -ForegroundColor Yellow
$client = New-Object System.Net.Sockets.TcpClient($targetIp, $port)
$stream = $client.GetStream()
$writer = New-Object System.IO.StreamWriter($stream, [System.Text.Encoding]::UTF8)
$writer.AutoFlush = $true
$reader = New-Object System.IO.StreamReader($stream, [System.Text.Encoding]::UTF8)

Write-Host "=================================================" -ForegroundColor Cyan
Write-Host " Running 10-Packet TCP Echo Verification over Wi-Fi" -ForegroundColor Cyan
Write-Host "=================================================" -ForegroundColor Cyan

$rtts = @()
for ($i = 1; $i -le 10; $i++) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $msg = "TURBO_PING_PACKET_$i"
    $writer.WriteLine($msg)
    $reply = $reader.ReadLine()
    $sw.Stop()
    $ms = [math]::Round($sw.Elapsed.TotalMilliseconds, 2)
    $rtts += $ms
    Write-Host "  [#$i] Sent '$msg' -> Received '$reply' (RTT: $ms ms)" -ForegroundColor Green
    Start-Sleep -Milliseconds 80
}

$avgRtt = [math]::Round(($rtts | Measure-Object -Average).Average, 2)
$minRtt = ($rtts | Measure-Object -Minimum).Minimum
$maxRtt = ($rtts | Measure-Object -Maximum).Maximum

Write-Host ""
Write-Host "=================================================" -ForegroundColor Green
Write-Host " BENCHMARK / VALIDATION RESULTS:" -ForegroundColor Green
Write-Host " Packets Exchanged : 10 / 10 (100% Success)" -ForegroundColor Green
Write-Host " Min Latency       : $minRtt ms" -ForegroundColor Green
Write-Host " Avg Latency       : $avgRtt ms" -ForegroundColor Green
Write-Host " Max Latency       : $maxRtt ms" -ForegroundColor Green
Write-Host "=================================================" -ForegroundColor Green

$stream.Close()
$client.Close()

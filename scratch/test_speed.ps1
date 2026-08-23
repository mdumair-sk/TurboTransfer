param(
    [string]$Ssid = "AndroidShare_3938",
    [string]$Passphrase = "yk3jv8ge9yd228j",
    [int]$SizeBytes = 52428800 # 50 MB
)

$InterfaceName = "Wi-Fi"

Write-Host "================================================================" -ForegroundColor Cyan
Write-Host " TurboTransfer - High-Speed Throughput Benchmark ($([math]::Round($SizeBytes / 1MB, 1)) MB)" -ForegroundColor Cyan
Write-Host "================================================================" -ForegroundColor Cyan

# 1. Connect to Hotspot
$profileXml = @"
<?xml version="1.0"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
    <name>$Ssid</name>
    <SSIDConfig>
        <SSID>
            <name>$Ssid</name>
        </SSID>
    </SSIDConfig>
    <connectionType>ESS</connectionType>
    <connectionMode>manual</connectionMode>
    <MSM>
        <security>
            <authEncryption>
                <authentication>WPA2PSK</authentication>
                <encryption>AES</encryption>
                <useOneX>false</useOneX>
            </authEncryption>
            <sharedKey>
                <keyType>passPhrase</keyType>
                <protected>false</protected>
                <keyMaterial>$Passphrase</keyMaterial>
            </sharedKey>
        </security>
    </MSM>
</WLANProfile>
"@

$tempXml = [System.IO.Path]::Combine($env:TEMP, "tt_speed.xml")
[System.IO.File]::WriteAllText($tempXml, $profileXml)

netsh wlan add profile filename="$tempXml" user=current interface="$InterfaceName" | Out-Null
netsh wlan connect name="$Ssid" ssid="$Ssid" interface="$InterfaceName" | Out-Null

Write-Host "Waiting 3 seconds for Wi-Fi association..."
Start-Sleep -Seconds 3

# 2. Get Android Gateway IP
$gateway = (Get-NetIPConfiguration -InterfaceAlias $InterfaceName -ErrorAction SilentlyContinue).IPv4DefaultGateway.NextHop
if (-not $gateway) {
    # Fallback to known subnets
    $gateway = "10.18.163.130"
}
Write-Host "Android Target Gateway IP: $gateway" -ForegroundColor Green

$port = 9876
Write-Host "Connecting TCP Socket to $gateway`:$port..." -ForegroundColor Yellow

$client = New-Object System.Net.Sockets.TcpClient
$client.SendBufferSize = 4 * 1024 * 1024
$client.ReceiveBufferSize = 4 * 1024 * 1024
$client.NoDelay = $true
$client.Connect($gateway, $port)

$stream = $client.GetStream()
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
$writer = New-Object System.IO.StreamWriter($stream, $utf8NoBom)
$writer.AutoFlush = $true
$reader = New-Object System.IO.StreamReader($stream, $utf8NoBom)

# 3. Handshake
$writer.WriteLine("STREAM_TEST:$SizeBytes")
$ready = $reader.ReadLine()
Write-Host "Android Server Handshake: '$ready'" -ForegroundColor Green

if ($ready -ne "READY") {
    Write-Host "Server was not ready: $ready" -ForegroundColor Red
    $client.Close()
    exit 1
}

# 4. Stream binary data
Write-Host "Streaming $([math]::Round($SizeBytes / 1MB, 1)) MB of binary data over Wi-Fi..." -ForegroundColor Yellow

$chunkSize = 64 * 1024 # 64 KB chunks
$buffer = New-Object byte[] $chunkSize
$rand = New-Object System.Random
$rand.NextBytes($buffer) # Random payload

$sent = 0
$sw = [System.Diagnostics.Stopwatch]::StartNew()

while ($sent -lt $SizeBytes) {
    $toSend = [math]::Min($chunkSize, $SizeBytes - $sent)
    $stream.Write($buffer, 0, $toSend)
    $sent += $toSend
}
$stream.Flush()

# 5. Read result from Android
$resultLine = $reader.ReadLine()
$sw.Stop()

$clientElapsed = $sw.Elapsed.TotalSeconds
$clientSpeedMB = ($SizeBytes / 1MB) / $clientElapsed
$clientSpeedMbps = ($SizeBytes * 8 / 1000000.0) / $clientElapsed

Write-Host ""
Write-Host "================================================================" -ForegroundColor Green
Write-Host " THROUGHPUT BENCHMARK RESULTS:" -ForegroundColor Green
Write-Host " Data Transferred : $([math]::Round($SizeBytes / 1MB, 2)) MB" -ForegroundColor Green
Write-Host " Client Elapsed   : $([math]::Round($clientElapsed, 2)) seconds" -ForegroundColor Green
Write-Host " Transfer Speed   : $([math]::Round($clientSpeedMB, 2)) MB/s  ($([math]::Round($clientSpeedMbps, 1)) Mbps)" -ForegroundColor Green
Write-Host " Server Report    : $resultLine" -ForegroundColor Green
Write-Host "================================================================" -ForegroundColor Green

$client.Close()

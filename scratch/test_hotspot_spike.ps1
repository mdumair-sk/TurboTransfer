$Ssid = "AndroidShare_5874"
$Passphrase = "z2affkh783z233d"
$AndroidIp = "192.168.43.1"
$Port = 9876
$InterfaceName = "Wi-Fi"

Write-Host "=================================================================" -ForegroundColor Cyan
Write-Host " TurboTransfer - Testing Connection to $Ssid" -ForegroundColor Cyan
Write-Host "=================================================================" -ForegroundColor Cyan

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

$tempXmlPath = [System.IO.Path]::Combine($env:TEMP, "tt_hotspot.xml")
[System.IO.File]::WriteAllText($tempXmlPath, $profileXml)

netsh wlan add profile filename="$tempXmlPath" user=current interface="$InterfaceName"
Write-Host "Connecting Windows Wi-Fi to '$Ssid'..."
netsh wlan connect name="$Ssid" ssid="$Ssid" interface="$InterfaceName"

Write-Host "Waiting 4 seconds for DHCP assignment on Wi-Fi..."
Start-Sleep -Seconds 4

$assignedIps = Get-NetIPAddress -InterfaceAlias $InterfaceName -AddressFamily IPv4 -ErrorAction SilentlyContinue | Select-Object -ExpandProperty IPAddress
Write-Host "Wi-Fi Interface IP(s): $($assignedIps -join ', ')" -ForegroundColor Green

Write-Host "Connecting raw TCP Socket to Android ($AndroidIp`:$Port)..." -ForegroundColor Yellow

$successCount = 0
for ($p = 1; $p -le 5; $p++) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    try {
        $client = New-Object System.Net.Sockets.TcpClient
        $client.ReceiveTimeout = 3000
        $client.SendTimeout = 3000
        $asyncConnect = $client.BeginConnect($AndroidIp, $Port, $null, $null)
        if ($asyncConnect.AsyncWaitHandle.WaitOne(3000, $false)) {
            $client.EndConnect($asyncConnect)
            $stream = $client.GetStream()
            $writer = New-Object System.IO.StreamWriter($stream, [System.Text.Encoding]::UTF8)
            $writer.AutoFlush = $true
            $reader = New-Object System.IO.StreamReader($stream, [System.Text.Encoding]::UTF8)

            $payload = "TURBO_PING_TEST_$p"
            $writer.WriteLine($payload)
            $reply = $reader.ReadLine()
            $sw.Stop()

            Write-Host "  [#$p] Send: '$payload' -> Reply: '$reply' (RTT: $([math]::Round($sw.Elapsed.TotalMilliseconds, 1)) ms)" -ForegroundColor Green
            $successCount++

            $stream.Close()
            $client.Close()
        } else {
            Write-Host "  [#$p] Connection timeout to $AndroidIp`:$Port" -ForegroundColor Red
            $client.Close()
        }
    } catch {
        Write-Host "  [#$p] Failed: $_" -ForegroundColor Red
    }
    Start-Sleep -Milliseconds 300
}

Write-Host ""
Write-Host "=================================================================" -ForegroundColor Cyan
if ($successCount -gt 0) {
    Write-Host " RESULT: >>> GO (LOCAL HOTSPOT VALIDATED) <<<" -ForegroundColor Green
    Write-Host " Successfully connected from Windows over standard Wi-Fi to Android!" -ForegroundColor Green
    Write-Host " TCP echo exchanges: $successCount / 5 successful" -ForegroundColor Green
} else {
    Write-Host " RESULT: No successful TCP echo packets." -ForegroundColor Red
}
Write-Host "=================================================================" -ForegroundColor Cyan

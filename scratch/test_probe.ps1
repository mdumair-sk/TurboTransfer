$Ssid = "DIRECT-Yf-OnePlus 13s"
$Passphrase = "Ad56byH5"
$AndroidIp = "192.168.49.1"
$Port = 9876
$InterfaceName = "Wi-Fi"

Write-Host "Adding profile with nonBroadcast=true for directed probe requests..." -ForegroundColor Cyan

$profileXml = @"
<?xml version="1.0"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
    <name>$Ssid</name>
    <SSIDConfig>
        <SSID>
            <name>$Ssid</name>
        </SSID>
        <nonBroadcast>true</nonBroadcast>
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

$tempXmlPath = [System.IO.Path]::Combine($env:TEMP, "tt_spike_probe.xml")
[System.IO.File]::WriteAllText($tempXmlPath, $profileXml)

netsh wlan add profile filename="$tempXmlPath" user=current interface="$InterfaceName"
Write-Host "Connecting to '$Ssid'..."
netsh wlan connect name="$Ssid" ssid="$Ssid" interface="$InterfaceName"

Write-Host "Polling for Wi-Fi association on '$InterfaceName'..."
$sw = [System.Diagnostics.Stopwatch]::StartNew()
for ($i = 1; $i -le 25; $i++) {
    Start-Sleep -Seconds 1
    $ipList = Get-NetIPAddress -InterfaceAlias $InterfaceName -AddressFamily IPv4 -ErrorAction SilentlyContinue |
              Where-Object { $_.IPAddress -like "192.168.49.*" }

    if ($ipList) {
        $assignedIp = $ipList[0].IPAddress
        Write-Host "CONNECTED! Assigned Windows IP: $assignedIp (took $($sw.Elapsed.TotalSeconds)s)" -ForegroundColor Green
        break
    } else {
        $allIps = (Get-NetIPAddress -InterfaceAlias $InterfaceName -AddressFamily IPv4 -ErrorAction SilentlyContinue | Select-Object -ExpandProperty IPAddress) -join ", "
        Write-Host "Waiting... ($i s) [Current IPs on Wi-Fi: $allIps]" -ForegroundColor Gray
    }
}

Write-Host "Testing TCP Socket to $AndroidIp`:$Port..."
try {
    $tcpClient = New-Object System.Net.Sockets.TcpClient
    $tcpClient.ReceiveTimeout = 3000
    $tcpClient.SendTimeout = 3000
    $asyncConnect = $tcpClient.BeginConnect($AndroidIp, $Port, $null, $null)
    $wait = $asyncConnect.AsyncWaitHandle.WaitOne(3000, $false)
    if ($wait) {
        $tcpClient.EndConnect($asyncConnect)
        $stream = $tcpClient.GetStream()
        $writer = New-Object System.IO.StreamWriter($stream, [System.Text.Encoding]::UTF8)
        $writer.AutoFlush = $true
        $reader = New-Object System.IO.StreamReader($stream, [System.Text.Encoding]::UTF8)

        $writer.WriteLine("TURBO_PING_TEST_DIRECT")
        $reply = $reader.ReadLine()
        Write-Host "SUCCESS! Received TCP Echo Reply: '$reply'" -ForegroundColor Green
        $stream.Close()
        $tcpClient.Close()
    } else {
        Write-Host "TCP Connect timeout to $AndroidIp`:$Port" -ForegroundColor Red
    }
} catch {
    Write-Host "TCP Connect error: $_" -ForegroundColor Red
}

Write-Host "Checking recent WLAN AutoConfig event..."
Get-WinEvent -LogName 'Microsoft-Windows-WLAN-AutoConfig/Operational' -MaxEvents 3 -ErrorAction SilentlyContinue | Select-Object TimeCreated, Id, Message | Format-List

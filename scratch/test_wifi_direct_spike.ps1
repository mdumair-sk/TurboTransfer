[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Ssid,

    [Parameter(Mandatory = $true)]
    [string]$Passphrase,

    [Parameter(Mandatory = $false)]
    [string]$AndroidIp = "192.168.49.1",

    [Parameter(Mandatory = $false)]
    [int]$Port = 9876,

    [Parameter(Mandatory = $false)]
    [string]$InterfaceName = "Wi-Fi",

    [Parameter(Mandatory = $false)]
    [int]$PingCount = 5
)

$ErrorActionPreference = "Continue"

Write-Host "=================================================================" -ForegroundColor Cyan
Write-Host " TurboTransfer - Milestone 7a Wi-Fi Direct Spike (Approach 1)" -ForegroundColor Cyan
Write-Host "=================================================================" -ForegroundColor Cyan
Write-Host " Target SSID        : $Ssid"
Write-Host " Target Passphrase  : $Passphrase"
Write-Host " Android GO IP      : $AndroidIp"
Write-Host " Echo TCP Port      : $Port"
Write-Host " Wi-Fi Interface    : $InterfaceName"
Write-Host "================================================================="

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

$tempXmlPath = [System.IO.Path]::Combine($env:TEMP, "tt_spike_profile.xml")
[System.IO.File]::WriteAllText($tempXmlPath, $profileXml)

try {
    Write-Host ""
    Write-Host "[1/5] Adding WLAN profile via netsh..." -ForegroundColor Yellow
    $addResult = netsh wlan add profile filename="$tempXmlPath" user=current interface="$InterfaceName"
    Write-Host "      $addResult"

    Write-Host ""
    Write-Host "[2/5] Connecting to SSID '$Ssid'..." -ForegroundColor Yellow
    $connectResult = netsh wlan connect name="$Ssid" ssid="$Ssid" interface="$InterfaceName"
    Write-Host "      $connectResult"

    Write-Host ""
    Write-Host "[3/5] Waiting for Wi-Fi association and DHCP IP assignment..." -ForegroundColor Yellow
    $associated = $false
    $assignedIp = $null
    $sw = [System.Diagnostics.Stopwatch]::StartNew()

    for ($i = 1; $i -le 30; $i++) {
        Start-Sleep -Seconds 1
        $ipList = Get-NetIPAddress -InterfaceAlias $InterfaceName -AddressFamily IPv4 -ErrorAction SilentlyContinue |
                  Where-Object { $_.IPAddress -notlike "169.254.*" }

        if ($ipList) {
            $assignedIp = $ipList[0].IPAddress
            $associated = $true
            $elapsedSec = [math]::Round($sw.Elapsed.TotalSeconds, 1)
            Write-Host "      Connected! Assigned Windows IP: $assignedIp (took $elapsedSec s)" -ForegroundColor Green
            break
        } else {
            Write-Host "      Waiting for DHCP IP... ($i s)" -ForegroundColor Gray
        }
    }

    if (-not $associated) {
        Write-Host "      [WARN] Timed out waiting for DHCP address. Attempting direct socket connect anyway..." -ForegroundColor Yellow
    }

    Write-Host ""
    Write-Host "[4/5] Testing Raw TCP Socket to Android ($AndroidIp`:$Port)..." -ForegroundColor Yellow

    $successCount = 0
    $totalRtt = 0

    for ($p = 1; $p -le $PingCount; $p++) {
        $tcpClient = New-Object System.Net.Sockets.TcpClient
        $tcpClient.ReceiveTimeout = 3000
        $tcpClient.SendTimeout = 3000

        $pingSw = [System.Diagnostics.Stopwatch]::StartNew()
        try {
            $asyncConnect = $tcpClient.BeginConnect($AndroidIp, $Port, $null, $null)
            $waitSuccess = $asyncConnect.AsyncWaitHandle.WaitOne(3000, $false)
            if (-not $waitSuccess) {
                throw "Socket connect timeout to $AndroidIp`:$Port"
            }
            $tcpClient.EndConnect($asyncConnect)

            $stream = $tcpClient.GetStream()
            $writer = New-Object System.IO.StreamWriter($stream, [System.Text.Encoding]::UTF8)
            $writer.AutoFlush = $true
            $reader = New-Object System.IO.StreamReader($stream, [System.Text.Encoding]::UTF8)

            $payload = "TURBO_PING_TEST_$p"
            $writer.WriteLine($payload)

            $reply = $reader.ReadLine()
            $pingSw.Stop()
            $rttMs = [math]::Round($pingSw.Elapsed.TotalMilliseconds, 1)

            if ($reply -match "TURBO_PONG") {
                Write-Host "      [#$p] Send: '$payload' -> Reply: '$reply' (RTT: $rttMs ms)" -ForegroundColor Green
                $successCount++
                $totalRtt += $rttMs
            } else {
                Write-Host "      [#$p] Unexpected reply: '$reply'" -ForegroundColor Red
            }

            $stream.Close()
            $tcpClient.Close()
        } catch {
            $pingSw.Stop()
            Write-Host "      [#$p] Failed: $_" -ForegroundColor Red
            $tcpClient.Close()
        }

        if ($p -lt $PingCount) {
            Start-Sleep -Milliseconds 500
        }
    }

    Write-Host ""
    Write-Host "[5/5] Spike Results Summary" -ForegroundColor Cyan
    Write-Host "=================================================================" -ForegroundColor Cyan
    if ($successCount -gt 0) {
        $avgRtt = [math]::Round(($totalRtt / $successCount), 1)
        Write-Host " RESULT: >>> GO (APPROACH 1 VALIDATED) <<<" -ForegroundColor Green
        Write-Host " Successful Echo Exchanges : $successCount / $PingCount" -ForegroundColor Green
        Write-Host " Average TCP Round-Trip   : $avgRtt ms" -ForegroundColor Green
        Write-Host " Windows IP on P2P Subnet : $assignedIp" -ForegroundColor Green
        Write-Host " Android Group Owner IP   : $AndroidIp" -ForegroundColor Green
        Write-Host " Confirmation: Windows successfully joined Android Wi-Fi Direct" -ForegroundColor Green
        Write-Host "               group as standard WPA2 AP and verified raw TCP" -ForegroundColor Green
        Write-Host "               echo communication without UWP!" -ForegroundColor Green
    } else {
        Write-Host " RESULT: >>> NO-GO / INVESTIGATION NEEDED <<<" -ForegroundColor Red
        Write-Host " No successful TCP echo packets exchanged." -ForegroundColor Red
    }
    Write-Host "=================================================================" -ForegroundColor Cyan

} finally {
    Write-Host ""
    Write-Host "Cleaning up Wi-Fi profile..." -ForegroundColor Gray
    try {
        netsh wlan disconnect interface="$InterfaceName" | Out-Null
        netsh wlan delete profile name="$Ssid" interface="$InterfaceName" | Out-Null
        if (Test-Path $tempXmlPath) {
            Remove-Item $tempXmlPath -Force
        }
        Write-Host "Cleanup completed." -ForegroundColor Gray
    } catch {
        Write-Host "Error during cleanup: $_" -ForegroundColor Yellow
    }
}

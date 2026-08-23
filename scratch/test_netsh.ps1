$profileXml = @"
<?xml version="1.0"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
    <name>TestSpikeProfile</name>
    <SSIDConfig>
        <SSID>
            <name>TestSpikeProfile</name>
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
                <keyMaterial>12345678</keyMaterial>
            </sharedKey>
        </security>
    </MSM>
</WLANProfile>
"@

$xmlPath = Join-Path $PSScriptRoot "test_profile.xml"
[System.IO.File]::WriteAllText($xmlPath, $profileXml)
Write-Host "Adding profile..."
netsh wlan add profile filename="$xmlPath" user=current
Write-Host "Showing profile..."
netsh wlan show profile name="TestSpikeProfile"
Write-Host "Deleting profile..."
netsh wlan delete profile name="TestSpikeProfile"
Remove-Item $xmlPath
Write-Host "Test completed successfully."

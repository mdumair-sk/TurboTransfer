use std::time::Duration;
use turbotransfer_core::protocol::{HeartbeatData, Message};
use turbotransfer_core::transport::wifi_direct::{WifiDirectConfig, WifiDirectTransport};
use turbotransfer_core::transport::{Transport, TransportStatus};

#[tokio::test]
#[ignore]
async fn test_live_wifi_direct_transport_end_to_end() {
    let ssid = std::env::var("TT_TEST_SSID").unwrap_or_else(|_| "AndroidShare_2745".to_string());
    let passphrase = std::env::var("TT_TEST_PASS").unwrap_or_else(|_| "cxh77vh4afjn5g9".to_string());
    let target_ip = std::env::var("TT_TEST_IP").unwrap_or_else(|_| "10.18.163.130".to_string());
    let port = 9876;

    println!("============================================================");
    println!(" TurboTransfer Live Wi-Fi Direct Transport Verification");
    println!(" SSID: {}, Target IP: {}:{}", ssid, target_ip, port);
    println!("============================================================");

    let mut config = WifiDirectConfig::new(&ssid, &passphrase, &target_ip, port);
    config.heartbeat_timeout = Duration::from_secs(5); // 5s for fast test verification
    config.reconnect_interval = Duration::from_secs(2);

    println!("[1/5] Connecting via WifiDirectTransport::connect()...");
    let mut transport = WifiDirectTransport::connect(config)
        .await
        .expect("Failed to connect WifiDirectTransport");

    assert_eq!(transport.status(), TransportStatus::Connected);
    assert!(transport.is_connected());
    println!("[1/5] SUCCESS: Transport connected! Status: {}", transport.status());

    println!("[2/5] Sending 5 Heartbeat messages over 5GHz Wi-Fi...");
    for i in 1..=5 {
        let msg = Message::Heartbeat(HeartbeatData { sequence: i });
        transport.send_frame(&msg).await.expect("Failed to send heartbeat frame");
        println!("  -> Sent Heartbeat frame #{}, Total bytes sent: {}", i, transport.bytes_sent());
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(transport.bytes_sent() > 0);
    println!("[2/5] SUCCESS: All frames transmitted!");

    println!("[3/5] Checking heartbeat liveness detector (active window)...");
    let is_alive = transport.check_heartbeat_liveness().await;
    assert!(is_alive);
    println!("[3/5] SUCCESS: Transport is alive and verified!");

    println!("[4/5] Testing heartbeat silence failure detection (> 5s silence)...");
    println!("  Waiting 6 seconds without receiving frames...");
    tokio::time::sleep(Duration::from_secs(6)).await;
    let is_alive_after_silence = transport.check_heartbeat_liveness().await;
    assert!(!is_alive_after_silence);
    assert_eq!(transport.status(), TransportStatus::Disconnected);
    println!("[4/5] SUCCESS: Transport correctly transitioned to Disconnected after silence!");

    println!("[5/5] Testing automatic reconnect state machine...");
    let reconnect_result = transport.reconnect(3).await;
    assert!(reconnect_result.is_ok());
    assert_eq!(transport.status(), TransportStatus::Connected);
    println!("[5/5] SUCCESS: Transport successfully reconnected! Status: {}", transport.status());

    println!("Cleaning up transport and WLAN profile...");
    transport.close().await.expect("Failed to close transport");
    assert_eq!(transport.status(), TransportStatus::Disconnected);
    println!("============================================================");
    println!(" ALL 5/5 LIVE WI-FI DIRECT VERIFICATIONS PASSED!");
    println!("============================================================");
}

use turbotransfer_core::protocol::{HeartbeatData, Message};
use turbotransfer_core::transport::usb::{UsbTransport, UsbTransportConfig};
use turbotransfer_core::transport::{Transport, TransportKind, TransportStatus};

#[tokio::test]
#[ignore]
async fn test_live_usb_transport_end_to_end() {
    println!("============================================================");
    println!(" TurboTransfer Live USB (ADB Tunnel) Transport Verification");
    println!("============================================================");

    println!("[1/4] Listing attached ADB devices...");
    let devices = UsbTransport::list_adb_devices().expect("Failed to list ADB devices");
    for d in &devices {
        println!("  -> Device found: serial='{}', state='{}', model={:?}", d.serial, d.state, d.model);
    }
    assert!(!devices.is_empty(), "Expected at least 1 ADB device attached");

    println!("\n[2/4] Connecting via UsbTransport::connect()...");
    let config = UsbTransportConfig::new(9876, 9876);
    let mut transport = UsbTransport::connect(config)
        .await
        .expect("Failed to establish UsbTransport");

    assert_eq!(transport.kind(), TransportKind::Usb);
    assert_eq!(transport.status(), TransportStatus::Connected);
    assert!(transport.is_connected());
    println!("  -> SUCCESS: UsbTransport connected! Serial: {:?}, Status: {}", transport.active_serial(), transport.status());

    println!("\n[3/4] Transmitting Heartbeat frames over USB transport...");
    for i in 1..=5 {
        let hb = Message::Heartbeat(HeartbeatData { sequence: i });
        transport.send_frame(&hb).await.expect("Failed to send frame over USB");
        println!("  -> Sent frame #{}, Total bytes sent: {}", i, transport.bytes_sent());
    }
    assert!(transport.bytes_sent() > 0);
    println!("  -> SUCCESS: Frame transmission verified!");

    println!("\n[4/4] Closing USB transport and cleaning up ADB forward rule...");
    transport.close().await.expect("Failed to close USB transport");
    assert_eq!(transport.status(), TransportStatus::Disconnected);
    assert!(!transport.is_connected());
    println!("  -> SUCCESS: USB transport cleanly closed!");

    println!("============================================================");
    println!(" ALL 4/4 LIVE USB TRANSPORT VERIFICATIONS PASSED!");
    println!("============================================================");
}

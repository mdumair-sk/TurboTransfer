pub mod adb;
pub mod transport;

pub use adb::*;
pub use transport::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{HelloData, Message};
    use crate::transport::{Transport, TransportKind, TransportStatus};
    use std::time::Duration;
    use tokio::net::{TcpListener, TcpStream};
    use uuid::Uuid;

    #[test]
    fn test_adb_devices_output_parser() {
        let sample_output = r#"
List of devices attached
b9b2c03f	device product:OnePlus13s model:PJZ110 device:OnePlus13s transport_id:1
emulator-5554	offline transport_id:2
192.168.1.50:5555	unauthorized transport_id:3
"#;

        let devices = AdbDeviceInfo::parse_adb_devices_output(sample_output);
        assert_eq!(devices.len(), 3);

        assert_eq!(devices[0].serial, "b9b2c03f");
        assert_eq!(devices[0].state, "device");
        assert_eq!(devices[0].product.as_deref(), Some("OnePlus13s"));
        assert_eq!(devices[0].model.as_deref(), Some("PJZ110"));

        assert_eq!(devices[1].serial, "emulator-5554");
        assert_eq!(devices[1].state, "offline");

        assert_eq!(devices[2].serial, "192.168.1.50:5555");
        assert_eq!(devices[2].state, "unauthorized");
    }

    #[tokio::test]
    async fn test_usb_transport_framing_and_lifecycle() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(addr).await.unwrap();
        let (server_stream, _) = listener.accept().await.unwrap();

        let config = UsbTransportConfig::new(addr.port(), addr.port());
        let mut client = UsbTransport::from_stream(client_stream, config.clone());
        let mut server = UsbTransport::from_stream(server_stream, config);

        assert_eq!(client.kind(), TransportKind::Usb);
        assert_eq!(client.status(), TransportStatus::Connected);
        assert!(client.is_connected());

        // Send Hello
        let hello = Message::Hello(HelloData {
            device_id: uuid::Uuid::nil(),
            device_name: "TestDevice".to_string(),
            protocol_version: 1,
        });
        client.send_frame(&hello).await.unwrap();
        assert!(client.bytes_sent() > 0);

        let received = server.receive_frame().await.unwrap().unwrap();
        match received {
            Message::Hello(msg) => assert_eq!(msg.protocol_version, 1),
            _ => panic!("Expected Hello message"),
        }

        // Close
        client.close().await.unwrap();
        assert_eq!(client.status(), TransportStatus::Disconnected);
        assert!(!client.is_connected());

        // Subsequent sends must fail
        assert!(client.send_frame(&hello).await.is_err());
    }

    #[tokio::test]
    #[ignore]
    async fn test_usb_transport_reconnect_retry_state_machine() {
        let mut config = UsbTransportConfig::new(9999, 9999);
        config.device_serial = Some("mock-nonexistent-serial".to_string());
        config.reconnect_interval = Duration::from_millis(10);
        config.handshake_timeout = Duration::from_millis(50);

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let client_stream = TcpStream::connect(addr).await.unwrap();
        let (_server_stream, _) = listener.accept().await.unwrap();

        let mut transport = UsbTransport::from_stream(client_stream, config);
        transport.status = TransportStatus::Disconnected;
        assert!(!transport.is_connected());

        let local_hello = crate::protocol::HelloData {
            device_id: Uuid::new_v4(),
            device_name: "TestPC".to_string(),
            protocol_version: 1,
        };

        let res = transport.reconnect(2, &local_hello).await;
        assert!(res.is_err());
        assert_eq!(transport.status(), TransportStatus::Failed);
    }
}

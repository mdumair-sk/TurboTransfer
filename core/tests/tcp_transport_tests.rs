use std::io::Write;
use tempfile::tempdir;
use tokio::time::{sleep, Duration};
use turbotransfer_core::protocol::*;
use turbotransfer_core::transfer::{
    receive_file_session, send_file_session, InMemoryChunkTracker,
};
use turbotransfer_core::transport::{
    TcpListenerTransport, TcpTransport, Transport, TransportError, TransportKind, TransportStatus,
};
use uuid::Uuid;

#[tokio::test]
async fn test_tcp_transport_direct_frames() {
    let listener = TcpListenerTransport::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind TCP listener");
    let local_addr = listener.local_addr().expect("Failed to get local addr");

    let server_task = tokio::spawn(async move {
        let (mut server_transport, peer_addr) =
            listener.accept().await.expect("Accept failed");
        assert_eq!(server_transport.kind(), TransportKind::Tcp);
        assert_eq!(server_transport.status(), TransportStatus::Connected);
        assert!(server_transport.is_connected());
        assert_eq!(server_transport.peer_addr(), Some(peer_addr));

        // Read Hello from client
        let msg = server_transport
            .receive_frame()
            .await
            .expect("Receive failed")
            .expect("Expected frame");
        match msg {
            Message::Hello(hello) => {
                assert_eq!(hello.device_name, "ClientDevice");
            }
            other => panic!("Unexpected frame: {:?}", other),
        }

        // Send Server Hello
        let server_hello = Message::Hello(HelloData {
            device_id: Uuid::new_v4(),
            device_name: "ServerDevice".into(),
            protocol_version: 1,
        });
        server_transport
            .send_frame(&server_hello)
            .await
            .expect("Send failed");

        assert!(server_transport.bytes_sent() > 0);
        assert!(server_transport.bytes_received() > 0);

        server_transport.close().await.expect("Close failed");
        assert_eq!(server_transport.status(), TransportStatus::Disconnected);
    });

    let client_task = tokio::spawn(async move {
        let mut client_transport = TcpTransport::connect(&local_addr.to_string())
            .await
            .expect("Connect failed");
        assert_eq!(client_transport.kind(), TransportKind::Tcp);
        assert_eq!(client_transport.status(), TransportStatus::Connected);
        assert!(client_transport.is_connected());

        // Send Client Hello
        let client_hello = Message::Hello(HelloData {
            device_id: Uuid::new_v4(),
            device_name: "ClientDevice".into(),
            protocol_version: 1,
        });
        client_transport
            .send_frame(&client_hello)
            .await
            .expect("Send failed");

        // Read Server Hello
        let msg = client_transport
            .receive_frame()
            .await
            .expect("Receive failed")
            .expect("Expected frame");
        match msg {
            Message::Hello(hello) => {
                assert_eq!(hello.device_name, "ServerDevice");
            }
            other => panic!("Unexpected frame: {:?}", other),
        }

        assert!(client_transport.bytes_sent() > 0);
        assert!(client_transport.bytes_received() > 0);

        client_transport.close().await.expect("Close failed");
        assert_eq!(client_transport.status(), TransportStatus::Disconnected);
    });

    let (s_res, c_res) = tokio::join!(server_task, client_task);
    s_res.unwrap();
    c_res.unwrap();
}

#[tokio::test]
async fn test_tcp_transport_wildcard_binding_and_file_transfer() {
    let temp_dir = tempdir().unwrap();
    let src_dir = temp_dir.path().join("src");
    let dest_dir = temp_dir.path().join("dest");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&dest_dir).unwrap();

    let src_file = src_dir.join("wildcard_test.bin");
    let test_data: Vec<u8> = (0..(1024 * 1024)).map(|i| (i % 251) as u8).collect();
    {
        let mut f = std::fs::File::create(&src_file).unwrap();
        f.write_all(&test_data).unwrap();
        f.flush().unwrap();
    }

    // Bind on 0.0.0.0 (all OS interfaces)
    let listener = TcpListenerTransport::bind("0.0.0.0:0")
        .await
        .expect("Failed to bind 0.0.0.0");
    let port = listener.local_addr().unwrap().port();
    let connect_addr = format!("127.0.0.1:{}", port);

    let src_file_clone = src_file.clone();
    let dest_dir_clone = dest_dir.clone();

    let receiver_task = tokio::spawn(async move {
        let (transport, _) = listener.accept().await.expect("Accept failed");
        let mut tracker = InMemoryChunkTracker::new();
        receive_file_session(
            Uuid::new_v4(),
            "WildcardReceiver",
            &dest_dir_clone,
            &mut tracker,
            transport,
        )
        .await
    });

    let sender_task = tokio::spawn(async move {
        let transport = TcpTransport::connect(&connect_addr)
            .await
            .expect("Connect failed");
        send_file_session(
            Uuid::new_v4(),
            "WildcardSender",
            &src_file_clone,
            128 * 1024,
            Uuid::new_v4(),
            transport,
            None,
            None,
        )
        .await
    });

    let (rec_res, send_res) = tokio::join!(receiver_task, sender_task);
    send_res.unwrap().expect("Sender session failed");
    let output_file = rec_res.unwrap().expect("Receiver session failed");

    let received_bytes = std::fs::read(&output_file).unwrap();
    assert_eq!(received_bytes.len(), test_data.len());
    assert_eq!(received_bytes, test_data);
}

#[tokio::test]
async fn test_tcp_transport_bidirectional_transfers() {
    let temp_dir = tempdir().unwrap();
    let dir_a = temp_dir.path().join("dir_a");
    let dir_b = temp_dir.path().join("dir_b");
    std::fs::create_dir_all(&dir_a).unwrap();
    std::fs::create_dir_all(&dir_b).unwrap();

    let data_a_to_b = vec![0xAA; 512 * 1024];
    let data_b_to_a = vec![0xBB; 768 * 1024];

    let file_a = dir_a.join("from_a.bin");
    let file_b = dir_b.join("from_b.bin");
    std::fs::write(&file_a, &data_a_to_b).unwrap();
    std::fs::write(&file_b, &data_b_to_a).unwrap();

    // Direction 1: Node A -> Node B
    {
        let listener = TcpListenerTransport::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let dest = dir_b.clone();
        let src = file_a.clone();

        let rec = tokio::spawn(async move {
            let (transport, _) = listener.accept().await.unwrap();
            let mut tracker = InMemoryChunkTracker::new();
            receive_file_session(Uuid::new_v4(), "NodeB", &dest, &mut tracker, transport).await
        });

        let send = tokio::spawn(async move {
            let transport = TcpTransport::connect(&addr).await.unwrap();
            send_file_session(Uuid::new_v4(), "NodeA", &src, 64 * 1024, Uuid::new_v4(), transport, None, None).await
        });

        let (r, s) = tokio::join!(rec, send);
        s.unwrap().unwrap();
        let out = r.unwrap().unwrap();
        assert_eq!(std::fs::read(out).unwrap(), data_a_to_b);
    }

    // Direction 2: Node B -> Node A
    {
        let listener = TcpListenerTransport::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        let dest = dir_a.clone();
        let src = file_b.clone();

        let rec = tokio::spawn(async move {
            let (transport, _) = listener.accept().await.unwrap();
            let mut tracker = InMemoryChunkTracker::new();
            receive_file_session(Uuid::new_v4(), "NodeA", &dest, &mut tracker, transport).await
        });

        let send = tokio::spawn(async move {
            let transport = TcpTransport::connect(&addr).await.unwrap();
            send_file_session(Uuid::new_v4(), "NodeB", &src, 64 * 1024, Uuid::new_v4(), transport, None, None).await
        });

        let (r, s) = tokio::join!(rec, send);
        s.unwrap().unwrap();
        let out = r.unwrap().unwrap();
        assert_eq!(std::fs::read(out).unwrap(), data_b_to_a);
    }
}

#[tokio::test]
async fn test_tcp_transport_disconnect_handling() {
    let listener = TcpListenerTransport::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut transport, _) = listener.accept().await.unwrap();
        // Abruptly close server transport
        transport.close().await.unwrap();
    });

    let client = tokio::spawn(async move {
        let mut transport = TcpTransport::connect(&addr).await.unwrap();
        // Wait briefly for server to close
        sleep(Duration::from_millis(50)).await;

        // Next receive or send should detect disconnect
        let frame = transport.receive_frame().await;
        match frame {
            Ok(None) => {} // Clean EOF
            Err(TransportError::Disconnected(_)) => {} // Disconnect error
            Err(TransportError::Io(_)) => {} // Socket reset
            other => panic!("Expected disconnect indication, got {:?}", other),
        }
        assert_eq!(transport.status(), TransportStatus::Disconnected);
        assert!(!transport.is_connected());
    });

    let (s, c) = tokio::join!(server, client);
    s.unwrap();
    c.unwrap();
}

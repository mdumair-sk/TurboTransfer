use tokio::io::AsyncWriteExt;
use turbotransfer_core::protocol::*;
use uuid::Uuid;

fn sample_messages() -> Vec<Message> {
    let t_id = Uuid::new_v4();
    let f_id = Uuid::new_v4();
    let d_id = Uuid::new_v4();

    vec![
        Message::Hello(HelloData {
            device_id: d_id,
            device_name: "Windows Desktop".into(),
            protocol_version: 1,
        }),
        Message::TransferOffer(TransferOfferData {
            transfer_id: t_id,
            file_id: f_id,
            file_name: "movie.mkv".into(),
            file_size: 13_639_045_120,
            chunk_size: 67_108_864,
            total_chunks: 203,
            checksum_algo: "xxhash64".into(),
        }),
        Message::TransferAccept(TransferAcceptData {
            transfer_id: t_id,
            resume_from: Some(vec![(0, 10), (15, 20)]),
        }),
        Message::TransferReject(TransferRejectData {
            transfer_id: t_id,
            reason: "User rejected transfer".into(),
        }),
        Message::ChunkData(ChunkDataPayload {
            transfer_id: t_id,
            file_id: f_id,
            chunk_id: 5,
            file_offset: 335_544_320,
            payload_length: 4,
            checksum: 0xFEEDFACECAFEBEEF,
            payload: vec![10, 20, 30, 40],
        }),
        Message::ChunkAck(ChunkAckData {
            transfer_id: t_id,
            chunk_id: 5,
            receiver_verify_us: Some(1500),
        }),
        Message::ChunkNack(ChunkNackData {
            transfer_id: t_id,
            chunk_id: 6,
            reason: "xxhash64 mismatch".into(),
        }),
        Message::Pause(PauseData { transfer_id: t_id }),
        Message::Resume(ResumeData { transfer_id: t_id }),
        Message::Cancel(CancelData { transfer_id: t_id }),
        Message::Complete(CompleteData {
            transfer_id: t_id,
            file_checksum: 0x87654321,
        }),
        Message::Heartbeat(HeartbeatData { sequence: 42 }),
        Message::BatchChunkAck(BatchChunkAckData {
            transfer_id: t_id,
            chunk_ids: vec![1, 2, 3],
            sum_receiver_verify_us: Some(4200),
        }),
    ]
}

#[test]
fn test_roundtrip_all_message_types() {
    let messages = sample_messages();
    assert_eq!(messages.len(), 13);

    for original in &messages {
        let encoded = encode_frame(original).expect("Encode should succeed");
        let decoded = decode_frame(&encoded).expect("Decode should succeed");
        assert_eq!(original, &decoded, "Message roundtrip failed for type 0x{:02X}", original.message_type());
    }
}

#[test]
fn test_legacy_chunk_ack_wire_compatibility() {
    let t_id = Uuid::new_v4();
    // Simulate legacy 20-byte payload: transfer_id (16) + chunk_id (4) without Option<u32>
    let mut legacy_payload = bincode::serialize(&t_id).unwrap();
    legacy_payload.extend_from_slice(&5u32.to_le_bytes());
    assert_eq!(legacy_payload.len(), 20);

    let decoded = Message::decode_payload(MSG_TYPE_CHUNK_ACK, &legacy_payload).expect("Must decode legacy ChunkAck");
    match decoded {
        Message::ChunkAck(ack) => {
            assert_eq!(ack.transfer_id, t_id);
            assert_eq!(ack.chunk_id, 5);
            assert_eq!(ack.receiver_verify_us, None);
        }
        _ => panic!("Expected Message::ChunkAck"),
    }
}

#[tokio::test]
async fn test_frame_reader_tiny_chunk_partial_reads() {
    let messages = sample_messages();
    let mut all_bytes = Vec::new();
    for msg in &messages {
        let encoded = encode_frame(msg).expect("Encode failed");
        all_bytes.extend_from_slice(&encoded);
    }

    let (client, server) = tokio::io::duplex(1024);

    // Writer task: feeds bytes 1 to 3 bytes at a time
    let writer_handle = tokio::spawn(async move {
        let mut writer = client;
        let mut offset = 0;
        let chunk_sizes = [1, 2, 3, 1, 3, 2];
        let mut idx = 0;

        while offset < all_bytes.len() {
            let size = chunk_sizes[idx % chunk_sizes.len()];
            let end = (offset + size).min(all_bytes.len());
            writer.write_all(&all_bytes[offset..end]).await.unwrap();
            writer.flush().await.unwrap();
            offset = end;
            idx += 1;
            tokio::task::yield_now().await;
        }
    });

    // Reader task: reads messages via FrameReader
    let mut frame_reader = FrameReader::new(server);
    let mut received = Vec::new();

    for _ in 0..messages.len() {
        let msg = frame_reader
            .read_frame()
            .await
            .expect("Read frame failed")
            .expect("Expected message, got EOF");
        received.push(msg);
    }

    writer_handle.await.unwrap();
    assert_eq!(messages, received);
}

#[test]
fn test_malformed_and_truncated_frames() {
    // 1. Truncated header (< 5 bytes)
    let slice = &[0x05, 0x00, 0x00];
    assert!(matches!(
        decode_frame(slice),
        Err(ProtocolError::TruncatedFrame { .. })
    ));

    // 2. Unknown message type code (e.g. 0xFF)
    let payload = vec![0x02, 0x00, 0x00, 0x00, 0xFF, 0x00];
    assert!(matches!(
        decode_frame(&payload),
        Err(ProtocolError::InvalidMessageType(0xFF))
    ));

    // 3. Payload length mismatch (header claims 10 bytes, but slice ends)
    let mut header = (10u32).to_le_bytes().to_vec();
    header.push(MSG_TYPE_HEARTBEAT); // type byte
    header.extend_from_slice(&[1, 2, 3]); // only 3 bytes payload instead of 9
    assert!(matches!(
        decode_frame(&header),
        Err(ProtocolError::TruncatedFrame { .. })
    ));

    // 4. Invalid bincode payload for variant
    let mut bad_payload = (5u32).to_le_bytes().to_vec();
    bad_payload.push(MSG_TYPE_HEARTBEAT);
    bad_payload.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]); // invalid bincode for u64
    assert!(matches!(
        decode_frame(&bad_payload),
        Err(ProtocolError::DeserializationError(_))
    ));
}

#[tokio::test]
async fn test_frame_reader_exceeds_max_size() {
    let (mut client, server) = tokio::io::duplex(64);

    // Frame claiming 1000 bytes payload length
    let mut header = (1000u32).to_le_bytes().to_vec();
    header.push(MSG_TYPE_HEARTBEAT);

    tokio::spawn(async move {
        client.write_all(&header).await.unwrap();
    });

    let mut reader = FrameReader::with_max_frame_size(server, 100); // max size 100
    let res = reader.read_frame().await;
    assert!(matches!(res, Err(ProtocolError::FrameTooLarge(1000, 100))));
}

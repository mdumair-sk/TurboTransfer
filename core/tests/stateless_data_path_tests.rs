use std::io::Write;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};
use tempfile::tempdir;
use tokio::io::{duplex, AsyncRead, AsyncWrite, ReadBuf};
use turbotransfer_core::protocol::*;
use turbotransfer_core::transfer::*;
use uuid::Uuid;

/// Helper stream wrapper that corrupts the payload of a specific chunk ID once on the send path.
struct CorruptingStream<S> {
    inner: S,
    target_chunk_id: u32,
    corrupted_once: Arc<AtomicBool>,
    corrupt_next_payload: Arc<AtomicBool>,
}

impl<S: AsyncWrite + Unpin> AsyncWrite for CorruptingStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, std::io::Error>> {
        let modified_buf;
        let buf_to_write = if self.corrupt_next_payload.load(Ordering::SeqCst) && !self.corrupted_once.load(Ordering::SeqCst) {
            self.corrupted_once.store(true, Ordering::SeqCst);
            self.corrupt_next_payload.store(false, Ordering::SeqCst);
            let mut b_vec = buf.to_vec();
            if let Some(b) = b_vec.first_mut() {
                *b = b.wrapping_add(1);
            }
            modified_buf = b_vec;
            &modified_buf[..]
        } else if buf.len() > 13 && buf[4] == MSG_TYPE_CHUNK_DATA {
            let mut header_buf = buf[5..].to_vec();
            let len_offset = header_buf.len() - 8;
            header_buf[len_offset..].copy_from_slice(&0u64.to_le_bytes());
            if let Ok(Message::ChunkData(chunk)) = Message::decode_payload(MSG_TYPE_CHUNK_DATA, &header_buf) {
                if chunk.chunk_id == self.target_chunk_id && !self.corrupted_once.load(Ordering::SeqCst) {
                    self.corrupt_next_payload.store(true, Ordering::SeqCst);
                }
            }
            buf
        } else {
            buf
        };

        Pin::new(&mut self.inner).poll_write(cx, buf_to_write)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for CorruptingStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

#[tokio::test]
async fn test_end_to_end_file_transfer() {
    let temp_dir = tempdir().unwrap();
    let src_path = temp_dir.path().join("source_2mb.bin");
    let dest_dir = temp_dir.path().join("dest");

    // Generate 2 MiB of test data
    let test_data: Vec<u8> = (0..(2 * 1024 * 1024)).map(|i| (i % 251) as u8).collect();
    {
        let mut file = std::fs::File::create(&src_path).unwrap();
        file.write_all(&test_data).unwrap();
        file.flush().unwrap();
    }

    let (client_stream, server_stream) = duplex(64 * 1024);

    let sender_id = Uuid::new_v4();
    let receiver_id = Uuid::new_v4();
    let transfer_id = Uuid::new_v4();
    let chunk_size = 256 * 1024; // 256 KiB chunks (8 chunks total)

    let src_path_clone = src_path.clone();
    let sender_task = tokio::spawn(async move {
        send_file_session_stream(
            sender_id,
            "SenderWin",
            &src_path_clone,
            chunk_size,
            transfer_id,
            client_stream,
        )
        .await
    });

    let receiver_task = tokio::spawn(async move {
        let mut tracker = InMemoryChunkTracker::new();
        receive_file_session_stream(
            receiver_id,
            "ReceiverAndroid",
            &dest_dir,
            &mut tracker,
            server_stream,
        )
        .await
    });

    let (sender_res, receiver_res) = tokio::join!(sender_task, receiver_task);
    sender_res.unwrap().expect("Sender session should succeed");
    let final_dest_path = receiver_res.unwrap().expect("Receiver session should succeed");

    // Assert reassembled file bytes are identical to source
    let reassembled = std::fs::read(&final_dest_path).unwrap();
    assert_eq!(reassembled.len(), test_data.len());
    assert_eq!(reassembled, test_data);
}

#[tokio::test]
async fn test_chunk_corruption_nack_and_retry() {
    let temp_dir = tempdir().unwrap();
    let src_path = temp_dir.path().join("source_512k.bin");
    let dest_dir = temp_dir.path().join("dest_corrupt");

    let test_data: Vec<u8> = (0..(512 * 1024)).map(|i| (i % 233) as u8).collect();
    {
        let mut file = std::fs::File::create(&src_path).unwrap();
        file.write_all(&test_data).unwrap();
        file.flush().unwrap();
    }

    let (client_stream, server_stream) = duplex(64 * 1024);
    let corrupted_once = Arc::new(AtomicBool::new(false));
    let corrupt_next_payload = Arc::new(AtomicBool::new(false));

    let sender_stream = CorruptingStream {
        inner: client_stream,
        target_chunk_id: 1, // Corrupt chunk 1 first time sent
        corrupted_once: corrupted_once.clone(),
        corrupt_next_payload,
    };

    let sender_id = Uuid::new_v4();
    let receiver_id = Uuid::new_v4();
    let transfer_id = Uuid::new_v4();
    let chunk_size = 128 * 1024; // 4 chunks total

    let src_path_clone = src_path.clone();
    let sender_task = tokio::spawn(async move {
        send_file_session_stream(
            sender_id,
            "SenderWin",
            &src_path_clone,
            chunk_size,
            transfer_id,
            sender_stream,
        )
        .await
    });

    let receiver_task = tokio::spawn(async move {
        let mut tracker = InMemoryChunkTracker::new();
        receive_file_session_stream(
            receiver_id,
            "ReceiverAndroid",
            &dest_dir,
            &mut tracker,
            server_stream,
        )
        .await
    });

    let (sender_res, receiver_res) = tokio::join!(sender_task, receiver_task);
    sender_res.unwrap().expect("Sender should recover from NACK and succeed");
    let final_dest_path = receiver_res.unwrap().expect("Receiver should succeed after retry");

    // Assert corruption actually triggered once
    assert!(corrupted_once.load(Ordering::SeqCst), "Chunk corruption interceptor should have triggered");

    // Assert final file is identical to original source
    let reassembled = std::fs::read(&final_dest_path).unwrap();
    assert_eq!(reassembled, test_data);
}

#[tokio::test]
async fn test_idempotent_duplicate_chunk() {
    let temp_dir = tempdir().unwrap();
    let dest_dir = temp_dir.path().join("dest_idempotent");

    let (client, server) = duplex(64 * 1024);
    let receiver_id = Uuid::new_v4();
    let transfer_id = Uuid::new_v4();
    let file_id = Uuid::new_v4();
    let expected_payload = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let chunk_payload = expected_payload.clone();
    let checksum = turbotransfer_core::checksum::compute_xxhash64(&chunk_payload);

    let receiver_task = tokio::spawn(async move {
        let mut tracker = InMemoryChunkTracker::new();
        receive_file_session_stream(
            receiver_id,
            "ReceiverAndroid",
            &dest_dir,
            &mut tracker,
            server,
        )
        .await
    });

    let client_task = tokio::spawn(async move {
        let (client_read, mut client_write) = tokio::io::split(client);
        let mut reader = FrameReader::new(client_read);

        // 1. Hello exchange
        let hello = Message::Hello(HelloData {
            device_id: Uuid::new_v4(),
            device_name: "Sender".into(),
            protocol_version: 1,
        });
        send_msg(&mut client_write, &hello).await.unwrap();
        let _peer_hello = reader.read_frame().await.unwrap().unwrap();

        // 2. TransferOffer
        let offer = Message::TransferOffer(TransferOfferData {
            transfer_id,
            file_id,
            file_name: "idempotent.bin".into(),
            file_size: chunk_payload.len() as u64,
            chunk_size: 64,
            total_chunks: 1,
            checksum_algo: "xxhash64".into(),
        });
        send_msg(&mut client_write, &offer).await.unwrap();
        let _accept = reader.read_frame().await.unwrap().unwrap();

        // 3. Send ChunkData #0 (First delivery)
        let chunk_msg = Message::ChunkData(ChunkDataPayload {
            transfer_id,
            file_id,
            chunk_id: 0,
            file_offset: 0,
            payload_length: chunk_payload.len() as u32,
            checksum,
            payload: chunk_payload.clone(),
        });
        send_msg(&mut client_write, &chunk_msg).await.unwrap();
        let ack1 = reader.read_frame().await.unwrap().unwrap();
        assert!(matches!(ack1, Message::ChunkAck(ref a) if a.chunk_id == 0) || matches!(ack1, Message::BatchChunkAck(ref b) if b.chunk_ids.contains(&0)));

        // 4. Send ChunkData #0 AGAIN (Duplicate delivery)
        send_msg(&mut client_write, &chunk_msg).await.unwrap();
        let ack2 = reader.read_frame().await.unwrap().unwrap();
        assert!(
            matches!(ack2, Message::ChunkAck(ref a) if a.chunk_id == 0) || matches!(ack2, Message::BatchChunkAck(ref b) if b.chunk_ids.contains(&0)),
            "Duplicate chunk must return ChunkAck or BatchChunkAck"
        );

        // 5. Complete
        let complete_msg = Message::Complete(CompleteData {
            transfer_id,
            file_checksum: turbotransfer_core::checksum::compute_crc32c(&chunk_payload),
        });
        send_msg(&mut client_write, &complete_msg).await.unwrap();
        let _final_ack = reader.read_frame().await.unwrap().unwrap();
    });

    let (client_res, receiver_res) = tokio::join!(client_task, receiver_task);
    client_res.unwrap();
    let final_file = receiver_res.unwrap().expect("Receiver session should succeed");

    // Assert final file content
    let content = std::fs::read(&final_file).unwrap();
    assert_eq!(content, expected_payload);
}

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let port = 9876;
    let size_bytes: u64 = 50 * 1024 * 1024; // 50 MB
    let chunk_size = 256 * 1024; // 256 KB

    println!("============================================================");
    println!(" Testing ADB Tunnel with 256KB Chunks & Large OS Buffers (50 MB)");
    println!("============================================================");

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("Failed to connect");
    stream.set_nodelay(true).unwrap();

    let cmd = format!("STREAM_TEST:{}\n", size_bytes);
    stream.write_all(cmd.as_bytes()).unwrap();
    stream.flush().unwrap();

    let mut ready_buf = [0u8; 6];
    stream.read_exact(&mut ready_buf).unwrap();

    let payload = vec![0xBBu8; chunk_size];
    let mut sent = 0u64;
    let start_upload = Instant::now();

    while sent < size_bytes {
        let to_send = std::cmp::min(chunk_size as u64, size_bytes - sent) as usize;
        stream.write_all(&payload[..to_send]).unwrap();
        sent += to_send as u64;
    }
    stream.flush().unwrap();
    let upload_duration = start_upload.elapsed();

    let mut result_buf = [0u8; 128];
    let n = stream.read(&mut result_buf).unwrap_or(0);
    let res_str = String::from_utf8_lossy(&result_buf[..n]);

    let upload_mbps = (size_bytes as f64 / 1_048_576.0) / upload_duration.as_secs_f64();
    println!("  -> Upload (PC -> Android, 256KB buffer): {:.2} MB/s in {:.2}s (Server: {})", upload_mbps, upload_duration.as_secs_f64(), res_str.trim());
}

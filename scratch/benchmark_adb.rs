use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Instant;

fn main() {
    let port = 9876;
    let size_bytes: u64 = 100 * 1024 * 1024; // 100 MB
    let chunk_size = 64 * 1024; // 64 KB

    println!("============================================================");
    println!(" TurboTransfer - Native Rust ADB Tunnel POC Benchmark (100 MB)");
    println!(" Target: 127.0.0.1:{}", port);
    println!("============================================================");

    // 1. Upload Test (PC -> Android)
    println!("\n[1/2] Running Native UPLOAD Benchmark (PC -> Android)...");
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("Failed to connect");
    stream.set_nodelay(true).unwrap();

    let cmd = format!("STREAM_TEST:{}\n", size_bytes);
    stream.write_all(cmd.as_bytes()).unwrap();
    stream.flush().unwrap();

    let mut ready_buf = [0u8; 6];
    stream.read_exact(&mut ready_buf).unwrap(); // "READY\n"

    let payload = vec![0xAAu8; chunk_size];
    let mut sent = 0u64;
    let start_upload = Instant::now();

    while sent < size_bytes {
        let to_send = std::cmp::min(chunk_size as u64, size_bytes - sent) as usize;
        stream.write_all(&payload[..to_send]).unwrap();
        sent += to_send as u64;
    }
    stream.flush().unwrap();
    let upload_duration = start_upload.elapsed();

    // Read result
    let mut result_buf = [0u8; 128];
    let n = stream.read(&mut result_buf).unwrap_or(0);
    let res_str = String::from_utf8_lossy(&result_buf[..n]);

    let upload_mbps = (size_bytes as f64 / 1_048_576.0) / upload_duration.as_secs_f64();
    println!("  -> Upload (PC -> Android): {:.2} MB/s in {:.2}s (Server reported: {})", upload_mbps, upload_duration.as_secs_f64(), res_str.trim());
    drop(stream);

    std::thread::sleep(std::time::Duration::from_millis(500));

    // 2. Download Test (Android -> PC)
    println!("\n[2/2] Running Native DOWNLOAD Benchmark (Android -> PC)...");
    let mut stream2 = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("Failed to connect");
    stream2.set_nodelay(true).unwrap();

    let cmd2 = format!("STREAM_DOWNLOAD:{}\n", size_bytes);
    stream2.write_all(cmd2.as_bytes()).unwrap();
    stream2.flush().unwrap();

    let mut ready_buf2 = [0u8; 6];
    stream2.read_exact(&mut ready_buf2).unwrap(); // "READY\n"

    let mut recv_buf = vec![0u8; chunk_size];
    let mut received = 0u64;
    let start_download = Instant::now();

    while received < size_bytes {
        let to_read = std::cmp::min(chunk_size as u64, size_bytes - received) as usize;
        let n = stream2.read(&mut recv_buf[..to_read]).unwrap();
        if n == 0 { break; }
        received += n as u64;
    }
    let download_duration = start_download.elapsed();
    let download_mbps = (received as f64 / 1_048_576.0) / download_duration.as_secs_f64();

    println!("  -> Download (Android -> PC): {:.2} MB/s in {:.2}s", download_mbps, download_duration.as_secs_f64());
    drop(stream2);

    println!("\n============================================================");
    println!(" NATIVE RUST ADB POC BENCHMARK SUMMARY (100 MB):");
    println!("  * PC -> Android (Upload)  : {:.2} MB/s", upload_mbps);
    println!("  * Android -> PC (Download): {:.2} MB/s", download_mbps);
    println!("============================================================");
}

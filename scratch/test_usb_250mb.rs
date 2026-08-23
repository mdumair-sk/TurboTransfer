use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

fn run_8_stream_upload(total_bytes: u64) {
    let port = 9876;
    let num_streams = 8;
    let bytes_per_stream = total_bytes / num_streams as u64;
    let chunk_size = 128 * 1024; // 128 KB buffer

    println!("============================================================");
    println!(" [1/2] 250 MB UPLOAD (PC -> Android) over 8 Parallel USB Streams");
    println!(" Total: {:.1} MB ({:.2} MB per stream)", total_bytes as f64 / 1_048_576.0, bytes_per_stream as f64 / 1_048_576.0);
    println!("============================================================");

    let total_sent = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();
    let start_time = Instant::now();

    for stream_idx in 0..num_streams {
        let sent_counter = Arc::clone(&total_sent);
        let handle = std::thread::spawn(move || {
            let mut stream = match TcpStream::connect(format!("127.0.0.1:{}", port)) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Stream #{} failed to connect: {}", stream_idx, e);
                    return;
                }
            };
            stream.set_nodelay(true).unwrap();

            let cmd = format!("STREAM_TEST:{}\n", bytes_per_stream);
            stream.write_all(cmd.as_bytes()).unwrap();
            stream.flush().unwrap();

            let mut ready_buf = [0u8; 6];
            stream.read_exact(&mut ready_buf).unwrap();

            let payload = vec![0xAAu8; chunk_size];
            let mut sent = 0u64;

            while sent < bytes_per_stream {
                let to_send = std::cmp::min(chunk_size as u64, bytes_per_stream - sent) as usize;
                stream.write_all(&payload[..to_send]).unwrap();
                sent += to_send as u64;
                sent_counter.fetch_add(to_send as u64, Ordering::Relaxed);
            }
            stream.flush().unwrap();

            let mut result_buf = [0u8; 128];
            let _ = stream.read(&mut result_buf);
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    let elapsed = start_time.elapsed().as_secs_f64();
    let total_mb = total_bytes as f64 / 1_048_576.0;
    let speed_mbps = total_mb / elapsed;
    let speed_mbit = (total_bytes * 8) as f64 / (1_000_000.0 * elapsed);

    println!("  -> UPLOAD RESULTS:");
    println!("     Data Transferred : {:.1} MB", total_mb);
    println!("     Elapsed Time     : {:.2} seconds", elapsed);
    println!("     Aggregate Speed  : {:.2} MB/s ({:.1} Mbps)", speed_mbps, speed_mbit);
}

fn run_8_stream_download(total_bytes: u64) {
    let port = 9876;
    let num_streams = 8;
    let bytes_per_stream = total_bytes / num_streams as u64;
    let chunk_size = 128 * 1024; // 128 KB buffer

    println!("\n============================================================");
    println!(" [2/2] 250 MB DOWNLOAD (Android -> PC) over 8 Parallel USB Streams");
    println!(" Total: {:.1} MB ({:.2} MB per stream)", total_bytes as f64 / 1_048_576.0, bytes_per_stream as f64 / 1_048_576.0);
    println!("============================================================");

    let total_received = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::new();
    let start_time = Instant::now();

    for stream_idx in 0..num_streams {
        let recv_counter = Arc::clone(&total_received);
        let handle = std::thread::spawn(move || {
            let mut stream = match TcpStream::connect(format!("127.0.0.1:{}", port)) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Stream #{} failed to connect: {}", stream_idx, e);
                    return;
                }
            };
            stream.set_nodelay(true).unwrap();

            let cmd = format!("STREAM_DOWNLOAD:{}\n", bytes_per_stream);
            stream.write_all(cmd.as_bytes()).unwrap();
            stream.flush().unwrap();

            let mut ready_buf = [0u8; 6];
            stream.read_exact(&mut ready_buf).unwrap();

            let mut recv_buf = vec![0u8; chunk_size];
            let mut received = 0u64;

            while received < bytes_per_stream {
                let to_read = std::cmp::min(chunk_size as u64, bytes_per_stream - received) as usize;
                let n = stream.read(&mut recv_buf[..to_read]).unwrap_or(0);
                if n == 0 { break; }
                received += n as u64;
                recv_counter.fetch_add(n as u64, Ordering::Relaxed);
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    let elapsed = start_time.elapsed().as_secs_f64();
    let total_mb = total_bytes as f64 / 1_048_576.0;
    let speed_mbps = total_mb / elapsed;
    let speed_mbit = (total_bytes * 8) as f64 / (1_000_000.0 * elapsed);

    println!("  -> DOWNLOAD RESULTS:");
    println!("     Data Transferred : {:.1} MB", total_mb);
    println!("     Elapsed Time     : {:.2} seconds", elapsed);
    println!("     Aggregate Speed  : {:.2} MB/s ({:.1} Mbps)", speed_mbps, speed_mbit);
}

fn main() {
    let test_bytes: u64 = 250 * 1024 * 1024; // 250 MB

    println!("============================================================");
    println!(" TurboTransfer - 250 MB USB 8-STREAM BENCHMARK (NO WI-FI)");
    println!("============================================================");

    run_8_stream_upload(test_bytes);
    std::thread::sleep(std::time::Duration::from_millis(500));
    run_8_stream_download(test_bytes);
}

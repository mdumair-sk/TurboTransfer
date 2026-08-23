use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

fn test_parallel_streams(num_streams: usize, total_bytes: u64) {
    let port = 9876;
    let bytes_per_stream = total_bytes / num_streams as u64;
    let chunk_size = 64 * 1024;

    println!("\n>>> Testing with {} Parallel TCP Stream(s) over ADB Tunnel (Total: {} MB) <<<", num_streams, total_bytes / (1024 * 1024));

    let mut handles = Vec::new();
    let total_sent = Arc::new(AtomicU64::new(0));
    let start_time = Instant::now();

    for id in 0..num_streams {
        let sent_counter = Arc::clone(&total_sent);
        let handle = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
                .expect("Failed to connect parallel stream");
            stream.set_nodelay(true).unwrap();

            let cmd = format!("STREAM_TEST:{}\n", bytes_per_stream);
            stream.write_all(cmd.as_bytes()).unwrap();
            stream.flush().unwrap();

            let mut ready_buf = [0u8; 6];
            stream.read_exact(&mut ready_buf).unwrap();

            let payload = vec![0xCCu8; chunk_size];
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

    println!("  -> Aggregate Speed: {:.2} MB/s ({:.1} Mbps) in {:.2}s", speed_mbps, speed_mbit, elapsed);
}

fn main() {
    let test_bytes = 40 * 1024 * 1024; // 40 MB total per test

    println!("============================================================");
    println!(" ADB Tunnel Parallel Sockets Benchmark");
    println!(" Measuring throughput scaling across concurrent streams");
    println!("============================================================");

    test_parallel_streams(1, test_bytes);
    test_parallel_streams(2, test_bytes);
    test_parallel_streams(4, test_bytes);
    test_parallel_streams(8, test_bytes);
}

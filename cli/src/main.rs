use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use turbotransfer_core::transfer::{
    cancel_transfer, enter_receive_mode, get_devices, get_progress, get_transfers, start_transfer,
    TransportPreference, DEFAULT_LISTEN_ADDR,
};
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "turbo", author, version, about = "TurboTransfer CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(ValueEnum, Clone, Copy, Debug)]
enum CliTransportPreference {
    Auto,
    Combined,
    Usb,
    WifiDirect,
}

impl From<CliTransportPreference> for TransportPreference {
    fn from(pref: CliTransportPreference) -> Self {
        match pref {
            CliTransportPreference::Auto => TransportPreference::Automatic,
            CliTransportPreference::Combined => TransportPreference::Combined,
            CliTransportPreference::Usb => TransportPreference::UsbOnly,
            CliTransportPreference::WifiDirect => TransportPreference::WifiDirectOnly,
        }
    }
}

#[derive(Subcommand)]
enum Commands {
    /// Send a file to a peer
    Send {
        /// Path to the file to send
        path: PathBuf,

        /// Target device UUID
        #[arg(short, long)]
        device: Option<Uuid>,

        /// Preferred transport layer
        #[arg(short, long, value_enum, default_value_t = CliTransportPreference::Auto)]
        transport: CliTransportPreference,

        /// Target peer network address (default 127.0.0.1:9876)
        #[arg(short, long)]
        address: Option<String>,
    },

    /// Receive incoming file transfers
    Receive {
        /// Destination directory for saved files (default current dir)
        #[arg(short, long, default_value = ".")]
        dest: PathBuf,

        /// Listening network address (default 127.0.0.1:9876)
        #[arg(short, long)]
        address: Option<String>,
    },

    /// Discover available transfer devices
    Discover,

    /// List active, resumable, and completed transfers
    Transfers,

    /// Show detailed logs and bottleneck diagnostics for a transfer
    Log {
        /// Transfer UUID to inspect
        transfer_id: Uuid,

        /// Output raw JSON telemetry report
        #[arg(long)]
        json: bool,

        /// Print individual event timeline
        #[arg(long)]
        events: bool,
    },

    /// List all archived transfer diagnostic log files
    Logs,

    /// Cancel an active transfer
    Cancel {
        /// Transfer UUID to cancel
        transfer_id: Uuid,
    },

    /// Resume a paused/interrupted transfer (or the most recent if omitted)
    Resume {
        /// Optional transfer UUID to resume
        transfer_id: Option<Uuid>,

        /// Preferred transport layer
        #[arg(short, long, value_enum, default_value_t = CliTransportPreference::Auto)]
        transport: CliTransportPreference,

        /// Target peer network address
        #[arg(short, long)]
        address: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Send {
            path,
            device,
            transport,
            address,
        } => {
            println!("Starting transfer for: {}", path.display());
            let handle = start_transfer(path, None, device, transport.into(), address).await?;
            println!("Transfer initiated with ID: {}", handle.transfer_id);
            println!("Streaming chunks to device...");

            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
                if let Some(p) = get_progress(handle.transfer_id) {
                    let mb_s = (p.aggregate_throughput_bps as f64) / (1024.0 * 1024.0);
                    print!(
                        "\rProgress: {:.1}% ({}/{} bytes) | {:.2} MB/s | ETA: {}s | Status: {:?}",
                        p.percent, p.bytes_transferred, p.file_size, mb_s, p.eta_seconds.unwrap_or(0), p.status
                    );
                    let _ = std::io::Write::flush(&mut std::io::stdout());

                    match p.status {
                        turbotransfer_core::manifest::TransferStatus::Completed => {
                            println!("\nTransfer completed successfully!");
                            break;
                        }
                        turbotransfer_core::manifest::TransferStatus::Failed => {
                            let err = turbotransfer_core::transfer::get_transfer_error(handle.transfer_id)
                                .unwrap_or_default();
                            println!("\nTransfer failed: {}", err);
                            break;
                        }
                        turbotransfer_core::manifest::TransferStatus::Cancelled => {
                            println!("\nTransfer cancelled.");
                            break;
                        }
                        _ => {}
                    }
                } else {
                    break;
                }
            }
        }
        Commands::Receive { dest, address } => {
            let addr = address.unwrap_or_else(|| DEFAULT_LISTEN_ADDR.to_string());
            println!(
                "Entering continuous receive mode on {}... (destination: {})",
                addr,
                dest.display()
            );
            let mut receive_task = enter_receive_mode(Some(addr.clone()), dest.clone()).await?;
            let mut seen_completed = std::collections::HashSet::new();
            let mut seen_started = std::collections::HashSet::new();
            let mut monitor_interval = tokio::time::interval(tokio::time::Duration::from_millis(200));

            loop {
                tokio::select! {
                    _ = monitor_interval.tick() => {
                        let active_transfers = get_transfers();
                        for t in active_transfers {
                            if t.role == turbotransfer_core::manifest::TransferRole::Receiver {
                                if seen_started.insert(t.transfer_id) {
                                    println!("\n[Incoming Transfer] Receiving '{}' ({} bytes, ID: {})", t.file_name, t.file_size, t.transfer_id);
                                }
                                if let Some(p) = get_progress(t.transfer_id) {
                                    let mb_s = (p.aggregate_throughput_bps as f64) / (1024.0 * 1024.0);
                                    print!(
                                        "\r[{}] {:.1}% ({}/{} bytes) | {:.2} MB/s | Status: {:?}",
                                        t.file_name, p.percent, p.bytes_transferred, p.file_size, mb_s, p.status
                                    );
                                    let _ = std::io::Write::flush(&mut std::io::stdout());

                                    if p.status == turbotransfer_core::manifest::TransferStatus::Completed && seen_completed.insert(t.transfer_id) {
                                        println!("\n[Incoming Transfer] Finished '{}' -> saved to destination directory!", t.file_name);
                                    } else if p.status == turbotransfer_core::manifest::TransferStatus::Failed && seen_completed.insert(t.transfer_id) {
                                        println!("\n[Incoming Transfer] Failed '{}'!", t.file_name);
                                    }
                                }
                            }
                        }
                    }
                    res = &mut receive_task => {
                        match res {
                            Ok(Ok(output_path)) => {
                                println!("\nSuccessfully received file: {}", output_path.display());
                            }
                            Ok(Err(e)) => {
                                eprintln!("\nReceive session error: {}", e);
                            }
                            Err(e) => {
                                eprintln!("\nReceive task error: {}", e);
                                break;
                            }
                        }
                    }
                }
            }
        }
        Commands::Discover => {
            println!("Discovered Devices:");
            for dev in get_devices() {
                println!(
                    " - {} [{}] ({}) connected={}",
                    dev.device_name, dev.device_id, dev.transport, dev.is_connected
                );
            }
        }
        Commands::Transfers => {
            println!("Transfer List:");
            for t in get_transfers() {
                println!(
                    " - [{:?}] {} ({} bytes) - status: {:?}",
                    t.role, t.file_name, t.file_size, t.status
                );
            }
        }
        Commands::Resume {
            transfer_id,
            transport,
            address,
        } => {
            println!("Resuming transfer (ID: {:?})...", transfer_id);
            let handle = turbotransfer_core::transfer::resume_transfer(
                transfer_id,
                transport.into(),
                address,
            )
            .await?;
            println!("Resumed transfer ID: {}", handle.transfer_id);

            loop {
                tokio::time::sleep(tokio::time::Duration::from_millis(250)).await;
                if let Some(p) = get_progress(handle.transfer_id) {
                    let mb_s = (p.aggregate_throughput_bps as f64) / (1024.0 * 1024.0);
                    print!(
                        "\rProgress: {:.1}% ({}/{} bytes) | {:.2} MB/s | ETA: {}s | Status: {:?}",
                        p.percent, p.bytes_transferred, p.file_size, mb_s, p.eta_seconds.unwrap_or(0), p.status
                    );
                    let _ = std::io::Write::flush(&mut std::io::stdout());

                    match p.status {
                        turbotransfer_core::manifest::TransferStatus::Completed => {
                            println!("\nResumed transfer completed successfully!");
                            break;
                        }
                        turbotransfer_core::manifest::TransferStatus::Failed => {
                            println!("\nResumed transfer failed!");
                            break;
                        }
                        turbotransfer_core::manifest::TransferStatus::Cancelled => {
                            println!("\nResumed transfer cancelled.");
                            break;
                        }
                        _ => {}
                    }
                } else {
                    break;
                }
            }
        }
        Commands::Log {
            transfer_id,
            json,
            events,
        } => {
            let data_dir = turbotransfer_core::transfer::default_data_dir();
            let json_path = data_dir.join("logs").join(format!("{}.json", transfer_id));
            let log_path = data_dir.join("logs").join(format!("{}.log", transfer_id));

            if !json_path.exists() && !log_path.exists() {
                if let Some(telemetry) = turbotransfer_core::util::telemetry::get_telemetry(transfer_id) {
                    let report = telemetry.generate_report();
                    if json {
                        println!("{}", serde_json::to_string_pretty(&report)?);
                    } else {
                        println!("============================================================");
                        println!("⚡ TURBOTRANSFER LIVE TELEMETRY & BOTTLENECK REPORT");
                        println!("============================================================");
                        println!("Transfer ID:      {}", report.transfer_id);
                        println!("File:             {} ({} bytes)", report.file_name, report.file_size);
                        println!("Role:             {:?}", report.role);
                        println!("Duration:         {} ms", report.total_duration_ms);
                        println!("Avg Throughput:   {:.2} MB/s ({:.2} Mbps)", report.avg_throughput_mbps, report.avg_throughput_mbps * 8.0);
                        println!("Peak Throughput:  {:.2} MB/s", report.peak_throughput_mbps);
                        println!("Primary Verdict:  🎯 {}", report.primary_bottleneck);
                        println!("------------------------------------------------------------");
                        println!("📊 Subsystem Latency & Speeds:");
                        println!("  Sender Disk Read:     {:.2} MB/s (avg: {:.0} µs, p95: {:.0} µs)", report.sender_disk_read_mbps, report.sender_disk_read_avg_us, report.sender_disk_read_p95_us);
                        println!("  Sender Checksum:      {:.2} MB/s (avg: {:.0} µs)", report.sender_checksum_mbps, report.sender_checksum_avg_us);
                        println!("  Receiver Disk Write:  {:.2} MB/s (avg: {:.0} µs, p95: {:.0} µs, max queue: {})", report.receiver_disk_write_mbps, report.receiver_disk_write_avg_us, report.receiver_disk_write_p95_us, report.receiver_max_queue_depth);
                        println!("  Receiver Finalize:    {} ms", report.receiver_finalize_ms);
                        println!("------------------------------------------------------------");
                        println!("🌐 Channel Metrics:");
                        for ch in &report.channels {
                            println!("  * [{}] {} bytes, {} chunks | write: {:.0} µs | RTT avg: {:.2} ms (p95: {:.2} ms) | NACKs: {}, drops: {}",
                                ch.channel_name, ch.bytes_transferred, ch.chunks_transferred, ch.avg_socket_write_us, ch.avg_rtt_ms, ch.p95_rtt_ms, ch.nack_count, ch.disconnect_count
                            );
                        }
                        if !report.recommendations.is_empty() {
                            println!("------------------------------------------------------------");
                            println!("💡 Diagnostic Recommendations:");
                            for rec in &report.recommendations {
                                println!("  - {}", rec);
                            }
                        }
                        println!("============================================================");
                    }
                    if events {
                        println!("\n📜 Event Timeline:");
                        let evs = telemetry.events.lock();
                        for ev in evs.iter() {
                            println!("  +{:>6}ms [{:?}] [{}] {}", ev.relative_ms, ev.stage, ev.channel, ev.message);
                        }
                    }
                    return Ok(());
                }

                eprintln!("No log files found for transfer {} in {}", transfer_id, data_dir.display());
                return Ok(());
            }

            if json && json_path.exists() {
                let content = std::fs::read_to_string(&json_path)?;
                println!("{}", content);
            } else if events && log_path.exists() {
                let content = std::fs::read_to_string(&log_path)?;
                println!("{}", content);
            } else if json_path.exists() {
                let content = std::fs::read_to_string(&json_path)?;
                if let Ok(report) = serde_json::from_str::<turbotransfer_core::util::telemetry::BottleneckReport>(&content) {
                    println!("============================================================");
                    println!("⚡ TURBOTRANSFER HISTORICAL BOTTLENECK REPORT");
                    println!("============================================================");
                    println!("Transfer ID:      {}", report.transfer_id);
                    println!("File:             {} ({} bytes)", report.file_name, report.file_size);
                    println!("Role:             {:?}", report.role);
                    println!("Duration:         {} ms", report.total_duration_ms);
                    println!("Avg Throughput:   {:.2} MB/s ({:.2} Mbps)", report.avg_throughput_mbps, report.avg_throughput_mbps * 8.0);
                    println!("Peak Throughput:  {:.2} MB/s", report.peak_throughput_mbps);
                    println!("Primary Verdict:  🎯 {}", report.primary_bottleneck);
                    println!("------------------------------------------------------------");
                    println!("📊 Subsystem Latency & Speeds:");
                    println!("  Sender Disk Read:     {:.2} MB/s (avg: {:.0} µs, p95: {:.0} µs)", report.sender_disk_read_mbps, report.sender_disk_read_avg_us, report.sender_disk_read_p95_us);
                    println!("  Sender Checksum:      {:.2} MB/s (avg: {:.0} µs)", report.sender_checksum_mbps, report.sender_checksum_avg_us);
                    println!("  Receiver Disk Write:  {:.2} MB/s (avg: {:.0} µs, p95: {:.0} µs, max queue: {})", report.receiver_disk_write_mbps, report.receiver_disk_write_avg_us, report.receiver_disk_write_p95_us, report.receiver_max_queue_depth);
                    println!("  Receiver Finalize:    {} ms", report.receiver_finalize_ms);
                    println!("------------------------------------------------------------");
                    println!("🌐 Channel Metrics:");
                    for ch in &report.channels {
                        println!("  * [{}] {} bytes, {} chunks | write: {:.0} µs | RTT avg: {:.2} ms (p95: {:.2} ms) | NACKs: {}, drops: {}",
                            ch.channel_name, ch.bytes_transferred, ch.chunks_transferred, ch.avg_socket_write_us, ch.avg_rtt_ms, ch.p95_rtt_ms, ch.nack_count, ch.disconnect_count
                        );
                    }
                    if !report.recommendations.is_empty() {
                        println!("------------------------------------------------------------");
                        println!("💡 Diagnostic Recommendations:");
                        for rec in &report.recommendations {
                            println!("  - {}", rec);
                        }
                    }
                    println!("============================================================");
                    println!("Log files: {} | {}", json_path.display(), log_path.display());
                } else if log_path.exists() {
                    let content = std::fs::read_to_string(&log_path)?;
                    println!("{}", content);
                }
            } else if log_path.exists() {
                let content = std::fs::read_to_string(&log_path)?;
                println!("{}", content);
            }
        }
        Commands::Logs => {
            let data_dir = turbotransfer_core::transfer::default_data_dir();
            let logs_dir = data_dir.join("logs");
            println!("Transfer Diagnostic Log Directory: {}", logs_dir.display());
            println!("------------------------------------------------------------");
            if let Ok(entries) = std::fs::read_dir(&logs_dir) {
                let mut found = 0;
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("json") {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if let Ok(report) = serde_json::from_str::<turbotransfer_core::util::telemetry::BottleneckReport>(&content) {
                                found += 1;
                                println!("{}. [{:?}] {} ({} bytes, ID: {})", found, report.role, report.file_name, report.file_size, report.transfer_id);
                                println!("   Throughput: {:.2} MB/s | Duration: {} ms | Verdict: {}", report.avg_throughput_mbps, report.total_duration_ms, report.primary_bottleneck);
                            }
                        }
                    }
                }
                if found == 0 {
                    println!("No transfer logs found yet. Logs are automatically written when transfers complete or fail.");
                }
            } else {
                println!("No logs directory found at {}.", logs_dir.display());
            }
        }
        Commands::Cancel { transfer_id } => {
            cancel_transfer(transfer_id);
            println!("Cancelled transfer: {}", transfer_id);
        }
    }

    Ok(())
}

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
            loop {
                let receive_task = enter_receive_mode(Some(addr.clone()), dest.clone()).await?;
                match receive_task.await {
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
        Commands::Cancel { transfer_id } => {
            cancel_transfer(transfer_id);
            println!("Cancelled transfer: {}", transfer_id);
        }
    }

    Ok(())
}

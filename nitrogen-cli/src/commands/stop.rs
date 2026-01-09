//! Stop command - stop a running nitrogen daemon

use anyhow::Result;
use clap::Args;
use nitrogen_core::{daemon_running, IpcClient};

/// Arguments for the stop command
#[derive(Args, Debug)]
pub struct StopArgs {
    /// Force stop without waiting for cleanup
    #[arg(short, long)]
    pub force: bool,
}

/// Stop the current capture session
pub async fn stop(args: StopArgs) -> Result<()> {
    println!("Nitrogen - Stop Capture\n");

    // Check if daemon is running
    if !daemon_running().await {
        println!("No nitrogen instance is currently running.");
        println!();
        println!("If nitrogen is running in another terminal, use Ctrl+C to stop it.");
        return Ok(());
    }

    // Connect to daemon
    let mut client = match IpcClient::connect().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to connect to daemon: {}", e);
            eprintln!();
            eprintln!("The socket file may be stale. Try running nitrogen again.");
            return Err(e.into());
        }
    };

    // Send stop command
    println!("Stopping nitrogen...");

    let result = if args.force {
        client.force_stop().await
    } else {
        client.stop().await
    };

    match result {
        Ok(()) => {
            println!("Stop signal sent. Nitrogen is shutting down.");
            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to stop daemon: {}", e);
            Err(e.into())
        }
    }
}

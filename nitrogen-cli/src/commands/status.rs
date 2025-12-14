//! Status command - show status of running capture

use anyhow::Result;

/// Show status of running capture
pub async fn status() -> Result<()> {
    println!("Nitrogen - Status\n");

    // In a full implementation, this would query a running nitrogen
    // daemon for current status.

    println!("Status information:");
    println!("  State: Not implemented yet");
    println!();
    println!("Note: Daemon mode is not yet implemented.");
    println!("When running 'nitrogen cast', the status is shown in the terminal.");

    // TODO: Implement proper IPC to query status from a running daemon
    // Information to show:
    // - Running state (idle, capturing, error)
    // - Current source (monitor/window)
    // - Resolution and framerate
    // - Encoder stats (fps, bitrate, latency)
    // - Virtual camera status

    Ok(())
}

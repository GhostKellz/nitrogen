//! Stop command - stop the current capture session

use anyhow::Result;

/// Stop the current capture session
pub async fn stop() -> Result<()> {
    println!("Nitrogen - Stop Capture\n");

    // In a full implementation, this would communicate with a running
    // nitrogen daemon or service to stop the capture.

    // For now, since we run in the foreground, Ctrl+C is the way to stop.

    println!("Note: If nitrogen is running in another terminal, use Ctrl+C to stop it.");
    println!();
    println!("For daemon mode (future feature), use: nitrogen stop");

    // TODO: Implement proper IPC to stop a running daemon
    // Options:
    // - Unix socket communication
    // - D-Bus interface
    // - PID file + signal

    Ok(())
}

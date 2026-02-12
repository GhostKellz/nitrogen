//! IPC (Inter-Process Communication) for daemon mode
//!
//! Provides Unix socket-based communication between the running nitrogen
//! daemon and CLI commands like `stop` and `status`.

mod client;
mod protocol;
mod server;

pub use client::IpcClient;
pub use protocol::{IpcMessage, IpcResponse, PipelineStatistics, PipelineStatus};
pub use server::IpcServer;

use std::path::PathBuf;

/// Get the IPC socket path
///
/// Uses XDG_RUNTIME_DIR if available, otherwise /tmp
pub fn socket_path() -> PathBuf {
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        PathBuf::from(runtime_dir).join("nitrogen.sock")
    } else {
        // Fallback to /tmp with user-specific name
        // SAFETY: libc::getuid() is a simple syscall that returns the real user ID.
        // It has no preconditions and cannot fail (always returns a valid uid_t).
        let uid = unsafe { libc::getuid() };
        PathBuf::from(format!("/tmp/nitrogen-{}.sock", uid))
    }
}

/// Check if the daemon is running by checking if the socket exists and is responsive
pub async fn daemon_running() -> bool {
    let path = socket_path();
    if !path.exists() {
        return false;
    }

    // Try to connect and ping
    match IpcClient::connect().await {
        Ok(mut client) => {
            matches!(client.ping().await, Ok(true))
        }
        Err(_) => false,
    }
}

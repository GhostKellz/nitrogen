//! IPC protocol definitions
//!
//! Defines the message types used for communication between the daemon and CLI.

use serde::{Deserialize, Serialize};

/// Messages that can be sent to the daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpcMessage {
    /// Check if daemon is alive
    Ping,
    /// Request current status
    Status,
    /// Request pipeline statistics
    Stats,
    /// Stop the daemon gracefully
    Stop,
    /// Force stop the daemon
    ForceStop,
}

/// Responses from the daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IpcResponse {
    /// Simple acknowledgment
    Ok,
    /// Pong response to ping
    Pong,
    /// Error response
    Error { message: String },
    /// Status response
    Status(PipelineStatus),
    /// Statistics response
    Stats(PipelineStatistics),
    /// Shutdown acknowledgment
    Stopping,
}

/// Current pipeline status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStatus {
    /// Is the pipeline running
    pub running: bool,
    /// Current state name
    pub state: String,
    /// Source being captured (if any)
    pub source: Option<String>,
    /// Output resolution
    pub resolution: Option<(u32, u32)>,
    /// Target FPS
    pub fps: Option<u32>,
    /// Virtual camera name
    pub camera_name: Option<String>,
    /// Process ID
    pub pid: u32,
    /// Uptime in seconds
    pub uptime_seconds: f64,
}

/// Pipeline statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineStatistics {
    /// Frames processed
    pub frames_processed: u64,
    /// Frames dropped (channel lag)
    pub frames_dropped: u64,
    /// Frames failed to write
    pub frames_failed: u64,
    /// Actual measured FPS
    pub actual_fps: f64,
    /// Target FPS
    pub target_fps: u32,
    /// Elapsed time in seconds
    pub elapsed_seconds: f64,
    /// Output resolution
    pub resolution: (u32, u32),
    /// Codec being used
    pub codec: String,
    /// Target bitrate in kbps
    pub bitrate: u32,
}

impl IpcMessage {
    /// Serialize message to JSON bytes with newline terminator
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = serde_json::to_vec(self).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }

    /// Deserialize message from JSON bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

impl IpcResponse {
    /// Serialize response to JSON bytes with newline terminator
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = serde_json::to_vec(self).unwrap_or_default();
        bytes.push(b'\n');
        bytes
    }

    /// Deserialize response from JSON bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }

    /// Create an error response
    pub fn error(message: impl Into<String>) -> Self {
        IpcResponse::Error {
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_serialization() {
        let msg = IpcMessage::Ping;
        let bytes = msg.to_bytes();
        let parsed = IpcMessage::from_bytes(&bytes[..bytes.len() - 1]).unwrap();
        assert!(matches!(parsed, IpcMessage::Ping));
    }

    #[test]
    fn test_response_serialization() {
        let resp = IpcResponse::Pong;
        let bytes = resp.to_bytes();
        let parsed = IpcResponse::from_bytes(&bytes[..bytes.len() - 1]).unwrap();
        assert!(matches!(parsed, IpcResponse::Pong));
    }
}

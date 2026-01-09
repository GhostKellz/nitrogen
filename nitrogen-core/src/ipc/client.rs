//! IPC client for CLI commands
//!
//! Connects to the running daemon to send commands and receive responses.

use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tracing::debug;

use super::protocol::{IpcMessage, IpcResponse, PipelineStatistics, PipelineStatus};
use super::socket_path;
use crate::error::{NitrogenError, Result};

/// Default connection timeout
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Default read/write timeout
const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// IPC client for communicating with the daemon
pub struct IpcClient {
    stream: UnixStream,
}

impl IpcClient {
    /// Connect to the daemon with default timeout
    pub async fn connect() -> Result<Self> {
        Self::connect_with_timeout(CONNECT_TIMEOUT).await
    }

    /// Connect to the daemon with custom timeout
    pub async fn connect_with_timeout(timeout: Duration) -> Result<Self> {
        let path = socket_path();

        if !path.exists() {
            return Err(NitrogenError::NoActiveSession);
        }

        let stream = tokio::time::timeout(timeout, UnixStream::connect(&path))
            .await
            .map_err(|_| NitrogenError::Config("Connection timed out".into()))?
            .map_err(|e| NitrogenError::Config(format!("Failed to connect to daemon: {}", e)))?;

        debug!("Connected to daemon at {:?}", path);

        Ok(Self { stream })
    }

    /// Send a message and receive a response with timeout
    async fn send(&mut self, msg: IpcMessage) -> Result<IpcResponse> {
        self.send_with_timeout(msg, IO_TIMEOUT).await
    }

    /// Send a message and receive a response with custom timeout
    async fn send_with_timeout(
        &mut self,
        msg: IpcMessage,
        timeout: Duration,
    ) -> Result<IpcResponse> {
        let (reader, mut writer) = self.stream.split();

        // Send message with timeout
        let msg_bytes = msg.to_bytes();
        tokio::time::timeout(timeout, writer.write_all(&msg_bytes))
            .await
            .map_err(|_| NitrogenError::Config("Write timed out".into()))?
            .map_err(|e| NitrogenError::Config(format!("Failed to send message: {}", e)))?;

        // Read response with timeout
        let mut reader = BufReader::new(reader);
        let mut line = String::new();
        tokio::time::timeout(timeout, reader.read_line(&mut line))
            .await
            .map_err(|_| NitrogenError::Config("Read timed out".into()))?
            .map_err(|e| NitrogenError::Config(format!("Failed to read response: {}", e)))?;

        IpcResponse::from_bytes(line.trim().as_bytes())
            .map_err(|e| NitrogenError::Config(format!("Invalid response: {}", e)))
    }

    /// Ping the daemon to check if it's alive
    pub async fn ping(&mut self) -> Result<bool> {
        match self.send(IpcMessage::Ping).await {
            Ok(IpcResponse::Pong) => Ok(true),
            Ok(_) => Ok(false),
            Err(_) => Ok(false),
        }
    }

    /// Get the current status
    pub async fn status(&mut self) -> Result<PipelineStatus> {
        match self.send(IpcMessage::Status).await? {
            IpcResponse::Status(status) => Ok(status),
            IpcResponse::Error { message } => Err(NitrogenError::Config(message)),
            _ => Err(NitrogenError::Config("Unexpected response".into())),
        }
    }

    /// Get pipeline statistics
    pub async fn stats(&mut self) -> Result<PipelineStatistics> {
        match self.send(IpcMessage::Stats).await? {
            IpcResponse::Stats(stats) => Ok(stats),
            IpcResponse::Error { message } => Err(NitrogenError::Config(message)),
            _ => Err(NitrogenError::Config("Unexpected response".into())),
        }
    }

    /// Request the daemon to stop
    pub async fn stop(&mut self) -> Result<()> {
        match self.send(IpcMessage::Stop).await? {
            IpcResponse::Stopping => Ok(()),
            IpcResponse::Error { message } => Err(NitrogenError::Config(message)),
            _ => Err(NitrogenError::Config("Unexpected response".into())),
        }
    }

    /// Request the daemon to force stop
    pub async fn force_stop(&mut self) -> Result<()> {
        match self.send(IpcMessage::ForceStop).await? {
            IpcResponse::Stopping => Ok(()),
            IpcResponse::Error { message } => Err(NitrogenError::Config(message)),
            _ => Err(NitrogenError::Config("Unexpected response".into())),
        }
    }
}

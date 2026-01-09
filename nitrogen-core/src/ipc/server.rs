//! IPC server for daemon mode
//!
//! Listens on a Unix socket and handles commands from CLI clients.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

use super::protocol::{IpcMessage, IpcResponse, PipelineStatistics, PipelineStatus};
use super::socket_path;
use crate::error::{NitrogenError, Result};
use crate::pipeline::Pipeline;

/// IPC server that handles client connections
pub struct IpcServer {
    /// Path to the Unix socket
    socket_path: PathBuf,
    /// Listener for incoming connections
    listener: Option<UnixListener>,
    /// Shared pipeline state
    pipeline: Arc<RwLock<Option<Pipeline>>>,
    /// Shutdown signal sender
    shutdown_tx: broadcast::Sender<()>,
    /// Start time for uptime calculation
    start_time: Instant,
}

impl IpcServer {
    /// Create a new IPC server
    pub fn new(pipeline: Arc<RwLock<Option<Pipeline>>>) -> Result<Self> {
        let path = socket_path();
        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Self {
            socket_path: path,
            listener: None,
            pipeline,
            shutdown_tx,
            start_time: Instant::now(),
        })
    }

    /// Start listening for connections
    pub async fn start(&mut self) -> Result<()> {
        // Remove existing socket if present
        if self.socket_path.exists() {
            std::fs::remove_file(&self.socket_path).map_err(|e| {
                NitrogenError::Config(format!("Failed to remove old socket: {}", e))
            })?;
        }

        // Create parent directory if needed
        if let Some(parent) = self.socket_path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    NitrogenError::Config(format!("Failed to create socket directory: {}", e))
                })?;
            }
        }

        // Bind to socket
        let listener = UnixListener::bind(&self.socket_path).map_err(|e| {
            NitrogenError::Config(format!(
                "Failed to bind socket at {:?}: {}",
                self.socket_path, e
            ))
        })?;

        // Set socket permissions to owner-only (0600)
        // This prevents other users from connecting to our daemon
        let permissions = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&self.socket_path, permissions).map_err(|e| {
            warn!("Failed to set socket permissions: {}", e);
            NitrogenError::Config(format!("Failed to set socket permissions: {}", e))
        })?;

        info!("IPC server listening on {:?}", self.socket_path);
        self.listener = Some(listener);

        Ok(())
    }

    /// Get a receiver for shutdown signals
    pub fn shutdown_receiver(&self) -> broadcast::Receiver<()> {
        self.shutdown_tx.subscribe()
    }

    /// Accept and handle one connection
    ///
    /// Returns true if the server should continue, false if it should shut down
    pub async fn accept_one(&self) -> Result<bool> {
        let listener = self
            .listener
            .as_ref()
            .ok_or_else(|| NitrogenError::Config("Server not started".into()))?;

        // Accept with timeout to allow checking for shutdown
        let accept_result =
            tokio::time::timeout(std::time::Duration::from_millis(100), listener.accept()).await;

        let (stream, _addr) = match accept_result {
            Ok(Ok((stream, addr))) => (stream, addr),
            Ok(Err(e)) => {
                error!("Failed to accept connection: {}", e);
                return Ok(true); // Continue running
            }
            Err(_) => {
                // Timeout, just continue
                return Ok(true);
            }
        };

        debug!("IPC client connected");

        // Handle the connection
        let should_continue = self.handle_connection(stream).await;

        Ok(should_continue)
    }

    /// Handle a client connection
    ///
    /// Returns true if server should continue, false if it should shut down
    async fn handle_connection(&self, stream: UnixStream) -> bool {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    // Connection closed
                    debug!("IPC client disconnected");
                    return true;
                }
                Ok(_) => {
                    // Parse and handle message
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    match IpcMessage::from_bytes(trimmed.as_bytes()) {
                        Ok(msg) => {
                            let (response, should_stop) = self.handle_message(msg).await;

                            // Send response
                            let response_bytes = response.to_bytes();
                            if let Err(e) = writer.write_all(&response_bytes).await {
                                error!("Failed to send IPC response: {}", e);
                                return true;
                            }

                            if should_stop {
                                // Signal shutdown
                                let _ = self.shutdown_tx.send(());
                                return false;
                            }
                        }
                        Err(e) => {
                            warn!("Invalid IPC message: {}", e);
                            let response = IpcResponse::error(format!("Invalid message: {}", e));
                            let _ = writer.write_all(&response.to_bytes()).await;
                        }
                    }
                }
                Err(e) => {
                    error!("Error reading from IPC client: {}", e);
                    return true;
                }
            }
        }
    }

    /// Handle an IPC message
    ///
    /// Returns (response, should_stop)
    async fn handle_message(&self, msg: IpcMessage) -> (IpcResponse, bool) {
        match msg {
            IpcMessage::Ping => (IpcResponse::Pong, false),
            IpcMessage::Status => {
                let status = self.get_status().await;
                (IpcResponse::Status(status), false)
            }
            IpcMessage::Stats => {
                let stats = self.get_stats().await;
                match stats {
                    Some(s) => (IpcResponse::Stats(s), false),
                    None => (IpcResponse::error("No active pipeline"), false),
                }
            }
            IpcMessage::Stop => {
                info!("Received stop command via IPC");
                (IpcResponse::Stopping, true)
            }
            IpcMessage::ForceStop => {
                info!("Received force stop command via IPC");
                (IpcResponse::Stopping, true)
            }
        }
    }

    /// Get current pipeline status
    async fn get_status(&self) -> PipelineStatus {
        let pipeline_guard = self.pipeline.read().await;
        let uptime = self.start_time.elapsed().as_secs_f64();

        match pipeline_guard.as_ref() {
            Some(pipeline) => {
                let stats = pipeline.stats();
                PipelineStatus {
                    running: pipeline.is_running(),
                    state: format!("{:?}", stats.state),
                    source: None, // Could add source info to stats
                    resolution: Some(stats.resolution),
                    fps: Some(stats.fps),
                    camera_name: None, // Could add to stats
                    pid: std::process::id(),
                    uptime_seconds: uptime,
                }
            }
            None => PipelineStatus {
                running: false,
                state: "Idle".to_string(),
                source: None,
                resolution: None,
                fps: None,
                camera_name: None,
                pid: std::process::id(),
                uptime_seconds: uptime,
            },
        }
    }

    /// Get pipeline statistics
    async fn get_stats(&self) -> Option<PipelineStatistics> {
        let pipeline_guard = self.pipeline.read().await;

        pipeline_guard.as_ref().map(|pipeline| {
            let stats = pipeline.stats();
            PipelineStatistics {
                frames_processed: stats.frames_processed,
                frames_dropped: stats.frames_dropped,
                frames_failed: stats.frames_failed,
                actual_fps: stats.actual_fps,
                target_fps: stats.fps,
                elapsed_seconds: stats.elapsed_seconds,
                resolution: stats.resolution,
                codec: stats.codec,
                bitrate: stats.bitrate,
            }
        })
    }

    /// Clean up the socket file
    pub fn cleanup(&self) {
        if self.socket_path.exists() {
            if let Err(e) = std::fs::remove_file(&self.socket_path) {
                warn!("Failed to remove socket file: {}", e);
            } else {
                debug!("Removed socket file {:?}", self.socket_path);
            }
        }
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        self.cleanup();
    }
}

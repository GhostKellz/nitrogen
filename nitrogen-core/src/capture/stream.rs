//! PipeWire stream handling for video capture
//!
//! Connects to the PipeWire graph and receives video frames from
//! the screencast portal session.

use std::os::fd::OwnedFd;
use std::sync::mpsc;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};

use crate::error::{NitrogenError, Result};
use crate::types::{Frame, FrameData, FrameFormat};

/// PipeWire capture stream
///
/// Receives video frames from a screencast portal session via PipeWire.
pub struct CaptureStream {
    /// Sender for frames to the processing pipeline
    frame_tx: broadcast::Sender<Arc<Frame>>,
    /// Thread handle for the PipeWire main loop
    pw_thread: Option<std::thread::JoinHandle<()>>,
    /// Channel to signal shutdown
    shutdown_tx: Option<mpsc::Sender<()>>,
}

impl CaptureStream {
    /// Create a new capture stream
    ///
    /// # Arguments
    /// * `fd` - PipeWire file descriptor from the portal
    /// * `node_id` - PipeWire node ID to connect to
    pub fn new(fd: OwnedFd, node_id: u32) -> Result<Self> {
        let (frame_tx, _) = broadcast::channel(4); // Small buffer for low latency
        let (shutdown_tx, _shutdown_rx) = mpsc::channel();

        // TODO: Implement proper PipeWire stream capture
        // This requires connecting to the portal's PipeWire node
        // and receiving video frames
        warn!(
            "Capture stream implementation is a stub - fd={:?}, node_id={}",
            fd, node_id
        );

        Ok(Self {
            frame_tx,
            pw_thread: None,
            shutdown_tx: Some(shutdown_tx),
        })
    }

    /// Subscribe to frames from this stream
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Frame>> {
        self.frame_tx.subscribe()
    }

    /// Check if the stream is still running
    pub fn is_running(&self) -> bool {
        self.pw_thread
            .as_ref()
            .map(|t| !t.is_finished())
            .unwrap_or(false)
    }

    /// Stop the capture stream
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(thread) = self.pw_thread.take() {
            let _ = thread.join();
        }
        info!("Capture stream stopped");
    }
}

impl Drop for CaptureStream {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_format() {
        let format = FrameFormat {
            width: 1920,
            height: 1080,
            fourcc: 0x34325258,
            stride: 1920 * 4,
        };
        assert_eq!(format.width, 1920);
        assert_eq!(format.height, 1080);
    }
}

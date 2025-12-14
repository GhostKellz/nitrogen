//! Main capture-encode-output pipeline
//!
//! Orchestrates the flow from screen capture through encoding to
//! virtual camera output.

use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tracing::{debug, error, info, warn};

use crate::capture::portal::{CaptureType, PortalCapture, SessionInfo};
use crate::capture::CaptureStream;
use crate::config::CaptureConfig;
use crate::encode::NvencEncoder;
use crate::error::{NitrogenError, Result};
use crate::output::VirtualCamera;
use crate::types::{Frame, Handle};

/// Main Nitrogen pipeline
///
/// Manages the complete flow from screen capture to virtual camera output.
pub struct Pipeline {
    /// Pipeline handle
    handle: Handle,
    /// Configuration
    config: CaptureConfig,
    /// Portal capture
    portal: PortalCapture,
    /// Capture stream (when active)
    capture: Option<CaptureStream>,
    /// Encoder (when active)
    encoder: Option<NvencEncoder>,
    /// Virtual camera (when active)
    camera: Option<VirtualCamera>,
    /// Pipeline state
    state: PipelineState,
}

/// Pipeline state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineState {
    /// Pipeline created but not started
    Idle,
    /// Waiting for user to select source via portal
    SelectingSource,
    /// Capture active, processing frames
    Running,
    /// Pipeline stopping
    Stopping,
    /// Pipeline stopped
    Stopped,
    /// Error state
    Error,
}

impl Pipeline {
    /// Create a new pipeline with the given configuration
    pub async fn new(config: CaptureConfig) -> Result<Self> {
        let portal = PortalCapture::new().await?;

        Ok(Self {
            handle: Handle::new(),
            config,
            portal,
            capture: None,
            encoder: None,
            camera: None,
            state: PipelineState::Idle,
        })
    }

    /// Get the pipeline handle
    pub fn handle(&self) -> Handle {
        self.handle
    }

    /// Get the current state
    pub fn state(&self) -> PipelineState {
        self.state
    }

    /// Start the pipeline
    ///
    /// This will:
    /// 1. Prompt the user to select a screen/window via the portal
    /// 2. Start the PipeWire capture stream
    /// 3. Initialize the NVENC encoder
    /// 4. Create the virtual camera
    /// 5. Begin processing frames
    pub async fn start(&mut self) -> Result<SessionInfo> {
        if self.state != PipelineState::Idle {
            return Err(NitrogenError::SessionAlreadyRunning);
        }

        self.state = PipelineState::SelectingSource;
        info!(
            "Starting pipeline {} with config: {:?}",
            self.handle, self.config
        );

        // Determine capture type from config
        let capture_type = match &self.config.source {
            crate::types::CaptureSource::Monitor { .. } => CaptureType::Monitor,
            crate::types::CaptureSource::Window { .. } => CaptureType::Window,
        };

        // Start portal session (will prompt user)
        let session_info = self.portal.start_session(capture_type, false).await?;

        info!(
            "Portal session started: {}x{}, node_id={}",
            session_info.width, session_info.height, session_info.node_id
        );

        // Get PipeWire fd and start capture stream
        let fd = self.portal.take_pipewire_fd().await?;
        let capture = CaptureStream::new(fd, session_info.node_id)?;
        self.capture = Some(capture);

        // Initialize encoder
        let encoder = NvencEncoder::new(&self.config)?;
        self.encoder = Some(encoder);

        // Create virtual camera
        let camera = VirtualCamera::new(
            &self.config.camera_name,
            self.config.width(),
            self.config.height(),
            self.config.fps(),
        )?;
        self.camera = Some(camera);

        self.state = PipelineState::Running;
        info!("Pipeline {} running", self.handle);

        Ok(session_info)
    }

    /// Process frames in the pipeline
    ///
    /// This should be called in a loop while the pipeline is running.
    /// Returns false when the pipeline should stop.
    pub async fn process(&mut self) -> Result<bool> {
        if self.state != PipelineState::Running {
            return Ok(false);
        }

        let capture = match &self.capture {
            Some(c) => c,
            None => return Ok(false),
        };

        if !capture.is_running() {
            warn!("Capture stream stopped unexpectedly");
            self.state = PipelineState::Error;
            return Ok(false);
        }

        // Get frame receiver
        let mut frame_rx = capture.subscribe();

        // Process one frame
        match frame_rx.recv().await {
            Ok(frame) => {
                // Encode frame
                if let Some(ref mut encoder) = self.encoder {
                    if let Err(e) = encoder.encode(&frame) {
                        error!("Encoding error: {}", e);
                        // Continue anyway - might recover
                    }
                }

                // TODO: Decode and send to virtual camera
                // For now, the encoder outputs encoded packets
                // For virtual camera, we need raw frames

                Ok(true)
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("Dropped {} frames due to lag", n);
                Ok(true)
            }
            Err(broadcast::error::RecvError::Closed) => {
                info!("Capture stream closed");
                Ok(false)
            }
        }
    }

    /// Stop the pipeline
    pub async fn stop(&mut self) -> Result<()> {
        if self.state == PipelineState::Stopped {
            return Ok(());
        }

        self.state = PipelineState::Stopping;
        info!("Stopping pipeline {}", self.handle);

        // Flush encoder
        if let Some(ref mut encoder) = self.encoder {
            let _ = encoder.flush();
        }

        // Stop capture
        if let Some(mut capture) = self.capture.take() {
            capture.stop();
        }

        // Stop camera
        if let Some(mut camera) = self.camera.take() {
            camera.stop();
        }

        // Stop portal session
        let _ = self.portal.stop_session().await;

        self.encoder = None;
        self.state = PipelineState::Stopped;

        info!("Pipeline {} stopped", self.handle);
        Ok(())
    }

    /// Check if the pipeline is running
    pub fn is_running(&self) -> bool {
        self.state == PipelineState::Running
    }

    /// Get pipeline statistics
    pub fn stats(&self) -> PipelineStats {
        PipelineStats {
            handle: self.handle,
            state: self.state,
            resolution: (self.config.width(), self.config.height()),
            fps: self.config.fps(),
            codec: self.config.codec.display_name().to_string(),
            bitrate: self.config.effective_bitrate(),
        }
    }
}

/// Pipeline statistics
#[derive(Debug, Clone)]
pub struct PipelineStats {
    /// Pipeline handle
    pub handle: Handle,
    /// Current state
    pub state: PipelineState,
    /// Output resolution
    pub resolution: (u32, u32),
    /// Target framerate
    pub fps: u32,
    /// Codec name
    pub codec: String,
    /// Target bitrate in kbps
    pub bitrate: u32,
}

impl std::fmt::Display for PipelineStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Pipeline {}: {:?} - {}x{} @ {}fps, {} @ {}kbps",
            self.handle,
            self.state,
            self.resolution.0,
            self.resolution.1,
            self.fps,
            self.codec,
            self.bitrate
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_state() {
        assert_eq!(PipelineState::Idle, PipelineState::Idle);
        assert_ne!(PipelineState::Idle, PipelineState::Running);
    }
}

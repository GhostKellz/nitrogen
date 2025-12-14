//! xdg-desktop-portal screencast integration
//!
//! Uses ashpd to communicate with the screencast portal for:
//! - Listing available monitors/windows
//! - Starting capture sessions
//! - Getting PipeWire node IDs for stream connection

use ashpd::desktop::screencast::{CursorMode, Screencast, SourceType};
use ashpd::{enumflags2::BitFlags, WindowIdentifier};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::error::{NitrogenError, Result};
use crate::types::{SourceInfo, SourceKind};

/// Portal-based capture session
pub struct PortalCapture {
    /// Screencast portal proxy
    screencast: Screencast<'static>,
    /// Active session (if any)
    session: Arc<Mutex<Option<ActiveSession>>>,
}

/// Active screencast session state
struct ActiveSession {
    /// PipeWire node ID for the stream
    pub node_id: u32,
    /// PipeWire file descriptor
    pub fd: std::os::fd::OwnedFd,
    /// Source kind that was selected
    pub source_kind: SourceKind,
}

impl PortalCapture {
    /// Create a new portal capture instance
    pub async fn new() -> Result<Self> {
        let screencast = Screencast::new().await?;
        Ok(Self {
            screencast,
            session: Arc::new(Mutex::new(None)),
        })
    }

    /// Start a screencast session
    ///
    /// This will prompt the user to select a screen/window via the portal.
    /// Returns the PipeWire node ID and file descriptor for stream connection.
    pub async fn start_session(
        &self,
        capture_type: CaptureType,
        multiple: bool,
    ) -> Result<SessionInfo> {
        let mut session_guard = self.session.lock().await;

        if session_guard.is_some() {
            return Err(NitrogenError::SessionAlreadyRunning);
        }

        info!("Creating screencast session via portal");

        // Create a new session
        let session = self.screencast.create_session().await?;

        // Select sources based on capture type
        let source_type: BitFlags<SourceType> = match capture_type {
            CaptureType::Monitor => SourceType::Monitor.into(),
            CaptureType::Window => SourceType::Window.into(),
            CaptureType::Both => SourceType::Monitor | SourceType::Window,
        };

        debug!("Requesting source selection: {:?}", source_type);

        // Select sources (this triggers the portal dialog)
        self.screencast
            .select_sources(
                &session,
                CursorMode::Embedded, // Show cursor in capture
                source_type,
                multiple,
                None, // No restore token for now
                ashpd::desktop::PersistMode::DoNot,
            )
            .await?;

        info!("Source selection complete, starting stream");

        // Start the screencast (use None for CLI apps without a window)
        let response = self
            .screencast
            .start(&session, None::<&WindowIdentifier>)
            .await?
            .response()?;

        let streams = response.streams();
        if streams.is_empty() {
            return Err(NitrogenError::portal("No streams returned from portal"));
        }

        let stream = &streams[0];
        let node_id = stream.pipe_wire_node_id();

        debug!("Got PipeWire node ID: {}", node_id);

        // Get the PipeWire file descriptor
        let fd = self.screencast.open_pipe_wire_remote(&session).await?;

        info!(
            "Screencast session started: node_id={}, fd={:?}",
            node_id, fd
        );

        // Determine source kind from stream properties
        let source_kind = stream
            .source_type()
            .map(|st| {
                if st == SourceType::Monitor {
                    SourceKind::Monitor
                } else {
                    SourceKind::Window
                }
            })
            .unwrap_or(SourceKind::Monitor);

        let active = ActiveSession {
            node_id,
            fd,
            source_kind,
        };

        // Get stream dimensions if available
        let (width, height) = stream.size().unwrap_or((1920, 1080));

        let info = SessionInfo {
            node_id,
            width: width.max(0) as u32,
            height: height.max(0) as u32,
            source_type: source_kind,
        };

        *session_guard = Some(active);

        Ok(info)
    }

    /// Get the PipeWire file descriptor for the active session
    pub async fn pipewire_fd(&self) -> Result<std::os::fd::BorrowedFd<'_>> {
        // This is tricky due to lifetimes - we need to return a reference to the fd
        // For now, return an error if not active
        Err(NitrogenError::NoActiveSession)
    }

    /// Get the PipeWire node ID for the active session
    pub async fn pipewire_node_id(&self) -> Result<u32> {
        let session = self.session.lock().await;
        session
            .as_ref()
            .map(|s| s.node_id)
            .ok_or(NitrogenError::NoActiveSession)
    }

    /// Take ownership of the session's PipeWire fd
    pub async fn take_pipewire_fd(&self) -> Result<std::os::fd::OwnedFd> {
        let mut session = self.session.lock().await;
        session
            .take()
            .map(|s| s.fd)
            .ok_or(NitrogenError::NoActiveSession)
    }

    /// Stop the active session
    pub async fn stop_session(&self) -> Result<()> {
        let mut session = self.session.lock().await;
        if session.take().is_some() {
            info!("Screencast session stopped");
            Ok(())
        } else {
            Err(NitrogenError::NoActiveSession)
        }
    }

    /// Check if a session is active
    pub async fn is_active(&self) -> bool {
        self.session.lock().await.is_some()
    }
}

/// Type of sources to capture
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureType {
    /// Only monitors/displays
    Monitor,
    /// Only windows
    Window,
    /// Both monitors and windows
    Both,
}

/// Information about a started session
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// PipeWire node ID for the stream
    pub node_id: u32,
    /// Stream width
    pub width: u32,
    /// Stream height
    pub height: u32,
    /// Type of source that was selected
    pub source_type: SourceKind,
}

/// List available capture sources
///
/// Note: On Wayland, this returns placeholder info since actual sources
/// are only revealed through the portal picker dialog.
pub async fn list_sources() -> Result<Vec<SourceInfo>> {
    // On Wayland, we can't enumerate sources directly without user interaction
    // The portal picker is the only way to select sources
    // We return a placeholder to indicate portal-based selection is needed

    warn!("Source listing on Wayland requires portal interaction");

    // We could try to get monitor info from other sources like wlr-output-management
    // or org.kde.KWin interfaces, but that's compositor-specific

    // For now, return placeholder sources that hint at using the portal
    Ok(vec![
        SourceInfo {
            id: "portal:screen".to_string(),
            name: "Select Screen (via Portal)".to_string(),
            kind: SourceKind::Monitor,
            dimensions: (0, 0), // Unknown until selected
            refresh_rate: None,
            hw_accelerated: true,
        },
        SourceInfo {
            id: "portal:window".to_string(),
            name: "Select Window (via Portal)".to_string(),
            kind: SourceKind::Window,
            dimensions: (0, 0),
            refresh_rate: None,
            hw_accelerated: true,
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_capture_type() {
        assert_eq!(CaptureType::Monitor, CaptureType::Monitor);
        assert_ne!(CaptureType::Monitor, CaptureType::Window);
    }
}

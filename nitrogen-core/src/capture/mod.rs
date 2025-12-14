//! Screen capture via xdg-desktop-portal and PipeWire
//!
//! This module handles:
//! - Source enumeration via portals
//! - Screencast session setup
//! - PipeWire stream connection for video frames

pub mod portal;
pub mod stream;

pub use portal::PortalCapture;
pub use stream::CaptureStream;

use crate::error::Result;
use crate::types::SourceInfo;

/// List available capture sources
///
/// This queries the xdg-desktop-portal for available screens and windows.
/// Note: On Wayland, the portal may return limited information until a
/// capture session is started (at which point the user picks the source).
pub async fn list_sources() -> Result<Vec<SourceInfo>> {
    portal::list_sources().await
}

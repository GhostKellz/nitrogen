//! Screen and audio capture via xdg-desktop-portal and PipeWire
//!
//! This module handles:
//! - Source enumeration via portals
//! - Screencast session setup
//! - PipeWire stream connection for video frames
//! - PipeWire audio capture (desktop and microphone)
//! - Audio mixing (combining multiple sources)
//! - (Future) Direct DRM capture for lower latency

pub mod audio;
pub mod drm;
pub mod mixer;
pub mod portal;
pub mod stream;

pub use audio::AudioCaptureStream;
pub use drm::DrmCapture;
pub use mixer::{AudioMixer, MixerConfig, VolumeControl};
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

/// Check PipeWire service status
///
/// Returns (is_running, status_message)
pub fn check_pipewire_status() -> (bool, String) {
    let output = std::process::Command::new("systemctl")
        .args(["--user", "is-active", "pipewire"])
        .output();

    match output {
        Ok(o) => {
            let status = String::from_utf8_lossy(&o.stdout).trim().to_string();
            (o.status.success(), status)
        }
        Err(_) => (false, "unknown".to_string()),
    }
}

/// Check xdg-desktop-portal service status
///
/// Returns (is_running, status_message)
pub fn check_portal_status() -> (bool, String) {
    let output = std::process::Command::new("systemctl")
        .args(["--user", "is-active", "xdg-desktop-portal"])
        .output();

    match output {
        Ok(o) => {
            let status = String::from_utf8_lossy(&o.stdout).trim().to_string();
            (o.status.success(), status)
        }
        Err(_) => (false, "unknown".to_string()),
    }
}

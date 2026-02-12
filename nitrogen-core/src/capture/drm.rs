//! Direct Rendering Manager (DRM) screen capture
//!
//! This module provides direct framebuffer capture through DRM/KMS,
//! bypassing the xdg-desktop-portal for lower latency and reduced overhead.
//!
//! # Status
//!
//! **NOT YET IMPLEMENTED** - This is a stub for future implementation.
//!
//! # Why DRM Capture?
//!
//! The default portal-based capture has several roundtrips and uses PipeWire
//! as an intermediary. DRM capture can provide:
//! - Lower latency (direct framebuffer access)
//! - Reduced CPU overhead (no PipeWire intermediary)
//! - Better integration with Gamescope
//!
//! # Requirements
//!
//! DRM capture requires:
//! - Root access OR appropriate DRM device permissions
//! - KMS (Kernel Mode Setting) support
//! - Compatible compositor (wlroots-based, Gamescope, etc.)
//!
//! Some compositors (GNOME/Mutter, KDE/KWin) don't expose DRM resources
//! for client use, making this approach compositor-specific.
//!
//! # Implementation Plan
//!
//! 1. **DRM Device Discovery**
//!    - Open `/dev/dri/card0` (or appropriate device)
//!    - Enumerate CRTCs, connectors, and framebuffers
//!    - Select the appropriate CRTC for capture
//!
//! 2. **Framebuffer Export**
//!    - Use `drmModeGetFB2()` to get framebuffer info
//!    - Export framebuffer as DMA-BUF using `drmPrimeHandleToFD()`
//!    - Handle tiled formats (modifier support)
//!
//! 3. **Frame Acquisition Loop**
//!    - Use `drmModePageFlip()` or `drmCrtcGetSequence()` for vsync
//!    - Capture framebuffer at each flip
//!    - Handle cursor overlay if needed
//!
//! 4. **Gamescope Integration**
//!    - Gamescope exposes internal surfaces via `xwayland-surface` atom
//!    - Can capture individual game windows directly
//!    - Supports HDR passthrough
//!
//! # Example (Conceptual)
//!
//! ```ignore
//! use nitrogen_core::capture::drm::DrmCapture;
//!
//! // This API doesn't exist yet - just showing the concept
//! let capture = DrmCapture::new("/dev/dri/card0")?;
//! capture.select_crtc(0)?;
//!
//! loop {
//!     let frame = capture.wait_frame()?;
//!     // frame.dmabuf_fd, frame.width, frame.height, frame.format
//! }
//! ```
//!
//! # References
//!
//! - [DRM/KMS Documentation](https://www.kernel.org/doc/html/latest/gpu/drm-kms.html)
//! - [Gamescope Source](https://github.com/ValveSoftware/gamescope)
//! - [drm-rs crate](https://crates.io/crates/drm)

use crate::error::{NitrogenError, Result};

/// DRM capture source (stub)
///
/// **Not yet implemented** - see module documentation for details.
pub struct DrmCapture {
    _device_path: String,
}

impl DrmCapture {
    /// Create a new DRM capture source
    ///
    /// # Errors
    ///
    /// Returns `Unsupported` - this feature is planned but not yet available.
    pub fn new(_device_path: &str) -> Result<Self> {
        Err(NitrogenError::Unsupported(
            "DRM capture is not yet implemented. Use portal capture (default) instead.".to_string()
        ))
    }

    /// Check if DRM capture is available on this system
    ///
    /// Returns `false` until implemented.
    pub fn is_available() -> bool {
        // TODO: Check for DRM device access and compatible compositor
        false
    }

    /// Get list of available DRM devices
    pub fn list_devices() -> Vec<String> {
        // TODO: Enumerate /dev/dri/card* devices
        Vec::new()
    }
}

/// Information about a DRM CRTC (stub)
#[derive(Debug, Clone)]
pub struct CrtcInfo {
    /// CRTC ID
    pub id: u32,
    /// Connected display name
    pub display: String,
    /// Current resolution
    pub resolution: (u32, u32),
    /// Current refresh rate in Hz
    pub refresh_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drm_not_available() {
        assert!(!DrmCapture::is_available());
    }

    #[test]
    fn test_drm_returns_not_implemented() {
        let result = DrmCapture::new("/dev/dri/card0");
        assert!(result.is_err());
    }
}

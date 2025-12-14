//! Core types for Nitrogen
//!
//! These types represent the fundamental data structures used throughout
//! the capture and streaming pipeline.

use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

/// Global handle counter for unique session IDs
static HANDLE_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Opaque handle for a capture session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle(u64);

impl Handle {
    /// Create a new unique handle
    pub fn new() -> Self {
        Self(HANDLE_COUNTER.fetch_add(1, Ordering::SeqCst))
    }

    /// Get the raw handle value
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl Default for Handle {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for Handle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Handle({})", self.0)
    }
}

/// Kind of capture source
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceKind {
    /// Full monitor/display capture
    Monitor,
    /// Individual window capture
    Window,
    /// Virtual source (e.g., from another application)
    Virtual,
}

impl std::fmt::Display for SourceKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SourceKind::Monitor => write!(f, "Monitor"),
            SourceKind::Window => write!(f, "Window"),
            SourceKind::Virtual => write!(f, "Virtual"),
        }
    }
}

/// What to capture
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum CaptureSource {
    /// Capture a monitor by its ID (e.g., "DP-2", "HDMI-1")
    Monitor {
        /// Monitor identifier
        id: String,
    },
    /// Capture a specific window
    Window {
        /// Window identifier (compositor-specific)
        id: String,
    },
}

impl CaptureSource {
    /// Create a monitor capture source
    pub fn monitor(id: impl Into<String>) -> Self {
        Self::Monitor { id: id.into() }
    }

    /// Create a window capture source
    pub fn window(id: impl Into<String>) -> Self {
        Self::Window { id: id.into() }
    }

    /// Get the source kind
    pub fn kind(&self) -> SourceKind {
        match self {
            Self::Monitor { .. } => SourceKind::Monitor,
            Self::Window { .. } => SourceKind::Window,
        }
    }

    /// Get the source ID
    pub fn id(&self) -> &str {
        match self {
            Self::Monitor { id } | Self::Window { id } => id,
        }
    }
}

impl std::fmt::Display for CaptureSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Monitor { id } => write!(f, "Monitor({})", id),
            Self::Window { id } => write!(f, "Window({})", id),
        }
    }
}

/// Information about an available capture source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Unique identifier for this source
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// What kind of source this is
    pub kind: SourceKind,
    /// Native resolution (width, height)
    pub dimensions: (u32, u32),
    /// Refresh rate in Hz (if known)
    pub refresh_rate: Option<f64>,
    /// Whether this source supports hardware capture
    pub hw_accelerated: bool,
}

impl SourceInfo {
    /// Create a new source info
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        kind: SourceKind,
        dimensions: (u32, u32),
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            dimensions,
            refresh_rate: None,
            hw_accelerated: false,
        }
    }

    /// Set the refresh rate
    pub fn with_refresh_rate(mut self, rate: f64) -> Self {
        self.refresh_rate = Some(rate);
        self
    }

    /// Set hardware acceleration support
    pub fn with_hw_accelerated(mut self, hw: bool) -> Self {
        self.hw_accelerated = hw;
        self
    }
}

impl std::fmt::Display for SourceInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} - {} ({}x{}",
            self.id, self.name, self.dimensions.0, self.dimensions.1
        )?;
        if let Some(rate) = self.refresh_rate {
            write!(f, " @ {:.0}Hz", rate)?;
        }
        write!(f, ")")
    }
}

/// Frame format information
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameFormat {
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Pixel format (DRM fourcc)
    pub fourcc: u32,
    /// Stride in bytes
    pub stride: u32,
}

/// Video frame data
#[derive(Debug)]
pub struct Frame {
    /// Frame format
    pub format: FrameFormat,
    /// Frame data (may be DMA-BUF fd or CPU buffer)
    pub data: FrameData,
    /// Presentation timestamp in nanoseconds
    pub pts: u64,
}

/// Frame data storage
#[derive(Debug)]
pub enum FrameData {
    /// DMA-BUF file descriptor for zero-copy GPU access
    DmaBuf {
        /// File descriptor
        fd: std::os::fd::RawFd,
        /// Offset into the buffer
        offset: u32,
        /// Modifier for tiled formats
        modifier: u64,
    },
    /// CPU-accessible memory
    Memory(Vec<u8>),
}

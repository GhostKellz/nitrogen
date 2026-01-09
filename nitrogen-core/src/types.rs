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

/// Audio sample format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioSampleFormat {
    /// 32-bit floating point, little-endian (interleaved)
    #[default]
    F32LE,
    /// 16-bit signed integer, little-endian (interleaved)
    S16LE,
    /// 32-bit signed integer, little-endian (interleaved)
    S32LE,
}

impl AudioSampleFormat {
    /// Bytes per sample
    pub fn bytes_per_sample(&self) -> usize {
        match self {
            AudioSampleFormat::F32LE => 4,
            AudioSampleFormat::S16LE => 2,
            AudioSampleFormat::S32LE => 4,
        }
    }
}

/// Audio format information
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormat {
    /// Sample rate in Hz (e.g., 48000)
    pub sample_rate: u32,
    /// Number of channels (1=mono, 2=stereo)
    pub channels: u32,
    /// Audio sample format
    pub format: AudioSampleFormat,
}

impl Default for AudioFormat {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            channels: 2,
            format: AudioSampleFormat::F32LE,
        }
    }
}

/// Audio frame data
#[derive(Debug, Clone)]
pub struct AudioFrame {
    /// Audio format
    pub format: AudioFormat,
    /// Sample data as f32 (converted from native format if needed)
    pub samples: Vec<f32>,
    /// Presentation timestamp in nanoseconds
    pub pts: u64,
    /// Number of samples per channel
    pub sample_count: u32,
}

impl AudioFrame {
    /// Create a new audio frame
    pub fn new(format: AudioFormat, samples: Vec<f32>, pts: u64) -> Self {
        let sample_count = (samples.len() / format.channels as usize) as u32;
        Self {
            format,
            samples,
            pts,
            sample_count,
        }
    }

    /// Duration of this frame in nanoseconds
    pub fn duration_ns(&self) -> u64 {
        (self.sample_count as u64 * 1_000_000_000) / self.format.sample_rate as u64
    }
}

impl FrameData {
    /// Try to map a DMA-BUF to CPU-accessible memory
    ///
    /// For DmaBuf variant, attempts to mmap the file descriptor and copy the data.
    /// For Memory variant, just clones the existing data.
    ///
    /// # Arguments
    /// * `size` - Expected size of the buffer in bytes
    ///
    /// # Returns
    /// * `Ok(Vec<u8>)` - The frame data copied to a Vec
    /// * `Err(String)` - Error message if mapping failed
    pub fn try_map_dmabuf(&self, size: usize) -> Result<Vec<u8>, String> {
        match self {
            FrameData::Memory(data) => Ok(data.clone()),
            FrameData::DmaBuf {
                fd,
                offset,
                modifier: _,
            } => {
                use std::ptr;

                // Safety: We're mapping a DMA-BUF fd that was passed to us from PipeWire.
                // The fd is borrowed (not owned), so we must not close it.
                // We use MAP_PRIVATE so our mapping is copy-on-write.
                let map_size = size + (*offset as usize);

                let ptr = unsafe {
                    libc::mmap(
                        ptr::null_mut(),
                        map_size,
                        libc::PROT_READ,
                        libc::MAP_PRIVATE,
                        *fd,
                        0,
                    )
                };

                if ptr == libc::MAP_FAILED {
                    let err = std::io::Error::last_os_error();
                    return Err(format!("mmap failed: {}", err));
                }

                // Copy the data out
                let data_ptr = unsafe { (ptr as *const u8).add(*offset as usize) };
                let mut buffer = vec![0u8; size];
                unsafe {
                    ptr::copy_nonoverlapping(data_ptr, buffer.as_mut_ptr(), size);
                }

                // Unmap the memory (but don't close the fd - it's borrowed)
                let unmap_result = unsafe { libc::munmap(ptr, map_size) };
                if unmap_result != 0 {
                    // Log but don't fail - we already have the data
                    let err = std::io::Error::last_os_error();
                    tracing::warn!("munmap failed: {}", err);
                }

                Ok(buffer)
            }
        }
    }

    /// Check if this is a DMA-BUF frame
    pub fn is_dmabuf(&self) -> bool {
        matches!(self, FrameData::DmaBuf { .. })
    }

    /// Get the memory data if this is a Memory variant
    pub fn as_memory(&self) -> Option<&[u8]> {
        match self {
            FrameData::Memory(data) => Some(data),
            FrameData::DmaBuf { .. } => None,
        }
    }
}

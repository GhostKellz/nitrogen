//! Configuration types for Nitrogen
//!
//! Provides capture presets, encoder settings, and runtime configuration.

use crate::types::CaptureSource;
use serde::{Deserialize, Serialize};

/// Video codec for encoding
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Codec {
    /// H.264 / AVC (most compatible)
    #[default]
    H264,
    /// H.265 / HEVC (better compression)
    Hevc,
    /// AV1 (best compression, newer GPUs only)
    Av1,
}

impl Codec {
    /// Get the FFmpeg encoder name for NVENC
    pub fn nvenc_encoder(&self) -> &'static str {
        match self {
            Self::H264 => "h264_nvenc",
            Self::Hevc => "hevc_nvenc",
            Self::Av1 => "av1_nvenc",
        }
    }

    /// Get the codec name for display
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::H264 => "H.264",
            Self::Hevc => "HEVC",
            Self::Av1 => "AV1",
        }
    }
}

impl std::fmt::Display for Codec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

impl std::str::FromStr for Codec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "h264" | "avc" | "264" => Ok(Self::H264),
            "hevc" | "h265" | "265" => Ok(Self::Hevc),
            "av1" => Ok(Self::Av1),
            _ => Err(format!("Unknown codec: {}", s)),
        }
    }
}

/// Quality preset for encoding
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EncoderPreset {
    /// Fastest encoding, lowest quality
    Fast,
    /// Balanced speed and quality
    Medium,
    /// Slower encoding, better quality
    Slow,
    /// Best quality, slowest encoding
    Quality,
}

impl Default for EncoderPreset {
    fn default() -> Self {
        Self::Medium
    }
}

impl EncoderPreset {
    /// Get the NVENC preset string
    pub fn nvenc_preset(&self) -> &'static str {
        match self {
            Self::Fast => "p1",    // fastest
            Self::Medium => "p4",  // balanced
            Self::Slow => "p6",    // slower but better
            Self::Quality => "p7", // best quality
        }
    }
}

/// Common resolution presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Preset {
    /// 1280x720 @ 30fps (720p30)
    #[serde(rename = "720p30")]
    P720_30,
    /// 1280x720 @ 60fps (720p60)
    #[serde(rename = "720p60")]
    P720_60,
    /// 1920x1080 @ 30fps (1080p30)
    #[serde(rename = "1080p30")]
    P1080_30,
    /// 1920x1080 @ 60fps (1080p60)
    #[serde(rename = "1080p60")]
    P1080_60,
    /// 2560x1440 @ 30fps (1440p30)
    #[serde(rename = "1440p30")]
    P1440_30,
    /// 2560x1440 @ 60fps (1440p60)
    #[serde(rename = "1440p60")]
    P1440_60,
    /// 2560x1440 @ 120fps (1440p120)
    #[serde(rename = "1440p120")]
    P1440_120,
    /// 3840x2160 @ 30fps (4K30)
    #[serde(rename = "4k30")]
    P4K_30,
    /// 3840x2160 @ 60fps (4K60)
    #[serde(rename = "4k60")]
    P4K_60,
    /// 3840x2160 @ 120fps (4K120)
    #[serde(rename = "4k120")]
    P4K_120,
    /// Custom resolution and framerate
    Custom {
        width: u32,
        height: u32,
        fps: u32,
    },
}

impl Default for Preset {
    fn default() -> Self {
        Self::P1080_60
    }
}

impl Preset {
    /// Get the resolution (width, height)
    pub fn resolution(&self) -> (u32, u32) {
        match self {
            Self::P720_30 | Self::P720_60 => (1280, 720),
            Self::P1080_30 | Self::P1080_60 => (1920, 1080),
            Self::P1440_30 | Self::P1440_60 | Self::P1440_120 => (2560, 1440),
            Self::P4K_30 | Self::P4K_60 | Self::P4K_120 => (3840, 2160),
            Self::Custom { width, height, .. } => (*width, *height),
        }
    }

    /// Get the framerate
    pub fn fps(&self) -> u32 {
        match self {
            Self::P720_30 | Self::P1080_30 | Self::P1440_30 | Self::P4K_30 => 30,
            Self::P720_60 | Self::P1080_60 | Self::P1440_60 | Self::P4K_60 => 60,
            Self::P1440_120 | Self::P4K_120 => 120,
            Self::Custom { fps, .. } => *fps,
        }
    }

    /// Get width
    pub fn width(&self) -> u32 {
        self.resolution().0
    }

    /// Get height
    pub fn height(&self) -> u32 {
        self.resolution().1
    }

    /// Suggested bitrate in kbps
    pub fn suggested_bitrate(&self) -> u32 {
        match self {
            Self::P720_30 => 2500,
            Self::P720_60 => 4000,
            Self::P1080_30 => 4500,
            Self::P1080_60 => 6000,
            Self::P1440_30 => 8000,
            Self::P1440_60 => 12000,
            Self::P1440_120 => 20000,
            Self::P4K_30 => 15000,
            Self::P4K_60 => 25000,
            Self::P4K_120 => 40000,
            Self::Custom { width, height, fps } => {
                // Rough estimate: pixels * fps * 0.1 bits per pixel / 1000
                let pixels = width * height;
                (pixels as u64 * *fps as u64 / 10000) as u32
            }
        }
    }

    /// Parse preset from string like "1080p60"
    pub fn from_preset_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "720p30" => Some(Self::P720_30),
            "720p60" => Some(Self::P720_60),
            "1080p30" => Some(Self::P1080_30),
            "1080p60" => Some(Self::P1080_60),
            "1440p30" => Some(Self::P1440_30),
            "1440p60" => Some(Self::P1440_60),
            "1440p120" => Some(Self::P1440_120),
            "4k30" | "2160p30" => Some(Self::P4K_30),
            "4k60" | "2160p60" => Some(Self::P4K_60),
            "4k120" | "2160p120" => Some(Self::P4K_120),
            _ => None,
        }
    }
}

impl std::fmt::Display for Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::P720_30 => write!(f, "720p30"),
            Self::P720_60 => write!(f, "720p60"),
            Self::P1080_30 => write!(f, "1080p30"),
            Self::P1080_60 => write!(f, "1080p60"),
            Self::P1440_30 => write!(f, "1440p30"),
            Self::P1440_60 => write!(f, "1440p60"),
            Self::P1440_120 => write!(f, "1440p120"),
            Self::P4K_30 => write!(f, "4K30"),
            Self::P4K_60 => write!(f, "4K60"),
            Self::P4K_120 => write!(f, "4K120"),
            Self::Custom { width, height, fps } => {
                write!(f, "{}x{}@{}fps", width, height, fps)
            }
        }
    }
}

impl std::str::FromStr for Preset {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_preset_str(s).ok_or_else(|| format!("Unknown preset: {}", s))
    }
}

/// Full capture configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    /// What to capture
    pub source: CaptureSource,
    /// Output resolution and framerate preset
    #[serde(default)]
    pub preset: Preset,
    /// Video codec
    #[serde(default)]
    pub codec: Codec,
    /// Target bitrate in kbps (0 = auto based on preset)
    #[serde(default)]
    pub bitrate: u32,
    /// Encoder quality preset
    #[serde(default)]
    pub encoder_preset: EncoderPreset,
    /// Virtual camera name
    #[serde(default = "default_camera_name")]
    pub camera_name: String,
    /// Enable low-latency mode
    #[serde(default = "default_true")]
    pub low_latency: bool,
}

fn default_camera_name() -> String {
    "Nitrogen Camera".to_string()
}

fn default_true() -> bool {
    true
}

impl CaptureConfig {
    /// Create a new config for monitor capture
    pub fn monitor(id: impl Into<String>) -> Self {
        Self {
            source: CaptureSource::monitor(id),
            preset: Preset::default(),
            codec: Codec::default(),
            bitrate: 0,
            encoder_preset: EncoderPreset::default(),
            camera_name: default_camera_name(),
            low_latency: true,
        }
    }

    /// Create a new config for window capture
    pub fn window(id: impl Into<String>) -> Self {
        Self {
            source: CaptureSource::window(id),
            preset: Preset::default(),
            codec: Codec::default(),
            bitrate: 0,
            encoder_preset: EncoderPreset::default(),
            camera_name: default_camera_name(),
            low_latency: true,
        }
    }

    /// Set the output preset
    pub fn with_preset(mut self, preset: Preset) -> Self {
        self.preset = preset;
        self
    }

    /// Set the codec
    pub fn with_codec(mut self, codec: Codec) -> Self {
        self.codec = codec;
        self
    }

    /// Set the bitrate in kbps
    pub fn with_bitrate(mut self, bitrate: u32) -> Self {
        self.bitrate = bitrate;
        self
    }

    /// Set the encoder quality preset
    pub fn with_encoder_preset(mut self, preset: EncoderPreset) -> Self {
        self.encoder_preset = preset;
        self
    }

    /// Set the virtual camera name
    pub fn with_camera_name(mut self, name: impl Into<String>) -> Self {
        self.camera_name = name.into();
        self
    }

    /// Get the effective bitrate (uses suggested if 0)
    pub fn effective_bitrate(&self) -> u32 {
        if self.bitrate > 0 {
            self.bitrate
        } else {
            self.preset.suggested_bitrate()
        }
    }

    /// Get output width
    pub fn width(&self) -> u32 {
        self.preset.width()
    }

    /// Get output height
    pub fn height(&self) -> u32 {
        self.preset.height()
    }

    /// Get output framerate
    pub fn fps(&self) -> u32 {
        self.preset.fps()
    }
}

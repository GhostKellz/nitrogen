//! Configuration types for Nitrogen
//!
//! Provides capture presets, encoder settings, and runtime configuration.

mod av1;
mod file;

pub use av1::{Av1Config, Av1Tier, Av1Tune, ChromaFormat, MultipassMode};
pub use file::{sample_config, ConfigFile};

use crate::types::CaptureSource;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

/// Cursor capture mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CursorMode {
    /// Hide cursor in capture
    Hidden,
    /// Show cursor embedded in capture frames
    #[default]
    Embedded,
    /// Cursor metadata only (compositor-dependent)
    Metadata,
}

/// Audio capture source
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AudioSource {
    /// No audio capture
    #[default]
    None,
    /// Capture desktop/system audio (what you hear)
    Desktop,
    /// Capture microphone input
    Microphone,
    /// Capture both desktop and microphone
    Both,
}

impl std::fmt::Display for AudioSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::None => write!(f, "none"),
            Self::Desktop => write!(f, "desktop"),
            Self::Microphone => write!(f, "microphone"),
            Self::Both => write!(f, "both"),
        }
    }
}

/// Audio codec for encoding
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AudioCodec {
    /// AAC (most compatible)
    #[default]
    Aac,
    /// Opus (better quality at low bitrates)
    Opus,
    /// Copy/passthrough (no re-encoding)
    Copy,
}

impl AudioCodec {
    /// Get the FFmpeg encoder name
    pub fn ffmpeg_encoder(&self) -> &'static str {
        match self {
            Self::Aac => "aac",
            Self::Opus => "libopus",
            Self::Copy => "copy",
        }
    }

    /// Get default bitrate for this codec in kbps
    pub fn default_bitrate(&self) -> u32 {
        match self {
            Self::Aac => 192,
            Self::Opus => 128,
            Self::Copy => 0,
        }
    }
}

impl std::fmt::Display for AudioCodec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Aac => write!(f, "AAC"),
            Self::Opus => write!(f, "Opus"),
            Self::Copy => write!(f, "Copy"),
        }
    }
}

/// Encoder quality preset
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EncoderPreset {
    /// Fast encoding, lower quality
    Fast,
    /// Balanced encoding (default)
    #[default]
    Medium,
    /// Slower encoding, better quality
    Slow,
    /// Best quality, slowest encoding
    Quality,
}

impl EncoderPreset {
    /// Get NVENC preset name
    pub fn nvenc_preset(&self) -> &'static str {
        match self {
            Self::Fast => "p1",
            Self::Medium => "p4",
            Self::Slow => "p6",
            Self::Quality => "p7",
        }
    }
}

/// Output resolution/framerate preset
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Preset {
    /// 1280x720 @ 30fps
    #[serde(rename = "720p30")]
    P720_30,
    /// 1280x720 @ 60fps
    #[serde(rename = "720p60")]
    P720_60,
    /// 1920x1080 @ 30fps
    #[serde(rename = "1080p30")]
    P1080_30,
    /// 1920x1080 @ 60fps (default)
    #[default]
    #[serde(rename = "1080p60")]
    P1080_60,
    /// 2560x1440 @ 30fps
    #[serde(rename = "1440p30")]
    P1440_30,
    /// 2560x1440 @ 60fps
    #[serde(rename = "1440p60")]
    P1440_60,
    /// 2560x1440 @ 120fps
    #[serde(rename = "1440p120")]
    P1440_120,
    /// 3840x2160 @ 30fps
    #[serde(rename = "4k30")]
    P4K_30,
    /// 3840x2160 @ 60fps
    #[serde(rename = "4k60")]
    P4K_60,
    /// 3840x2160 @ 120fps
    #[serde(rename = "4k120")]
    P4K_120,
    /// Custom resolution/framerate
    Custom {
        width: u32,
        height: u32,
        fps: u32,
    },
}

impl Preset {
    /// Get width in pixels
    pub fn width(&self) -> u32 {
        match self {
            Self::P720_30 | Self::P720_60 => 1280,
            Self::P1080_30 | Self::P1080_60 => 1920,
            Self::P1440_30 | Self::P1440_60 | Self::P1440_120 => 2560,
            Self::P4K_30 | Self::P4K_60 | Self::P4K_120 => 3840,
            Self::Custom { width, .. } => *width,
        }
    }

    /// Get height in pixels
    pub fn height(&self) -> u32 {
        match self {
            Self::P720_30 | Self::P720_60 => 720,
            Self::P1080_30 | Self::P1080_60 => 1080,
            Self::P1440_30 | Self::P1440_60 | Self::P1440_120 => 1440,
            Self::P4K_30 | Self::P4K_60 | Self::P4K_120 => 2160,
            Self::Custom { height, .. } => *height,
        }
    }

    /// Get framerate
    pub fn fps(&self) -> u32 {
        match self {
            Self::P720_30 | Self::P1080_30 | Self::P1440_30 | Self::P4K_30 => 30,
            Self::P720_60 | Self::P1080_60 | Self::P1440_60 | Self::P4K_60 => 60,
            Self::P1440_120 | Self::P4K_120 => 120,
            Self::Custom { fps, .. } => *fps,
        }
    }

    /// Get suggested bitrate in kbps
    pub fn suggested_bitrate(&self) -> u32 {
        match self {
            Self::P720_30 => 2500,
            Self::P720_60 => 4000,
            Self::P1080_30 => 4500,
            Self::P1080_60 => 6000,
            Self::P1440_30 => 9000,
            Self::P1440_60 => 12000,
            Self::P1440_120 => 18000,
            Self::P4K_30 => 20000,
            Self::P4K_60 => 35000,
            Self::P4K_120 => 50000,
            Self::Custom { width, height, fps } => {
                // Estimate based on pixels per second
                let pixels_per_second = (*width as u64) * (*height as u64) * (*fps as u64);
                // Roughly 0.07 bits per pixel for decent quality
                ((pixels_per_second * 7) / 100_000) as u32
            }
        }
    }

    /// Parse from string, returning Option instead of Result
    pub fn from_preset_str(s: &str) -> Option<Self> {
        s.parse().ok()
    }

    /// Get resolution as (width, height) tuple
    pub fn resolution(&self) -> (u32, u32) {
        (self.width(), self.height())
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
            Self::Custom { width, height, fps } => write!(f, "{}x{}@{}", width, height, fps),
        }
    }
}

impl std::str::FromStr for Preset {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "720p30" => Ok(Self::P720_30),
            "720p60" => Ok(Self::P720_60),
            "1080p30" => Ok(Self::P1080_30),
            "1080p60" => Ok(Self::P1080_60),
            "1440p30" | "2k30" => Ok(Self::P1440_30),
            "1440p60" | "2k60" => Ok(Self::P1440_60),
            "1440p120" | "2k120" => Ok(Self::P1440_120),
            "4k30" | "2160p30" => Ok(Self::P4K_30),
            "4k60" | "2160p60" => Ok(Self::P4K_60),
            "4k120" | "2160p120" => Ok(Self::P4K_120),
            _ => Err(format!("Unknown preset: {}", s)),
        }
    }
}

/// Complete capture configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    /// Capture source (monitor or window)
    pub source: CaptureSource,
    /// Output preset (resolution/framerate)
    pub preset: Preset,
    /// Video codec
    pub codec: Codec,
    /// Bitrate in kbps (0 = auto)
    pub bitrate: u32,
    /// Encoder quality preset
    pub encoder_preset: EncoderPreset,
    /// Virtual camera name
    pub camera_name: String,
    /// Enable low-latency mode
    pub low_latency: bool,
    /// GPU index for encoding
    pub gpu: u32,
    /// Optional recording file path
    pub record_path: Option<std::path::PathBuf>,
    /// Cursor capture mode
    pub cursor_mode: CursorMode,
    /// Audio capture source
    pub audio_source: AudioSource,
    /// AV1-specific configuration
    pub av1: Av1Config,
    /// Audio codec
    pub audio_codec: AudioCodec,
    /// Audio bitrate in kbps (0 = auto)
    pub audio_bitrate: u32,
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
            gpu: 0,
            record_path: None,
            cursor_mode: CursorMode::default(),
            audio_source: AudioSource::default(),
            av1: Av1Config::default(),
            audio_codec: AudioCodec::default(),
            audio_bitrate: 0,
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
            gpu: 0,
            record_path: None,
            cursor_mode: CursorMode::default(),
            audio_source: AudioSource::default(),
            av1: Av1Config::default(),
            audio_codec: AudioCodec::default(),
            audio_bitrate: 0,
        }
    }

    /// Set the GPU index for encoding
    pub fn with_gpu(mut self, gpu: u32) -> Self {
        self.gpu = gpu;
        self
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

    /// Set the recording output path
    pub fn with_record_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.record_path = Some(path.into());
        self
    }

    /// Set the cursor capture mode
    pub fn with_cursor_mode(mut self, mode: CursorMode) -> Self {
        self.cursor_mode = mode;
        self
    }

    /// Set the audio capture source
    pub fn with_audio_source(mut self, source: AudioSource) -> Self {
        self.audio_source = source;
        self
    }

    /// Set the audio codec
    pub fn with_audio_codec(mut self, codec: AudioCodec) -> Self {
        self.audio_codec = codec;
        self
    }

    /// Set the audio bitrate in kbps
    pub fn with_audio_bitrate(mut self, bitrate: u32) -> Self {
        self.audio_bitrate = bitrate;
        self
    }

    /// Set AV1-specific configuration
    pub fn with_av1(mut self, av1: Av1Config) -> Self {
        self.av1 = av1;
        self
    }

    /// Set AV1 tier (main or high)
    pub fn with_av1_tier(mut self, tier: Av1Tier) -> Self {
        self.av1.tier = tier;
        self
    }

    /// Enable 10-bit AV1 encoding
    pub fn with_av1_10bit(mut self, enabled: bool) -> Self {
        self.av1.ten_bit = enabled;
        self
    }

    /// Enable AV1 lookahead
    pub fn with_av1_lookahead(mut self, enabled: bool) -> Self {
        self.av1.lookahead = enabled;
        self
    }

    /// Enable AV1 spatial adaptive quantization
    pub fn with_av1_spatial_aq(mut self, enabled: bool) -> Self {
        self.av1.spatial_aq = enabled;
        self
    }

    /// Check if audio capture is enabled
    pub fn has_audio(&self) -> bool {
        self.audio_source != AudioSource::None
    }

    /// Get the effective audio bitrate (uses default if 0)
    pub fn effective_audio_bitrate(&self) -> u32 {
        if self.audio_bitrate > 0 {
            self.audio_bitrate
        } else {
            match self.audio_codec {
                AudioCodec::Aac => 192,
                AudioCodec::Opus => 128,
                AudioCodec::Copy => 0,
            }
        }
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

    /// Validate the configuration and return any warnings
    ///
    /// Returns a list of warning messages for potentially problematic settings.
    /// An empty list means the configuration looks good.
    pub fn validate(&self) -> Vec<String> {
        let mut warnings = Vec::new();

        // Check bitrate ranges
        let effective = self.effective_bitrate();
        let suggested = self.preset.suggested_bitrate();

        if self.bitrate > 0 {
            // User specified explicit bitrate
            if effective < suggested / 4 {
                warnings.push(format!(
                    "Bitrate {} kbps is very low for {} (suggested: {} kbps). Quality may suffer.",
                    effective, self.preset, suggested
                ));
            } else if effective > suggested * 3 {
                warnings.push(format!(
                    "Bitrate {} kbps is very high for {} (suggested: {} kbps). May be wasteful without quality improvement.",
                    effective, self.preset, suggested
                ));
            }

            // Absolute limits
            if effective < 500 {
                warnings.push(
                    "Bitrate below 500 kbps will likely produce poor quality video.".to_string(),
                );
            }
            if effective > 100_000 {
                warnings.push("Bitrate above 100 Mbps is excessive for streaming.".to_string());
            }
        }

        // Check for high-bandwidth configurations that might stress the system
        let pixels_per_second =
            (self.width() as u64) * (self.height() as u64) * (self.fps() as u64);
        if pixels_per_second > 500_000_000 {
            // 4K@60 is ~500M pixels/second
            warnings.push(format!(
                "High resolution/framerate ({}) requires significant GPU encoding power.",
                self.preset
            ));
        }

        // AV1 encoding is more demanding
        if self.codec == Codec::Av1 && self.fps() > 60 {
            warnings.push("AV1 encoding at high framerates may cause performance issues. Consider HEVC or H.264.".to_string());
        }

        // Check if 120fps is selected (Discord doesn't support >60fps)
        if self.fps() > 60 {
            warnings.push(format!(
                "{}fps exceeds Discord's maximum of 60fps. Frames may be dropped by Discord.",
                self.fps()
            ));
        }

        warnings
    }

    /// Validate and return an error if configuration is invalid
    ///
    /// Unlike `validate()` which returns warnings, this returns hard errors
    /// for configurations that cannot work.
    pub fn validate_strict(&self) -> Result<(), String> {
        // Check for zero dimensions
        if self.width() == 0 || self.height() == 0 {
            return Err("Resolution cannot be zero".to_string());
        }

        // Check for zero framerate
        if self.fps() == 0 {
            return Err("Framerate cannot be zero".to_string());
        }

        // Check reasonable resolution limits
        if self.width() > 7680 || self.height() > 4320 {
            return Err(format!(
                "Resolution {}x{} exceeds maximum supported (7680x4320)",
                self.width(),
                self.height()
            ));
        }

        // Check reasonable framerate limits
        if self.fps() > 240 {
            return Err(format!(
                "Framerate {} exceeds maximum supported (240)",
                self.fps()
            ));
        }

        Ok(())
    }
}

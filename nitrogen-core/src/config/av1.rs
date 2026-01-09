//! AV1-specific configuration and helpers
//!
//! Supports RTX 40 series (Ada) and RTX 50 series (Blackwell) features:
//! - RTX 40+: AV1 encoding, tier selection, spatial AQ
//! - RTX 50+: Ultra High Quality mode, temporal filtering, 4:2:2 chroma, extended lookahead

use serde::{Deserialize, Serialize};

/// NVENC AV1 tier selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Av1Tier {
    /// Main tier (broad compatibility)
    #[default]
    Main,
    /// High tier (allows higher bitrates/resolutions on Ada+)
    High,
}

impl Av1Tier {
    /// Returns ffmpeg option string for NVENC
    pub fn ffmpeg_value(&self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::High => "high",
        }
    }
}

/// NVENC tuning mode for quality optimization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Av1Tune {
    /// High quality (default, good balance)
    #[default]
    Hq,
    /// Ultra High Quality - RTX 50 series only (best quality, ~5% better compression)
    Uhq,
    /// Low latency (for streaming)
    Ll,
    /// Ultra low latency (minimal latency)
    Ull,
    /// Lossless encoding
    Lossless,
}

impl Av1Tune {
    /// Returns ffmpeg tune option string
    pub fn ffmpeg_value(&self) -> &'static str {
        match self {
            Self::Hq => "hq",
            Self::Uhq => "uhq",
            Self::Ll => "ll",
            Self::Ull => "ull",
            Self::Lossless => "lossless",
        }
    }

    /// Check if this tune mode requires RTX 50 series
    pub fn requires_blackwell(&self) -> bool {
        matches!(self, Self::Uhq)
    }
}

/// Chroma subsampling format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ChromaFormat {
    /// 4:2:0 - Standard, most compatible (default)
    #[default]
    #[serde(rename = "420")]
    Yuv420,
    /// 4:2:2 - Pro-grade, RTX 50 series only (better color fidelity)
    #[serde(rename = "422")]
    Yuv422,
    /// 4:4:4 - Full chroma, highest quality
    #[serde(rename = "444")]
    Yuv444,
}

impl ChromaFormat {
    /// Check if this format requires RTX 50 series
    pub fn requires_blackwell(&self) -> bool {
        matches!(self, Self::Yuv422)
    }
}

/// Multipass encoding mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MultipassMode {
    /// Disabled (fastest)
    #[default]
    Disabled,
    /// Quarter resolution first pass
    Quarter,
    /// Full resolution first pass (best quality)
    Full,
}

impl MultipassMode {
    /// Returns ffmpeg multipass option string
    pub fn ffmpeg_value(&self) -> Option<&'static str> {
        match self {
            Self::Disabled => None,
            Self::Quarter => Some("qres"),
            Self::Full => Some("fullres"),
        }
    }
}

/// AV1 tuning options with RTX 50 series enhancements
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Av1Config {
    /// Enable 10-bit surfaces (main10 profile)
    pub ten_bit: bool,
    /// Tier selection (main/high)
    pub tier: Av1Tier,
    /// GOP length override (frames)
    pub gop_length: Option<u32>,
    /// Enable lookahead (if supported)
    pub lookahead: bool,
    /// Lookahead depth (frames, RTX 50: up to 250, default 20)
    pub lookahead_depth: u32,
    /// Enable spatial AQ (adaptive quantization)
    pub spatial_aq: bool,
    /// Enable temporal AQ (RTX 50 series, ~4-5% efficiency gain)
    pub temporal_aq: bool,
    /// Tuning mode (hq, uhq for RTX 50, ll, ull)
    pub tune: Av1Tune,
    /// Chroma subsampling format (420, 422 for RTX 50, 444)
    pub chroma: ChromaFormat,
    /// Multipass encoding mode
    pub multipass: MultipassMode,
    /// B-frame reference mode for better compression (RTX 50)
    pub b_ref_mode: bool,
}

impl Default for Av1Config {
    fn default() -> Self {
        Self {
            ten_bit: false,
            tier: Av1Tier::Main,
            gop_length: None,
            lookahead: false,
            lookahead_depth: 20,
            spatial_aq: true,
            temporal_aq: false,
            tune: Av1Tune::Hq,
            chroma: ChromaFormat::Yuv420,
            multipass: MultipassMode::Disabled,
            b_ref_mode: false,
        }
    }
}

impl Av1Config {
    /// Resolve GOP length (default 2x target FPS, minimum 1)
    pub fn resolved_gop(&self, fps: u32) -> u32 {
        self.gop_length.unwrap_or_else(|| fps.max(1) * 2)
    }

    /// Create config optimized for RTX 50 series (Blackwell)
    pub fn blackwell_optimized() -> Self {
        Self {
            ten_bit: true,
            tier: Av1Tier::High,
            gop_length: None,
            lookahead: true,
            lookahead_depth: 250, // Extended lookahead on Blackwell
            spatial_aq: true,
            temporal_aq: true, // New on Blackwell
            tune: Av1Tune::Uhq, // Ultra High Quality mode
            chroma: ChromaFormat::Yuv420, // 422 available but less compatible
            multipass: MultipassMode::Full,
            b_ref_mode: true,
        }
    }

    /// Create config for low-latency streaming on RTX 50 series
    pub fn blackwell_streaming() -> Self {
        Self {
            ten_bit: false,
            tier: Av1Tier::Main,
            gop_length: Some(60), // 1 second at 60fps
            lookahead: false,
            lookahead_depth: 0,
            spatial_aq: true,
            temporal_aq: false,
            tune: Av1Tune::Ll,
            chroma: ChromaFormat::Yuv420,
            multipass: MultipassMode::Disabled,
            b_ref_mode: false,
        }
    }

    /// Check if this config uses RTX 50 series features
    pub fn uses_blackwell_features(&self) -> bool {
        self.tune.requires_blackwell()
            || self.chroma.requires_blackwell()
            || self.temporal_aq
            || self.lookahead_depth > 32
    }
}

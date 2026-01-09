//! Configuration file loading and merging
//!
//! Loads user configuration from `~/.config/nitrogen/config.toml`

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, info, warn};

use crate::error::{NitrogenError, Result};

/// Configuration file structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConfigFile {
    /// Default settings
    #[serde(default)]
    pub defaults: DefaultSettings,

    /// Encoder settings
    #[serde(default)]
    pub encoder: EncoderSettings,

    /// AV1 codec settings
    #[serde(default)]
    pub av1: Av1Settings,

    /// Virtual camera settings
    #[serde(default)]
    pub camera: CameraSettings,

    /// Audio settings
    #[serde(default)]
    pub audio: AudioSettings,
}

/// Default capture settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultSettings {
    /// Default output preset (e.g., "1080p60")
    #[serde(default = "default_preset")]
    pub preset: String,

    /// Default codec (h264, hevc, av1)
    #[serde(default = "default_codec")]
    pub codec: String,

    /// Default bitrate in kbps (0 = auto)
    #[serde(default)]
    pub bitrate: u32,

    /// Enable low latency mode by default
    #[serde(default = "default_true")]
    pub low_latency: bool,
}

/// Encoder-specific settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncoderSettings {
    /// Encoder quality preset (fast, medium, slow, quality)
    #[serde(default = "default_quality")]
    pub quality: String,

    /// GPU index to use (0 = first GPU)
    #[serde(default)]
    pub gpu: u32,
}

/// Virtual camera settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraSettings {
    /// Virtual camera name
    #[serde(default = "default_camera_name")]
    pub name: String,
}

/// Audio capture and encoding settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSettings {
    /// Audio source (none, desktop, mic, both)
    #[serde(default = "default_audio_source")]
    pub source: String,

    /// Audio codec (aac, opus)
    #[serde(default = "default_audio_codec")]
    pub codec: String,

    /// Audio bitrate in kbps (0 = auto based on codec)
    #[serde(default)]
    pub bitrate: u32,
}

/// AV1 codec-specific settings (RTX 40/50 series)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Av1Settings {
    /// Enable 10-bit encoding (main10 profile)
    #[serde(default)]
    pub ten_bit: bool,

    /// AV1 tier: main (default) or high (RTX 40+ for higher bitrates)
    #[serde(default = "default_av1_tier")]
    pub tier: String,

    /// Tuning mode: hq (default), uhq (RTX 50 only), ll, ull
    #[serde(default = "default_av1_tune")]
    pub tune: String,

    /// Enable lookahead for better quality (disabled in low-latency mode)
    #[serde(default)]
    pub lookahead: bool,

    /// Lookahead depth in frames (RTX 50: up to 250, default 20)
    #[serde(default = "default_lookahead_depth")]
    pub lookahead_depth: u32,

    /// Enable spatial adaptive quantization
    #[serde(default = "default_true")]
    pub spatial_aq: bool,

    /// Enable temporal AQ (RTX 50 series, ~4-5% efficiency gain)
    #[serde(default)]
    pub temporal_aq: bool,

    /// Multipass encoding: disabled (default), quarter, full
    #[serde(default = "default_multipass")]
    pub multipass: String,

    /// GOP length override (frames, 0 = auto = 2x FPS)
    #[serde(default)]
    pub gop_length: u32,

    /// B-frame reference mode (RTX 50 series)
    #[serde(default)]
    pub b_ref_mode: bool,
}

impl Default for Av1Settings {
    fn default() -> Self {
        Self {
            ten_bit: false,
            tier: default_av1_tier(),
            tune: default_av1_tune(),
            lookahead: false,
            lookahead_depth: 20,
            spatial_aq: true,
            temporal_aq: false,
            multipass: default_multipass(),
            gop_length: 0,
            b_ref_mode: false,
        }
    }
}

fn default_av1_tier() -> String {
    "main".to_string()
}

fn default_av1_tune() -> String {
    "hq".to_string()
}

fn default_lookahead_depth() -> u32 {
    20
}

fn default_multipass() -> String {
    "disabled".to_string()
}

// Default value functions
fn default_preset() -> String {
    "1080p60".to_string()
}

fn default_codec() -> String {
    "h264".to_string()
}

fn default_quality() -> String {
    "medium".to_string()
}

fn default_camera_name() -> String {
    "Nitrogen Camera".to_string()
}

fn default_true() -> bool {
    true
}

fn default_audio_source() -> String {
    "none".to_string()
}

fn default_audio_codec() -> String {
    "aac".to_string()
}

impl Default for DefaultSettings {
    fn default() -> Self {
        Self {
            preset: default_preset(),
            codec: default_codec(),
            bitrate: 0,
            low_latency: true,
        }
    }
}

impl Default for EncoderSettings {
    fn default() -> Self {
        Self {
            quality: default_quality(),
            gpu: 0,
        }
    }
}

impl Default for CameraSettings {
    fn default() -> Self {
        Self {
            name: default_camera_name(),
        }
    }
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            source: default_audio_source(),
            codec: default_audio_codec(),
            bitrate: 0,
        }
    }
}

impl ConfigFile {
    /// Get the default config file path
    pub fn default_path() -> PathBuf {
        if let Some(config_dir) = dirs::config_dir() {
            config_dir.join("nitrogen").join("config.toml")
        } else if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home)
                .join(".config")
                .join("nitrogen")
                .join("config.toml")
        } else {
            PathBuf::from("/etc/nitrogen/config.toml")
        }
    }

    /// Load configuration from the default path
    pub fn load() -> Result<Self> {
        Self::load_from(Self::default_path())
    }

    /// Load configuration from a specific path
    pub fn load_from(path: PathBuf) -> Result<Self> {
        if !path.exists() {
            debug!("Config file not found at {:?}, using defaults", path);
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)
            .map_err(|e| NitrogenError::Config(format!("Failed to read config file: {}", e)))?;

        let config: ConfigFile = toml::from_str(&content)
            .map_err(|e| NitrogenError::Config(format!("Failed to parse config file: {}", e)))?;

        info!("Loaded configuration from {:?}", path);
        Ok(config)
    }

    /// Load configuration, logging warnings but returning defaults on error
    pub fn load_or_default() -> Self {
        match Self::load() {
            Ok(config) => config,
            Err(e) => {
                warn!("Failed to load config file: {}, using defaults", e);
                Self::default()
            }
        }
    }

    /// Save configuration to the default path
    pub fn save(&self) -> Result<()> {
        self.save_to(Self::default_path())
    }

    /// Save configuration to a specific path
    pub fn save_to(&self, path: PathBuf) -> Result<()> {
        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| {
                    NitrogenError::Config(format!("Failed to create config directory: {}", e))
                })?;
            }
        }

        let content = toml::to_string_pretty(self)
            .map_err(|e| NitrogenError::Config(format!("Failed to serialize config: {}", e)))?;

        std::fs::write(&path, content)
            .map_err(|e| NitrogenError::Config(format!("Failed to write config file: {}", e)))?;

        info!("Saved configuration to {:?}", path);
        Ok(())
    }

    /// Create a default config file if it doesn't exist
    pub fn create_default_if_missing() -> Result<bool> {
        let path = Self::default_path();
        if path.exists() {
            return Ok(false);
        }

        let config = Self::default();
        config.save_to(path)?;
        Ok(true)
    }
}

/// Generate a sample configuration file
pub fn sample_config() -> String {
    r#"# Nitrogen Configuration
# https://github.com/ghostkellz/nitrogen

[defaults]
# Output preset: 720p30, 720p60, 1080p30, 1080p60, 1440p30, 1440p60, 1440p120, 4k30, 4k60, 4k120
preset = "1080p60"

# Video codec: h264, hevc, av1
codec = "h264"

# Bitrate in kbps (0 = automatic based on preset)
bitrate = 0

# Enable low-latency mode (recommended for Discord)
low_latency = true

[encoder]
# Quality preset: fast, medium, slow, quality
quality = "medium"

# GPU index (0 = first NVIDIA GPU)
gpu = 0

[av1]
# AV1-specific settings (only used when codec = "av1")
# Supports RTX 40 series (Ada) and RTX 50 series (Blackwell) features

# Enable 10-bit encoding (main10 profile) for better color depth
ten_bit = false

# AV1 tier: "main" (default, broad compatibility) or "high" (RTX 40+ for higher bitrates)
tier = "main"

# Tuning mode:
#   "hq"  - High quality (default, good balance)
#   "uhq" - Ultra High Quality (RTX 50 only, ~5% better compression)
#   "ll"  - Low latency (for streaming)
#   "ull" - Ultra low latency
tune = "hq"

# Enable lookahead for better quality (ignored in low-latency mode)
lookahead = false

# Lookahead depth in frames (RTX 40: up to 32, RTX 50: up to 250)
lookahead_depth = 20

# Enable spatial adaptive quantization (better quality at same bitrate)
spatial_aq = true

# Enable temporal AQ (RTX 50 series only, ~4-5% efficiency improvement)
temporal_aq = false

# Multipass encoding: "disabled" (default), "quarter", "full" (best quality)
multipass = "disabled"

# GOP (keyframe interval) override in frames (0 = auto, defaults to 2x FPS)
gop_length = 0

# B-frame reference mode for better compression (RTX 50 series)
b_ref_mode = false

[camera]
# Virtual camera name shown in applications
name = "Nitrogen Camera"

[audio]
# Audio source: none, desktop, mic, both
source = "none"

# Audio codec: aac, opus
codec = "aac"

# Audio bitrate in kbps (0 = automatic based on codec)
bitrate = 0
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ConfigFile::default();
        assert_eq!(config.defaults.preset, "1080p60");
        assert_eq!(config.defaults.codec, "h264");
        assert_eq!(config.encoder.quality, "medium");
    }

    #[test]
    fn test_sample_config_parses() {
        let sample = sample_config();
        let config: ConfigFile = toml::from_str(&sample).unwrap();
        assert_eq!(config.defaults.preset, "1080p60");
    }
}

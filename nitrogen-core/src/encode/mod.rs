//! Video and audio encoding via FFmpeg/NVENC
//!
//! This module provides:
//! - Hardware-accelerated video encoding using NVIDIA's NVENC
//! - Audio encoding (AAC, Opus) for recording
//! - Smooth Motion frame interpolation for streaming

mod audio;
mod frame_gen;
mod nvenc;
mod scaler;

pub use audio::{
    audio_codec_available, list_available_audio_encoders, AudioEncoder, AudioEncoderInfo,
    EncodedAudioPacket,
};
pub use frame_gen::{FrameGenMode, SmoothMotion, SmoothMotionConfig, supports_smooth_motion};
pub use nvenc::{EncodedPacket, NvencEncoder};
pub use scaler::FrameScaler;

use crate::config::Codec;

/// Check if NVENC is available on this system
pub fn nvenc_available() -> bool {
    nvenc::check_nvenc_available()
}

/// Check if a specific codec is available
pub fn codec_available(codec: Codec) -> bool {
    nvenc::encoder_available(codec)
}

/// Get list of available NVENC encoders
pub fn available_encoders() -> Vec<String> {
    nvenc::list_available_encoders()
        .into_iter()
        .map(|(_, name)| name.to_string())
        .collect()
}

/// GPU information
#[derive(Debug, Clone)]
pub struct GpuInfo {
    /// GPU model name
    pub name: String,
    /// Total VRAM in MB
    pub vram_mb: u64,
    /// Driver version
    pub driver_version: String,
}

/// Get GPU information using nvidia-smi
pub fn get_gpu_info() -> Option<GpuInfo> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,memory.total,driver_version",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split(", ").collect();
    if parts.len() >= 3 {
        Some(GpuInfo {
            name: parts[0].to_string(),
            vram_mb: parts[1].trim().parse().unwrap_or(0),
            driver_version: parts[2].to_string(),
        })
    } else {
        None
    }
}

/// Encoder capabilities for a specific codec
#[derive(Debug, Clone)]
pub struct EncoderCapabilities {
    /// Supports B-frames
    pub b_frames: bool,
    /// Supports 10-bit encoding
    pub bit_10: bool,
    /// Supports lookahead
    pub lookahead: bool,
    /// Maximum width
    pub max_width: u32,
    /// Maximum height
    pub max_height: u32,
}

/// Get encoder capabilities for a codec (estimates based on NVENC generation)
pub fn get_encoder_capabilities(codec: Codec) -> Option<EncoderCapabilities> {
    if !codec_available(codec) {
        return None;
    }

    // These are estimates based on typical NVENC capabilities
    Some(EncoderCapabilities {
        b_frames: codec != Codec::Av1, // AV1 NVENC doesn't support B-frames
        bit_10: true,                  // Most modern NVENC supports 10-bit
        lookahead: true,
        max_width: 8192,
        max_height: 8192,
    })
}

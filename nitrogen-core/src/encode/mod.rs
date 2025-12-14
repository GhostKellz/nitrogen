//! Video encoding via FFmpeg/NVENC
//!
//! This module provides hardware-accelerated video encoding using
//! NVIDIA's NVENC encoder through FFmpeg.

mod nvenc;
mod scaler;

pub use nvenc::NvencEncoder;
pub use scaler::FrameScaler;

use crate::config::{Codec, EncoderPreset};
use crate::error::Result;

/// Check if NVENC is available on this system
pub fn nvenc_available() -> bool {
    nvenc::check_nvenc_available()
}

/// Get list of available NVENC encoders
pub fn available_encoders() -> Vec<String> {
    let mut encoders = Vec::new();

    if nvenc::encoder_available(Codec::H264) {
        encoders.push("h264_nvenc".to_string());
    }
    if nvenc::encoder_available(Codec::Hevc) {
        encoders.push("hevc_nvenc".to_string());
    }
    if nvenc::encoder_available(Codec::Av1) {
        encoders.push("av1_nvenc".to_string());
    }

    encoders
}

//! Nitrogen Core Library
//!
//! Wayland-native NVIDIA streaming for Discord and friends.
//!
//! This library provides:
//! - Wayland screencast capture via xdg-desktop-portal
//! - NVENC-accelerated video encoding (H.264, HEVC, AV1)
//! - PipeWire virtual camera output
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐    ┌──────────────┐    ┌─────────────────┐
//! │ Portal Capture  │───▶│ NVENC Encode │───▶│ Virtual Camera  │
//! │ (PipeWire In)   │    │ (FFmpeg)     │    │ (PipeWire Out)  │
//! └─────────────────┘    └──────────────┘    └─────────────────┘
//! ```

pub mod capture;
pub mod config;
pub mod encode;
pub mod error;
pub mod output;
pub mod pipeline;
pub mod types;

pub use config::{CaptureConfig, Codec, Preset};
pub use error::{NitrogenError, Result};
pub use pipeline::Pipeline;
pub use types::{CaptureSource, Handle, SourceInfo, SourceKind};

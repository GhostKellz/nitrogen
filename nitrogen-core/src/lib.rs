//! Nitrogen Core Library
//!
//! Wayland-native NVIDIA streaming for Discord and friends.
//!
//! This library provides:
//! - Wayland screencast capture via xdg-desktop-portal
//! - NVENC-accelerated video encoding (H.264, HEVC, AV1)
//! - PipeWire virtual camera output
//! - Optional file recording (MP4, MKV)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐    ┌──────────────┐    ┌─────────────────┐
//! │ Portal Capture  │───▶│ NVENC Encode │───▶│ Virtual Camera  │
//! │ (PipeWire In)   │    │ (FFmpeg)     │    │ (PipeWire Out)  │
//! └─────────────────┘    └──────────────┘    │ + File Output   │
//!                                            └─────────────────┘
//! ```

pub mod capture;
pub mod config;
pub mod encode;
pub mod env;
pub mod error;
pub mod formats;
pub mod gpu;
pub mod hotkeys;
pub mod ipc;
pub mod output;
pub mod overlay;
pub mod performance;
pub mod pipeline;
pub mod types;

pub use config::{
    AudioCodec, AudioSource, Av1Config, Av1Tier, Av1Tune, CaptureConfig, ChromaFormat, Codec,
    EncoderPreset, MultipassMode, Preset,
};
pub use encode::{TonemapAlgorithm, TonemapConfig, TonemapMode, Tonemapper};
pub use env::{
    EnvironmentOptimizations, GamescopeInfo, RuntimeEnvironment, WaylandInfo, detect_environment,
    is_steam_deck_hardware,
};
pub use error::{NitrogenError, Result};
pub use gpu::{
    GpuGeneration, RecommendedAv1Settings, Rtx50Features, detect_rtx50_features, get_gpu_generation,
};
pub use hotkeys::{Hotkey, HotkeyAction, HotkeyListener};
pub use ipc::{IpcClient, IpcServer, daemon_running, socket_path};
pub use output::{
    FileRecorder, StreamConfig, StreamOutput, StreamProtocol, WebRTCConfig, WebRTCOutput,
    start_signaling_server, stream_av_from_channels, stream_from_channel,
};
pub use overlay::{LatencyOverlay, OverlayConfig, OverlayPosition};
pub use performance::{
    GpuStats, LatencyStats, PerformanceMetrics, create_metrics, query_gpu_stats,
};
pub use pipeline::{Pipeline, PipelineState, PipelineStats};
pub use types::{
    AudioFormat, AudioFrame, AudioSampleFormat, CaptureSource, ColorPrimaries, Handle, HdrMetadata,
    SourceInfo, SourceKind, TransferFunction,
};

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

pub use config::{AudioCodec, AudioSource, Av1Config, Av1Tier, Av1Tune, CaptureConfig, ChromaFormat, Codec, EncoderPreset, MultipassMode, Preset};
pub use encode::{TonemapAlgorithm, TonemapConfig, TonemapMode, Tonemapper};
pub use env::{detect_environment, is_steam_deck_hardware, RuntimeEnvironment, GamescopeInfo, WaylandInfo, EnvironmentOptimizations};
pub use error::{NitrogenError, Result};
pub use performance::{create_metrics, query_gpu_stats, GpuStats, LatencyStats, PerformanceMetrics};
pub use overlay::{LatencyOverlay, OverlayConfig, OverlayPosition};
pub use gpu::{detect_rtx50_features, get_gpu_generation, GpuGeneration, RecommendedAv1Settings, Rtx50Features};
pub use hotkeys::{Hotkey, HotkeyAction, HotkeyListener};
pub use ipc::{daemon_running, socket_path, IpcClient, IpcServer};
pub use output::{
    FileRecorder, StreamConfig, StreamOutput, StreamProtocol,
    WebRTCConfig, WebRTCOutput, start_signaling_server,
    stream_av_from_channels, stream_from_channel,
};
pub use pipeline::{Pipeline, PipelineState, PipelineStats};
pub use types::{
    AudioFormat, AudioFrame, AudioSampleFormat, CaptureSource, ColorPrimaries, Handle, HdrMetadata,
    SourceInfo, SourceKind, TransferFunction,
};

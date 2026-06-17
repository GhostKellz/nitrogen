//! Video and audio output modules
//!
//! Supports multiple output targets:
//! - Virtual camera (via PipeWire/ghoststream)
//! - Virtual microphone (for Discord audio passthrough)
//! - File recording (MP4, MKV)
//! - WebRTC streaming (browser-based viewing)
//! - RTMP/SRT streaming (Twitch, YouTube, etc.)

mod file;
mod stream;
mod virtual_audio;
mod webrtc;

// Re-export ghoststream's virtual camera and traits
pub use file::{FileRecorder, record_av_from_channels, record_from_channel};
pub use ghoststream::output::{RawOutputSink, VirtualCamera};
pub use stream::{
    StreamConfig, StreamOutput, StreamProtocol, stream_av_from_channels, stream_from_channel,
};
pub use virtual_audio::{DEFAULT_VIRTUAL_MIC_NAME, VirtualMicrophone};
pub use webrtc::{WebRTCConfig, WebRTCOutput, start_signaling_server};

/// Default camera name
pub const DEFAULT_CAMERA_NAME: &str = "Nitrogen Camera";

/// Create a virtual camera with nitrogen defaults
pub fn create_camera(name: Option<&str>) -> VirtualCamera {
    let camera_name = name.unwrap_or(DEFAULT_CAMERA_NAME);
    VirtualCamera::new(camera_name)
}

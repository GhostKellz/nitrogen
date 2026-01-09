//! Video and audio output modules
//!
//! Supports multiple output targets:
//! - Virtual camera (via PipeWire/ghoststream)
//! - Virtual microphone (for Discord audio passthrough)
//! - File recording (MP4, MKV)

mod file;
mod virtual_audio;

// Re-export ghoststream's virtual camera and traits
pub use file::{record_av_from_channels, record_from_channel, FileRecorder};
pub use ghoststream::output::{RawOutputSink, VirtualCamera};
pub use virtual_audio::{VirtualMicrophone, DEFAULT_VIRTUAL_MIC_NAME};

/// Default camera name
pub const DEFAULT_CAMERA_NAME: &str = "Nitrogen Camera";

/// Create a virtual camera with nitrogen defaults
pub fn create_camera(name: Option<&str>) -> VirtualCamera {
    let camera_name = name.unwrap_or(DEFAULT_CAMERA_NAME);
    VirtualCamera::new(camera_name)
}

//! Virtual camera output via PipeWire
//!
//! Creates a PipeWire video source node that appears as a camera
//! in Discord, OBS, and other applications.

mod camera;

pub use camera::VirtualCamera;

use crate::error::Result;

/// Default camera name
pub const DEFAULT_CAMERA_NAME: &str = "Nitrogen Camera";

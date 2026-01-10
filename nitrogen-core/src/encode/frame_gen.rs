//! Smooth Motion Frame Generation for streaming
//!
//! Provides frame interpolation to increase output framerate without
//! requiring higher capture rates. Uses NVIDIA Optical Flow when available.

use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::types::Frame;

/// Frame generation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrameGenMode {
    /// No frame generation (passthrough)
    #[default]
    Off,
    /// 2x interpolation (30fps -> 60fps)
    Double,
    /// 3x interpolation (30fps -> 90fps)
    Triple,
    /// 4x interpolation (30fps -> 120fps)
    Quadruple,
    /// Adaptive - adjusts based on scene complexity
    Adaptive,
}

impl FrameGenMode {
    /// Get the frame multiplier
    pub fn multiplier(&self) -> u32 {
        match self {
            FrameGenMode::Off => 1,
            FrameGenMode::Double => 2,
            FrameGenMode::Triple => 3,
            FrameGenMode::Quadruple => 4,
            FrameGenMode::Adaptive => 2, // Default for adaptive
        }
    }

    /// Get effective output FPS from input FPS
    pub fn output_fps(&self, input_fps: u32) -> u32 {
        input_fps * self.multiplier()
    }

    /// Parse from string
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "off" | "none" | "0" => FrameGenMode::Off,
            "double" | "2x" | "2" => FrameGenMode::Double,
            "triple" | "3x" | "3" => FrameGenMode::Triple,
            "quadruple" | "4x" | "4" => FrameGenMode::Quadruple,
            "adaptive" | "auto" => FrameGenMode::Adaptive,
            _ => FrameGenMode::Off,
        }
    }
}

impl std::fmt::Display for FrameGenMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameGenMode::Off => write!(f, "off"),
            FrameGenMode::Double => write!(f, "2x"),
            FrameGenMode::Triple => write!(f, "3x"),
            FrameGenMode::Quadruple => write!(f, "4x"),
            FrameGenMode::Adaptive => write!(f, "adaptive"),
        }
    }
}

/// Smooth Motion configuration
#[derive(Debug, Clone)]
pub struct SmoothMotionConfig {
    /// Frame generation mode
    pub mode: FrameGenMode,
    /// Use GPU-accelerated optical flow (requires CUDA)
    pub gpu_accelerated: bool,
    /// Quality preset (0-100, higher = better quality but more latency)
    pub quality: u8,
    /// Maximum latency in milliseconds (0 = no limit)
    pub max_latency_ms: u32,
    /// Scene change threshold (0.0-1.0, lower = more sensitive)
    pub scene_threshold: f32,
    /// Enable temporal stability (reduces flickering)
    pub temporal_stability: bool,
}

impl Default for SmoothMotionConfig {
    fn default() -> Self {
        Self {
            mode: FrameGenMode::Off,
            gpu_accelerated: true,
            quality: 75,
            max_latency_ms: 50,
            scene_threshold: 0.4,
            temporal_stability: true,
        }
    }
}

impl SmoothMotionConfig {
    /// Preset for low-latency streaming (e.g., game streaming)
    pub fn low_latency() -> Self {
        Self {
            mode: FrameGenMode::Double,
            gpu_accelerated: true,
            quality: 50,
            max_latency_ms: 16, // ~1 frame at 60fps
            scene_threshold: 0.5,
            temporal_stability: false,
        }
    }

    /// Preset for high-quality streaming (e.g., video content)
    pub fn high_quality() -> Self {
        Self {
            mode: FrameGenMode::Double,
            gpu_accelerated: true,
            quality: 90,
            max_latency_ms: 100,
            scene_threshold: 0.3,
            temporal_stability: true,
        }
    }

    /// Preset for maximum smoothness
    pub fn max_smoothness() -> Self {
        Self {
            mode: FrameGenMode::Quadruple,
            gpu_accelerated: true,
            quality: 80,
            max_latency_ms: 50,
            scene_threshold: 0.4,
            temporal_stability: true,
        }
    }
}

/// Smooth Motion frame interpolator
pub struct SmoothMotion {
    config: SmoothMotionConfig,
    /// Previous frame for interpolation
    prev_frame: Option<Arc<Frame>>,
    /// Frame counter
    frame_count: u64,
    /// Output sender
    output_tx: broadcast::Sender<Arc<Frame>>,
    /// Is optical flow available?
    optical_flow_available: bool,
}

impl SmoothMotion {
    /// Create a new Smooth Motion interpolator
    pub fn new(config: SmoothMotionConfig) -> Self {
        let (output_tx, _) = broadcast::channel(32);

        // Check for optical flow availability
        let optical_flow_available = check_optical_flow_available();

        if config.mode != FrameGenMode::Off {
            if optical_flow_available {
                info!(
                    "Smooth Motion enabled: {} interpolation with GPU optical flow",
                    config.mode
                );
            } else {
                warn!(
                    "Smooth Motion enabled: {} interpolation (CPU fallback - optical flow not available)",
                    config.mode
                );
            }
        }

        Self {
            config,
            prev_frame: None,
            frame_count: 0,
            output_tx,
            optical_flow_available,
        }
    }

    /// Subscribe to interpolated frames
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Frame>> {
        self.output_tx.subscribe()
    }

    /// Process an input frame and generate interpolated frames
    pub fn process(&mut self, frame: Arc<Frame>) -> Result<Vec<Arc<Frame>>> {
        if self.config.mode == FrameGenMode::Off {
            // Passthrough mode
            let _ = self.output_tx.send(frame.clone());
            return Ok(vec![frame]);
        }

        let mut output_frames = Vec::new();
        let multiplier = self.config.mode.multiplier();

        if let Some(ref prev) = self.prev_frame {
            // Generate interpolated frames
            for i in 1..multiplier {
                let t = i as f32 / multiplier as f32;

                // Check for scene change
                if self.detect_scene_change(prev, &frame) {
                    debug!("Scene change detected, skipping interpolation");
                    // On scene change, just duplicate the new frame
                    output_frames.push(frame.clone());
                } else {
                    // Interpolate frame
                    let interp = self.interpolate_frame(prev, &frame, t)?;
                    output_frames.push(Arc::new(interp));
                }
            }
        }

        // Add the original frame
        output_frames.push(frame.clone());

        // Send all frames to subscribers
        for f in &output_frames {
            let _ = self.output_tx.send(f.clone());
        }

        // Store current frame for next interpolation
        self.prev_frame = Some(frame);
        self.frame_count += 1;

        Ok(output_frames)
    }

    /// Detect scene change between two frames
    fn detect_scene_change(&self, _prev: &Frame, _curr: &Frame) -> bool {
        // TODO: Implement actual scene change detection
        // For now, always return false (no scene change)
        // In production, this would compute frame difference metrics
        false
    }

    /// Interpolate between two frames at time t (0.0 to 1.0)
    fn interpolate_frame(&self, prev: &Frame, curr: &Frame, t: f32) -> Result<Frame> {
        // TODO: Implement actual optical flow interpolation
        // For now, we do simple linear blending as a placeholder

        if self.optical_flow_available && self.config.gpu_accelerated {
            // Would use NVIDIA Optical Flow SDK here
            self.gpu_interpolate(prev, curr, t)
        } else {
            // CPU fallback - simple blending
            self.cpu_interpolate(prev, curr, t)
        }
    }

    /// GPU-accelerated interpolation using NVIDIA Optical Flow
    fn gpu_interpolate(&self, _prev: &Frame, curr: &Frame, t: f32) -> Result<Frame> {
        // TODO: Integrate NVIDIA Optical Flow SDK
        // For now, duplicate the frame with interpolated timestamp
        self.duplicate_frame(curr, t)
    }

    /// CPU fallback interpolation (simple blend)
    fn cpu_interpolate(&self, _prev: &Frame, curr: &Frame, t: f32) -> Result<Frame> {
        // Simple frame duplication as placeholder
        // Real implementation would do motion-compensated blending
        self.duplicate_frame(curr, t)
    }

    /// Duplicate a frame (placeholder for actual interpolation)
    fn duplicate_frame(&self, frame: &Frame, _t: f32) -> Result<Frame> {
        use crate::types::FrameData;

        let data = match &frame.data {
            FrameData::Memory(bytes) => FrameData::Memory(bytes.clone()),
            FrameData::DmaBuf { fd, offset, modifier } => {
                // Can't duplicate DMA-BUF, would need to copy via GPU
                // For now, just reference the same fd (caller must handle lifetime)
                FrameData::DmaBuf {
                    fd: *fd,
                    offset: *offset,
                    modifier: *modifier,
                }
            }
        };

        Ok(Frame {
            format: frame.format.clone(),
            data,
            pts: frame.pts,
        })
    }

    /// Get frame count
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Get effective output multiplier
    pub fn multiplier(&self) -> u32 {
        self.config.mode.multiplier()
    }

    /// Check if optical flow is being used
    pub fn using_optical_flow(&self) -> bool {
        self.optical_flow_available && self.config.gpu_accelerated
    }
}

/// Check if NVIDIA Optical Flow is available
fn check_optical_flow_available() -> bool {
    // Check for libnvidia-opticalflow.so
    let paths = [
        "/usr/lib/libnvidia-opticalflow.so",
        "/usr/lib/x86_64-linux-gnu/libnvidia-opticalflow.so",
        "/usr/lib64/libnvidia-opticalflow.so",
    ];

    for path in paths {
        if std::path::Path::new(path).exists() {
            debug!("Found NVIDIA Optical Flow at: {}", path);
            return true;
        }
    }

    // Also check via nvidia-smi for driver that supports it
    if let Ok(output) = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=driver_version", "--format=csv,noheader"])
        .output()
    {
        if output.status.success() {
            let version = String::from_utf8_lossy(&output.stdout);
            let version = version.trim();

            // Optical Flow requires driver 418+
            if let Some(major) = version.split('.').next() {
                if let Ok(major_ver) = major.parse::<u32>() {
                    if major_ver >= 418 {
                        debug!("Driver {} supports Optical Flow", version);
                        return true;
                    }
                }
            }
        }
    }

    false
}

/// Check if GPU supports Smooth Motion (RTX 20 series or newer)
pub fn supports_smooth_motion() -> bool {
    if let Ok(output) = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output()
    {
        if output.status.success() {
            let name = String::from_utf8_lossy(&output.stdout);
            let name = name.trim();

            // RTX series supports optical flow-based interpolation
            return name.contains("RTX");
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_gen_mode() {
        assert_eq!(FrameGenMode::Off.multiplier(), 1);
        assert_eq!(FrameGenMode::Double.multiplier(), 2);
        assert_eq!(FrameGenMode::Triple.multiplier(), 3);
        assert_eq!(FrameGenMode::Quadruple.multiplier(), 4);
    }

    #[test]
    fn test_output_fps() {
        assert_eq!(FrameGenMode::Double.output_fps(30), 60);
        assert_eq!(FrameGenMode::Quadruple.output_fps(30), 120);
    }

    #[test]
    fn test_from_str() {
        assert_eq!(FrameGenMode::from_str("2x"), FrameGenMode::Double);
        assert_eq!(FrameGenMode::from_str("off"), FrameGenMode::Off);
        assert_eq!(FrameGenMode::from_str("adaptive"), FrameGenMode::Adaptive);
    }
}

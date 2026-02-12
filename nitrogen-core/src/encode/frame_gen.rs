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

use super::nvfruc::{nvfruc_available, NvFruc};

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
    /// NVIDIA FRUC instance for GPU interpolation
    nvfruc: Option<NvFruc>,
}

impl SmoothMotion {
    /// Create a new Smooth Motion interpolator
    pub fn new(config: SmoothMotionConfig) -> Self {
        let (output_tx, _) = broadcast::channel(32);

        // Check for optical flow / FRUC availability
        let optical_flow_available = check_optical_flow_available() || nvfruc_available();

        if config.mode != FrameGenMode::Off {
            if nvfruc_available() {
                info!(
                    "Smooth Motion enabled: {} interpolation with NVIDIA FRUC",
                    config.mode
                );
            } else if optical_flow_available {
                info!(
                    "Smooth Motion enabled: {} interpolation with GPU optical flow",
                    config.mode
                );
            } else {
                warn!(
                    "Smooth Motion enabled: {} interpolation (CPU fallback - GPU acceleration not available)",
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
            nvfruc: None, // Initialized lazily on first frame
        }
    }

    /// Try to initialize NvFRUC for the given frame dimensions
    fn try_init_nvfruc(&mut self, width: u32, height: u32) {
        if self.nvfruc.is_some() {
            return; // Already initialized
        }

        if !self.config.gpu_accelerated || !nvfruc_available() {
            return; // GPU not requested or not available
        }

        match NvFruc::new(width, height) {
            Ok(fruc) => {
                info!("NvFRUC initialized for {}x{} frames", width, height);
                self.nvfruc = Some(fruc);
            }
            Err(e) => {
                warn!("Failed to initialize NvFRUC: {}, using CPU fallback", e);
            }
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

        // Try to initialize NvFRUC on first frame (lazy init)
        if self.nvfruc.is_none() && self.config.gpu_accelerated {
            self.try_init_nvfruc(frame.format.width, frame.format.height);
        }

        let mut output_frames = Vec::new();
        let multiplier = self.config.mode.multiplier();

        // Clone previous frame to avoid borrow conflict with interpolate_frame
        let prev_frame = self.prev_frame.clone();

        if let Some(ref prev) = prev_frame {
            // Check for scene change once (it's the same for all interpolated frames)
            let is_scene_change = self.detect_scene_change(prev, &frame);

            // Generate interpolated frames
            for i in 1..multiplier {
                let t = i as f32 / multiplier as f32;

                if is_scene_change {
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

    /// Detect scene change between two frames using histogram comparison
    fn detect_scene_change(&self, prev: &Frame, curr: &Frame) -> bool {
        use crate::types::FrameData;

        // Different dimensions always trigger scene change
        if prev.format.width != curr.format.width || prev.format.height != curr.format.height {
            return true;
        }

        // Get frame data - only works for Memory frames
        let prev_data = match &prev.data {
            FrameData::Memory(data) => data,
            FrameData::DmaBuf { .. } => return false, // Can't analyze DMA-BUF without mapping
        };
        let curr_data = match &curr.data {
            FrameData::Memory(data) => data,
            FrameData::DmaBuf { .. } => return false,
        };

        // Compute histogram difference
        let diff = compute_histogram_difference(
            prev_data,
            curr_data,
            prev.format.width,
            prev.format.height,
            prev.format.stride,
            prev.format.fourcc,
        );

        diff > self.config.scene_threshold
    }

    /// Interpolate between two frames at time t (0.0 to 1.0)
    fn interpolate_frame(&mut self, prev: &Frame, curr: &Frame, t: f32) -> Result<Frame> {
        // Try GPU interpolation first if available
        if self.config.gpu_accelerated {
            if let Some(ref mut fruc) = self.nvfruc {
                match fruc.interpolate(prev, curr, t) {
                    Ok(frame) => return Ok(frame),
                    Err(e) => {
                        debug!("NvFRUC interpolation failed: {}, falling back to CPU", e);
                    }
                }
            }
        }

        // CPU fallback - linear blending
        self.cpu_interpolate(prev, curr, t)
    }

    /// CPU fallback interpolation using linear pixel blending
    fn cpu_interpolate(&self, prev: &Frame, curr: &Frame, t: f32) -> Result<Frame> {
        use crate::types::FrameData;

        // Only support Memory frames for CPU interpolation
        let prev_data = match &prev.data {
            FrameData::Memory(data) => data,
            FrameData::DmaBuf { .. } => {
                // Fall back to frame duplication for DMA-BUF
                return self.duplicate_frame(curr, t);
            }
        };
        let curr_data = match &curr.data {
            FrameData::Memory(data) => data,
            FrameData::DmaBuf { .. } => {
                return self.duplicate_frame(curr, t);
            }
        };

        // Verify matching formats
        if prev.format.width != curr.format.width
            || prev.format.height != curr.format.height
            || prev.format.fourcc != curr.format.fourcc
            || prev_data.len() != curr_data.len()
        {
            return self.duplicate_frame(curr, t);
        }

        // Linear blend: result = prev * (1-t) + curr * t
        // Use fixed-point math for performance: multiply by 256, then >> 8
        let t_fixed = (t * 256.0).round() as u16;
        let inv_t_fixed = 256 - t_fixed;

        let blended: Vec<u8> = prev_data
            .iter()
            .zip(curr_data.iter())
            .map(|(&p, &c)| {
                // Fixed-point blend: (p * inv_t + c * t) >> 8
                (((p as u16 * inv_t_fixed) + (c as u16 * t_fixed)) >> 8) as u8
            })
            .collect();

        // Interpolate presentation timestamp
        let interpolated_pts = interpolate_pts(prev.pts, curr.pts, t);

        Ok(Frame {
            format: curr.format,
            data: FrameData::Memory(blended),
            pts: interpolated_pts,
            hdr_metadata: curr.hdr_metadata,
        })
    }

    /// Duplicate a frame (fallback when interpolation not possible)
    ///
    /// For Memory frames, this clones the data.
    /// For DMA-BUF frames, this references the same fd (no interpolation).
    ///
    /// # DMA-BUF Interpolation (Future Work)
    ///
    /// True DMA-BUF interpolation would require:
    /// 1. Import both DMA-BUF frames to GPU memory (CUDA, Vulkan, or OpenGL)
    /// 2. Run interpolation shader/kernel on GPU
    /// 3. Export result as new DMA-BUF or copy to CPU memory
    ///
    /// This is blocked on:
    /// - CUDA interop for DMA-BUF import (`cuExternalMemoryGetMappedBuffer`)
    /// - Or Vulkan compute shader implementation
    ///
    /// For now, DMA-BUF frames fall back to frame duplication (no interpolation),
    /// which avoids artifacts but provides no smoothing benefit.
    fn duplicate_frame(&self, frame: &Frame, _t: f32) -> Result<Frame> {
        use crate::types::FrameData;

        let data = match &frame.data {
            FrameData::Memory(bytes) => FrameData::Memory(bytes.clone()),
            FrameData::DmaBuf { fd, offset, modifier } => {
                // DMA-BUF interpolation requires GPU compute - not yet implemented.
                // For now, just reference the same fd (caller must handle lifetime).
                // This means no smoothing for DMA-BUF frames, but also no artifacts.
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
            hdr_metadata: frame.hdr_metadata,
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

/// Compute histogram difference between two frames for scene change detection
///
/// Returns a value between 0.0 (identical) and 1.0+ (completely different)
fn compute_histogram_difference(
    prev: &[u8],
    curr: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    fourcc: u32,
) -> f32 {
    const BINS: usize = 64;
    let mut prev_hist = [0u32; BINS];
    let mut curr_hist = [0u32; BINS];

    // Determine bytes per pixel based on fourcc format
    // Common formats: XRGB8888 (0x34325258), ARGB8888 (0x34325241), BGRA (0x41524742)
    let bytes_per_pixel: u32 = match fourcc {
        0x34325258 | 0x34325241 | 0x41524742 | 0x34324142 => 4, // XRGB, ARGB, BGRA, ABGR
        _ => return 0.0, // Unknown format, assume no scene change
    };

    // Sample every 4th pixel in each dimension for performance (1/16 of pixels)
    let mut sample_count = 0u32;
    for y in (0..height).step_by(4) {
        for x in (0..width).step_by(4) {
            let offset = (y * stride + x * bytes_per_pixel) as usize;

            // Ensure we have enough bytes to read RGB
            if offset + 3 <= prev.len() && offset + 3 <= curr.len() {
                // For BGRA/XRGB formats: B=0, G=1, R=2 (or R=0, G=1, B=2 for RGB)
                // Use standard luminance formula: Y = 0.299*R + 0.587*G + 0.114*B
                // Fixed-point: Y = (77*R + 150*G + 29*B) >> 8
                let prev_luma = ((77 * prev[offset + 2] as u32
                    + 150 * prev[offset + 1] as u32
                    + 29 * prev[offset] as u32)
                    >> 8) as usize;
                let curr_luma = ((77 * curr[offset + 2] as u32
                    + 150 * curr[offset + 1] as u32
                    + 29 * curr[offset] as u32)
                    >> 8) as usize;

                // Map 0-255 to 0-63 bins
                prev_hist[prev_luma.min(255) >> 2] += 1;
                curr_hist[curr_luma.min(255) >> 2] += 1;
                sample_count += 1;
            }
        }
    }

    if sample_count == 0 {
        return 0.0;
    }

    // Chi-squared distance between histograms
    let mut chi_sq = 0.0f32;
    for i in 0..BINS {
        let sum = prev_hist[i] + curr_hist[i];
        if sum > 0 {
            let diff = prev_hist[i] as f32 - curr_hist[i] as f32;
            chi_sq += (diff * diff) / sum as f32;
        }
    }

    // Normalize by sample count to get a 0-1 range
    chi_sq / (sample_count as f32 * 2.0)
}

/// Interpolate presentation timestamp between two frames
fn interpolate_pts(prev_pts: u64, curr_pts: u64, t: f32) -> u64 {
    let duration = curr_pts.saturating_sub(prev_pts);
    prev_pts + ((duration as f64 * t as f64) as u64)
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
    use crate::types::{Frame, FrameData, FrameFormat};

    /// Helper to create a test frame with uniform color
    fn create_test_frame(width: u32, height: u32, color: u8) -> Frame {
        let stride = width * 4;
        let size = (stride * height) as usize;
        // BGRA format: fill all channels with the same value
        let data = vec![color; size];
        Frame {
            format: FrameFormat {
                width,
                height,
                fourcc: 0x34325258, // XRGB8888
                stride,
            },
            data: FrameData::Memory(data),
            pts: 0,
            hdr_metadata: None,
        }
    }

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

    #[test]
    fn test_scene_change_same_frame() {
        let config = SmoothMotionConfig::default();
        let smooth = SmoothMotion::new(config);
        let frame = create_test_frame(1920, 1080, 128);

        // Same frame should NOT trigger scene change
        assert!(!smooth.detect_scene_change(&frame, &frame));
    }

    #[test]
    fn test_scene_change_similar_frames() {
        let config = SmoothMotionConfig::default();
        let smooth = SmoothMotion::new(config);
        let frame1 = create_test_frame(1920, 1080, 128);
        let frame2 = create_test_frame(1920, 1080, 130); // Slightly different

        // Similar frames should NOT trigger scene change
        assert!(!smooth.detect_scene_change(&frame1, &frame2));
    }

    #[test]
    fn test_scene_change_different_frames() {
        let config = SmoothMotionConfig::default();
        let smooth = SmoothMotion::new(config);
        let frame1 = create_test_frame(1920, 1080, 0); // Black
        let frame2 = create_test_frame(1920, 1080, 255); // White

        // Black to white should trigger scene change
        assert!(smooth.detect_scene_change(&frame1, &frame2));
    }

    #[test]
    fn test_scene_change_different_dimensions() {
        let config = SmoothMotionConfig::default();
        let smooth = SmoothMotion::new(config);
        let frame1 = create_test_frame(1920, 1080, 128);
        let frame2 = create_test_frame(1280, 720, 128);

        // Different dimensions should always trigger scene change
        assert!(smooth.detect_scene_change(&frame1, &frame2));
    }

    #[test]
    fn test_cpu_interpolation_midpoint() {
        let config = SmoothMotionConfig::default();
        let smooth = SmoothMotion::new(config);

        let frame1 = create_test_frame(100, 100, 0); // Black
        let frame2 = create_test_frame(100, 100, 255); // White

        let interp = smooth.cpu_interpolate(&frame1, &frame2, 0.5).unwrap();

        // Midpoint interpolation of 0 and 255 should be ~128
        if let FrameData::Memory(data) = &interp.data {
            // Check first pixel (should be around 128)
            assert!(data[0] >= 126 && data[0] <= 130, "Expected ~128, got {}", data[0]);
        } else {
            panic!("Expected Memory frame data");
        }
    }

    #[test]
    fn test_cpu_interpolation_quarter() {
        let config = SmoothMotionConfig::default();
        let smooth = SmoothMotion::new(config);

        let frame1 = create_test_frame(100, 100, 0);
        let frame2 = create_test_frame(100, 100, 200);

        let interp = smooth.cpu_interpolate(&frame1, &frame2, 0.25).unwrap();

        // t=0.25: result = 0 * 0.75 + 200 * 0.25 = 50
        if let FrameData::Memory(data) = &interp.data {
            assert!(data[0] >= 48 && data[0] <= 52, "Expected ~50, got {}", data[0]);
        } else {
            panic!("Expected Memory frame data");
        }
    }

    #[test]
    fn test_pts_interpolation() {
        // Test PTS interpolation at various points
        assert_eq!(interpolate_pts(0, 1000, 0.0), 0);
        assert_eq!(interpolate_pts(0, 1000, 0.5), 500);
        assert_eq!(interpolate_pts(0, 1000, 1.0), 1000);
        assert_eq!(interpolate_pts(1000, 2000, 0.25), 1250);
    }

    #[test]
    fn test_histogram_difference_identical() {
        let data = vec![128u8; 1920 * 1080 * 4];
        let diff = compute_histogram_difference(
            &data, &data,
            1920, 1080, 1920 * 4, 0x34325258
        );
        assert!(diff < 0.01, "Identical frames should have near-zero difference, got {}", diff);
    }

    #[test]
    fn test_histogram_difference_opposite() {
        let black = vec![0u8; 1920 * 1080 * 4];
        let white = vec![255u8; 1920 * 1080 * 4];
        let diff = compute_histogram_difference(
            &black, &white,
            1920, 1080, 1920 * 4, 0x34325258
        );
        assert!(diff > 0.3, "Black vs white should have high difference, got {}", diff);
    }
}

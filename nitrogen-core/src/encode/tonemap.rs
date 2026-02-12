//! HDR to SDR tonemapping for Nitrogen
//!
//! Provides tonemapping algorithms to convert HDR content (HDR10/PQ, HLG)
//! to SDR for compatibility with Discord and other streaming platforms.
//!
//! Supported algorithms:
//! - Reinhard (simple, preserves colors well)
//! - ACES (filmic look, used in film production)
//! - Hable (Uncharted 2 filmic curve)

use crate::types::{HdrMetadata, TransferFunction};
use serde::{Deserialize, Serialize};

/// Tonemapping algorithm selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TonemapAlgorithm {
    /// Reinhard tonemapping - simple and preserves colors
    #[default]
    Reinhard,
    /// ACES filmic tonemapping - cinematic look
    Aces,
    /// Hable/Uncharted 2 filmic curve
    Hable,
}

impl std::fmt::Display for TonemapAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TonemapAlgorithm::Reinhard => write!(f, "Reinhard"),
            TonemapAlgorithm::Aces => write!(f, "ACES"),
            TonemapAlgorithm::Hable => write!(f, "Hable"),
        }
    }
}

impl std::str::FromStr for TonemapAlgorithm {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "reinhard" => Ok(TonemapAlgorithm::Reinhard),
            "aces" => Ok(TonemapAlgorithm::Aces),
            "hable" | "uncharted2" | "filmic" => Ok(TonemapAlgorithm::Hable),
            _ => Err(format!("Unknown tonemap algorithm: {}", s)),
        }
    }
}

/// Tonemapping mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TonemapMode {
    /// Automatic: tonemap if HDR content detected
    #[default]
    Auto,
    /// Always apply tonemapping
    On,
    /// Never apply tonemapping (passthrough)
    Off,
}

impl std::str::FromStr for TonemapMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(TonemapMode::Auto),
            "on" | "true" | "yes" | "1" => Ok(TonemapMode::On),
            "off" | "false" | "no" | "0" => Ok(TonemapMode::Off),
            _ => Err(format!("Unknown tonemap mode: {}", s)),
        }
    }
}

impl std::fmt::Display for TonemapMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TonemapMode::Auto => write!(f, "auto"),
            TonemapMode::On => write!(f, "on"),
            TonemapMode::Off => write!(f, "off"),
        }
    }
}

/// Tonemapping configuration
#[derive(Debug, Clone)]
pub struct TonemapConfig {
    /// Tonemapping mode
    pub mode: TonemapMode,
    /// Algorithm to use
    pub algorithm: TonemapAlgorithm,
    /// Peak luminance override (nits) - used when metadata is unavailable
    pub peak_luminance: u32,
    /// Target SDR white point (nits), typically 100-203
    pub sdr_white_point: u32,
}

impl Default for TonemapConfig {
    fn default() -> Self {
        Self {
            mode: TonemapMode::Auto,
            algorithm: TonemapAlgorithm::Reinhard,
            peak_luminance: 1000,
            sdr_white_point: 100,
        }
    }
}

/// HDR tonemapper
pub struct Tonemapper {
    config: TonemapConfig,
    /// Precomputed PQ EOTF lookup table (12-bit input -> linear)
    pq_to_linear_lut: Vec<f32>,
    /// Precomputed linear -> SDR gamma lookup table (linear -> 8-bit)
    linear_to_sdr_lut: Vec<u8>,
}

impl Tonemapper {
    /// Create a new tonemapper with the given configuration
    pub fn new(config: TonemapConfig) -> Self {
        // Build PQ EOTF lookup table (4096 entries for 12-bit precision)
        let pq_to_linear_lut: Vec<f32> = (0..4096)
            .map(|i| {
                let normalized = i as f32 / 4095.0;
                pq_eotf(normalized)
            })
            .collect();

        // Build linear to SDR gamma lookup table (1024 entries)
        let linear_to_sdr_lut: Vec<u8> = (0..1024)
            .map(|i| {
                let linear = i as f32 / 1023.0;
                let gamma = bt1886_oetf(linear);
                (gamma * 255.0).clamp(0.0, 255.0) as u8
            })
            .collect();

        Self {
            config,
            pq_to_linear_lut,
            linear_to_sdr_lut,
        }
    }

    /// Check if tonemapping should be applied for the given metadata
    pub fn should_tonemap(&self, metadata: Option<&HdrMetadata>) -> bool {
        match self.config.mode {
            TonemapMode::Off => false,
            TonemapMode::On => true,
            TonemapMode::Auto => metadata.map(|m| m.is_hdr()).unwrap_or(false),
        }
    }

    /// Tonemap an HDR frame to SDR
    ///
    /// The frame data is expected to be in RGBA or BGRA format (4 bytes per pixel).
    /// For HDR10, the input should be 10-bit values packed into 16-bit or represented
    /// as normalized 8-bit for this simplified implementation.
    ///
    /// # Arguments
    /// * `frame` - Mutable frame buffer (RGBA/BGRA, 4 bytes per pixel)
    /// * `width` - Frame width in pixels
    /// * `height` - Frame height in pixels
    /// * `metadata` - HDR metadata for the frame
    pub fn tonemap(
        &self,
        frame: &mut [u8],
        width: u32,
        height: u32,
        metadata: Option<&HdrMetadata>,
    ) {
        if !self.should_tonemap(metadata) {
            return;
        }

        let default_metadata = HdrMetadata::default();
        let metadata = metadata.unwrap_or(&default_metadata);
        let peak_nits = metadata.peak_luminance().max(self.config.peak_luminance);

        // Scaling factor from peak luminance to SDR white point
        let scale = self.config.sdr_white_point as f32 / peak_nits as f32;

        let pixel_count = (width * height) as usize;
        let expected_size = pixel_count * 4;

        if frame.len() < expected_size {
            tracing::warn!(
                "Frame buffer too small for tonemapping: {} < {}",
                frame.len(),
                expected_size
            );
            return;
        }

        match metadata.transfer {
            TransferFunction::Pq => {
                self.tonemap_pq(frame, pixel_count, scale);
            }
            TransferFunction::Hlg => {
                self.tonemap_hlg(frame, pixel_count, scale);
            }
            TransferFunction::Sdr => {
                // Nothing to do for SDR
            }
        }
    }

    /// Tonemap PQ (HDR10) content
    fn tonemap_pq(&self, frame: &mut [u8], pixel_count: usize, scale: f32) {
        for i in 0..pixel_count {
            let offset = i * 4;

            // Read RGB values (assuming BGRA or RGBA - we process RGB the same)
            let r = frame[offset] as f32 / 255.0;
            let g = frame[offset + 1] as f32 / 255.0;
            let b = frame[offset + 2] as f32 / 255.0;
            // Alpha stays unchanged

            // Convert from PQ to linear light (using LUT for the 8-bit -> 12-bit approximation)
            let r_linear = self.pq_lookup(r);
            let g_linear = self.pq_lookup(g);
            let b_linear = self.pq_lookup(b);

            // Apply tonemapping in linear space
            let (r_tm, g_tm, b_tm) = self.apply_tonemap(r_linear, g_linear, b_linear, scale);

            // Convert back to SDR gamma
            frame[offset] = self.linear_to_sdr(r_tm);
            frame[offset + 1] = self.linear_to_sdr(g_tm);
            frame[offset + 2] = self.linear_to_sdr(b_tm);
        }
    }

    /// Tonemap HLG content
    fn tonemap_hlg(&self, frame: &mut [u8], pixel_count: usize, scale: f32) {
        for i in 0..pixel_count {
            let offset = i * 4;

            let r = frame[offset] as f32 / 255.0;
            let g = frame[offset + 1] as f32 / 255.0;
            let b = frame[offset + 2] as f32 / 255.0;

            // Convert from HLG to linear light
            let r_linear = hlg_eotf(r);
            let g_linear = hlg_eotf(g);
            let b_linear = hlg_eotf(b);

            // Apply tonemapping
            let (r_tm, g_tm, b_tm) = self.apply_tonemap(r_linear, g_linear, b_linear, scale);

            // Convert to SDR gamma
            frame[offset] = self.linear_to_sdr(r_tm);
            frame[offset + 1] = self.linear_to_sdr(g_tm);
            frame[offset + 2] = self.linear_to_sdr(b_tm);
        }
    }

    /// Look up PQ value in EOTF table
    fn pq_lookup(&self, pq_value: f32) -> f32 {
        let index = (pq_value * 4095.0).clamp(0.0, 4095.0) as usize;
        self.pq_to_linear_lut[index]
    }

    /// Convert linear value to SDR using LUT
    fn linear_to_sdr(&self, linear: f32) -> u8 {
        let index = (linear * 1023.0).clamp(0.0, 1023.0) as usize;
        self.linear_to_sdr_lut[index]
    }

    /// Apply the configured tonemapping algorithm
    fn apply_tonemap(&self, r: f32, g: f32, b: f32, scale: f32) -> (f32, f32, f32) {
        // Scale down from HDR luminance range
        let r = r * scale;
        let g = g * scale;
        let b = b * scale;

        match self.config.algorithm {
            TonemapAlgorithm::Reinhard => {
                (reinhard(r), reinhard(g), reinhard(b))
            }
            TonemapAlgorithm::Aces => {
                aces_tonemap(r, g, b)
            }
            TonemapAlgorithm::Hable => {
                (hable(r), hable(g), hable(b))
            }
        }
    }

    /// Get the current configuration
    pub fn config(&self) -> &TonemapConfig {
        &self.config
    }

    /// Update the configuration
    pub fn set_config(&mut self, config: TonemapConfig) {
        self.config = config;
    }
}

// ============================================================================
// Transfer function implementations
// ============================================================================

/// PQ (ST 2084) constants
const PQ_M1: f32 = 0.1593017578125;
const PQ_M2: f32 = 78.84375;
const PQ_C1: f32 = 0.8359375;
const PQ_C2: f32 = 18.8515625;
const PQ_C3: f32 = 18.6875;

/// PQ EOTF (Electro-Optical Transfer Function)
/// Converts PQ signal to linear light (normalized to 10000 nits)
fn pq_eotf(pq: f32) -> f32 {
    if pq <= 0.0 {
        return 0.0;
    }

    let pq_pow = pq.powf(1.0 / PQ_M2);
    let numerator = (pq_pow - PQ_C1).max(0.0);
    let denominator = PQ_C2 - PQ_C3 * pq_pow;

    if denominator <= 0.0 {
        return 0.0;
    }

    (numerator / denominator).powf(1.0 / PQ_M1)
}

/// HLG EOTF constants
const HLG_A: f32 = 0.17883277;
const HLG_B: f32 = 0.28466892; // 1 - 4 * HLG_A
const HLG_C: f32 = 0.55991073; // 0.5 - HLG_A * ln(4 * HLG_A)

/// HLG OETF inverse (signal to scene-linear)
fn hlg_eotf(hlg: f32) -> f32 {
    if hlg <= 0.5 {
        (hlg * hlg) / 3.0
    } else {
        (((hlg - HLG_C) / HLG_A).exp() + HLG_B) / 12.0
    }
}

/// BT.1886 OETF (linear to display gamma ~2.4)
fn bt1886_oetf(linear: f32) -> f32 {
    if linear <= 0.0 {
        0.0
    } else {
        linear.powf(1.0 / 2.4)
    }
}

// ============================================================================
// Tonemapping operators
// ============================================================================

/// Reinhard tonemapping
/// Simple and effective, preserves colors well
fn reinhard(x: f32) -> f32 {
    x / (1.0 + x)
}

/// Extended Reinhard with white point
#[allow(dead_code)]
fn reinhard_extended(x: f32, white_point: f32) -> f32 {
    let numerator = x * (1.0 + x / (white_point * white_point));
    numerator / (1.0 + x)
}

/// Hable/Uncharted 2 tonemapping curve component
fn hable_partial(x: f32) -> f32 {
    const A: f32 = 0.15; // Shoulder strength
    const B: f32 = 0.50; // Linear strength
    const C: f32 = 0.10; // Linear angle
    const D: f32 = 0.20; // Toe strength
    const E: f32 = 0.02; // Toe numerator
    const F: f32 = 0.30; // Toe denominator

    ((x * (A * x + C * B) + D * E) / (x * (A * x + B) + D * F)) - E / F
}

/// Hable/Uncharted 2 filmic tonemapping
fn hable(x: f32) -> f32 {
    const EXPOSURE_BIAS: f32 = 2.0;
    const WHITE_POINT: f32 = 11.2;

    let curr = hable_partial(x * EXPOSURE_BIAS);
    let white_scale = 1.0 / hable_partial(WHITE_POINT);

    curr * white_scale
}

/// ACES filmic tonemapping (approximation)
/// Based on the RRT+ODT fit by Krzysztof Narkowicz
fn aces_tonemap(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    // ACES input matrix (sRGB to ACES)
    let aces_r = r * 0.59719 + g * 0.35458 + b * 0.04823;
    let aces_g = r * 0.07600 + g * 0.90834 + b * 0.01566;
    let aces_b = r * 0.02840 + g * 0.13383 + b * 0.83777;

    // Apply RRT+ODT approximation
    fn rrt_odt(x: f32) -> f32 {
        const A: f32 = 0.0245786;
        const B: f32 = 0.000090537;
        const C: f32 = 0.983729;
        const D: f32 = 0.4329510;
        const E: f32 = 0.238081;

        (x * (x + A) - B) / (x * (C * x + D) + E)
    }

    let out_r = rrt_odt(aces_r);
    let out_g = rrt_odt(aces_g);
    let out_b = rrt_odt(aces_b);

    // ACES output matrix (ACES to sRGB)
    let srgb_r = out_r * 1.60475 + out_g * -0.53108 + out_b * -0.07367;
    let srgb_g = out_r * -0.10208 + out_g * 1.10813 + out_b * -0.00605;
    let srgb_b = out_r * -0.00327 + out_g * -0.07276 + out_b * 1.07602;

    (srgb_r.clamp(0.0, 1.0), srgb_g.clamp(0.0, 1.0), srgb_b.clamp(0.0, 1.0))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tonemap_algorithm_from_str() {
        assert_eq!("reinhard".parse::<TonemapAlgorithm>().unwrap(), TonemapAlgorithm::Reinhard);
        assert_eq!("aces".parse::<TonemapAlgorithm>().unwrap(), TonemapAlgorithm::Aces);
        assert_eq!("hable".parse::<TonemapAlgorithm>().unwrap(), TonemapAlgorithm::Hable);
        assert_eq!("filmic".parse::<TonemapAlgorithm>().unwrap(), TonemapAlgorithm::Hable);
        assert!("invalid".parse::<TonemapAlgorithm>().is_err());
    }

    #[test]
    fn test_tonemap_mode_from_str() {
        assert_eq!("auto".parse::<TonemapMode>().unwrap(), TonemapMode::Auto);
        assert_eq!("on".parse::<TonemapMode>().unwrap(), TonemapMode::On);
        assert_eq!("off".parse::<TonemapMode>().unwrap(), TonemapMode::Off);
        assert_eq!("true".parse::<TonemapMode>().unwrap(), TonemapMode::On);
        assert_eq!("false".parse::<TonemapMode>().unwrap(), TonemapMode::Off);
    }

    #[test]
    fn test_pq_eotf() {
        // PQ signal 0 -> linear 0
        assert!((pq_eotf(0.0) - 0.0).abs() < 0.001);

        // PQ signal 0.5 should give some positive value
        let mid = pq_eotf(0.5);
        assert!(mid > 0.0 && mid < 1.0);

        // PQ signal 1 -> linear 1 (10000 nits normalized)
        assert!((pq_eotf(1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_hlg_eotf() {
        // HLG signal 0 -> linear 0
        assert!((hlg_eotf(0.0) - 0.0).abs() < 0.001);

        // HLG is scene-referred, test continuity at 0.5
        let just_below = hlg_eotf(0.49);
        let just_above = hlg_eotf(0.51);
        assert!((just_below - just_above).abs() < 0.1);
    }

    #[test]
    fn test_reinhard() {
        assert!((reinhard(0.0) - 0.0).abs() < 0.001);
        assert!((reinhard(1.0) - 0.5).abs() < 0.001);
        // High values should approach 1.0
        assert!(reinhard(100.0) > 0.99);
    }

    #[test]
    fn test_hable() {
        assert!(hable(0.0) >= 0.0);
        let mid = hable(1.0);
        assert!(mid > 0.0 && mid <= 1.0);
    }

    #[test]
    fn test_aces_tonemap() {
        let (r, g, b) = aces_tonemap(0.0, 0.0, 0.0);
        assert!(r >= 0.0 && g >= 0.0 && b >= 0.0);

        let (r, g, b) = aces_tonemap(1.0, 1.0, 1.0);
        assert!(r <= 1.0 && g <= 1.0 && b <= 1.0);
    }

    #[test]
    fn test_should_tonemap() {
        let config = TonemapConfig::default();
        let tonemapper = Tonemapper::new(config);

        // Auto mode with SDR metadata -> no tonemap
        let sdr_meta = HdrMetadata::sdr();
        assert!(!tonemapper.should_tonemap(Some(&sdr_meta)));

        // Auto mode with HDR metadata -> tonemap
        let hdr_meta = HdrMetadata::hdr10();
        assert!(tonemapper.should_tonemap(Some(&hdr_meta)));

        // Auto mode with no metadata -> no tonemap
        assert!(!tonemapper.should_tonemap(None));
    }

    #[test]
    fn test_should_tonemap_forced() {
        let mut config = TonemapConfig::default();
        config.mode = TonemapMode::On;
        let tonemapper = Tonemapper::new(config);

        // Forced on always tonemaps
        assert!(tonemapper.should_tonemap(None));
        assert!(tonemapper.should_tonemap(Some(&HdrMetadata::sdr())));
    }

    #[test]
    fn test_tonemap_frame() {
        let config = TonemapConfig {
            mode: TonemapMode::On,
            algorithm: TonemapAlgorithm::Reinhard,
            peak_luminance: 1000,
            sdr_white_point: 100,
        };
        let tonemapper = Tonemapper::new(config);

        // Create a small test frame (2x2 RGBA)
        let mut frame = vec![
            128, 128, 128, 255, // Pixel 1
            255, 255, 255, 255, // Pixel 2
            0, 0, 0, 255,       // Pixel 3
            200, 100, 50, 255,  // Pixel 4
        ];

        let original = frame.clone();
        tonemapper.tonemap(&mut frame, 2, 2, Some(&HdrMetadata::hdr10()));

        // Frame should be modified
        assert_ne!(frame, original);

        // Alpha should be preserved
        assert_eq!(frame[3], 255);
        assert_eq!(frame[7], 255);
        assert_eq!(frame[11], 255);
        assert_eq!(frame[15], 255);
    }

    #[test]
    fn test_tonemap_sdr_passthrough() {
        let config = TonemapConfig::default(); // Auto mode
        let tonemapper = Tonemapper::new(config);

        let mut frame = vec![128, 128, 128, 255];
        let original = frame.clone();

        // SDR metadata should not be tonemapped
        tonemapper.tonemap(&mut frame, 1, 1, Some(&HdrMetadata::sdr()));
        assert_eq!(frame, original);
    }

    #[test]
    fn test_hdr_metadata_peak_luminance() {
        let mut meta = HdrMetadata::hdr10();
        assert_eq!(meta.peak_luminance(), 1000); // Default

        meta.max_cll = Some(500);
        assert_eq!(meta.peak_luminance(), 500);

        meta.max_cll = None;
        meta.mastering_max_luminance = Some(4000);
        assert_eq!(meta.peak_luminance(), 4000);
    }
}

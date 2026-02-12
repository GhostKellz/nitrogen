//! GPU detection and capability querying
//!
//! Provides detection of GPU capabilities, particularly for:
//! - RTX 50 series (Blackwell) UHQ encoding features
//! - NVENC AV1 feature detection
//! - GPU generation identification

use std::process::Command;

use crate::error::{NitrogenError, Result};

/// RTX 50 series feature capabilities
#[derive(Debug, Clone)]
pub struct Rtx50Features {
    /// GPU name
    pub name: String,
    /// Is RTX 50 series (Blackwell)
    pub is_rtx50: bool,
    /// Ultra High Quality (UHQ) tuning supported (~8% efficiency gain)
    pub uhq_supported: bool,
    /// Temporal AQ supported (~4-5% efficiency gain)
    pub temporal_aq_supported: bool,
    /// YUV 4:2:2 chroma supported
    pub yuv422_supported: bool,
    /// YUV 4:4:4 chroma supported
    pub yuv444_supported: bool,
    /// B-frame reference mode supported
    pub b_ref_supported: bool,
    /// Extended lookahead depth (up to 250 frames)
    pub extended_lookahead: bool,
}

impl Default for Rtx50Features {
    fn default() -> Self {
        Self {
            name: "Unknown".to_string(),
            is_rtx50: false,
            uhq_supported: false,
            temporal_aq_supported: false,
            yuv422_supported: false,
            yuv444_supported: false,
            b_ref_supported: false,
            extended_lookahead: false,
        }
    }
}

/// GPU generation enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuGeneration {
    /// Pre-Turing (Pascal, etc.)
    Legacy,
    /// RTX 20 series (Turing)
    Turing,
    /// RTX 30 series (Ampere)
    Ampere,
    /// RTX 40 series (Ada Lovelace)
    AdaLovelace,
    /// RTX 50 series (Blackwell)
    Blackwell,
    /// Unknown generation
    Unknown,
}

impl GpuGeneration {
    pub fn supports_av1(&self) -> bool {
        matches!(self, Self::AdaLovelace | Self::Blackwell)
    }

    pub fn supports_uhq(&self) -> bool {
        matches!(self, Self::Blackwell)
    }
}

/// Detect RTX 50 series features for a given GPU index
pub fn detect_rtx50_features(gpu_index: u32) -> Result<Rtx50Features> {
    let gpu_name = query_gpu_name(gpu_index)?;
    let generation = detect_generation(&gpu_name);

    let is_rtx50 = generation == GpuGeneration::Blackwell;

    Ok(Rtx50Features {
        name: gpu_name,
        is_rtx50,
        uhq_supported: is_rtx50,
        temporal_aq_supported: is_rtx50,
        yuv422_supported: is_rtx50,
        yuv444_supported: is_rtx50,
        b_ref_supported: is_rtx50,
        extended_lookahead: is_rtx50,
    })
}

/// Query GPU name from nvidia-smi
fn query_gpu_name(gpu_index: u32) -> Result<String> {
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name",
            "--format=csv,noheader,nounits",
            &format!("--id={}", gpu_index),
        ])
        .output()
        .map_err(|e| NitrogenError::Nvenc(format!("Failed to run nvidia-smi: {}", e)))?;

    if !output.status.success() {
        return Err(NitrogenError::Nvenc(
            "nvidia-smi query failed".to_string(),
        ));
    }

    let name = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_string();

    if name.is_empty() {
        return Err(NitrogenError::Nvenc(
            "Empty GPU name from nvidia-smi".to_string(),
        ));
    }

    Ok(name)
}

/// Detect GPU generation from name string
fn detect_generation(name: &str) -> GpuGeneration {
    let name_upper = name.to_uppercase();

    // RTX 50 series (Blackwell) - GeForce RTX 5090, 5080, 5070, etc.
    if name_upper.contains("RTX 50") || name_upper.contains("RTX50") {
        return GpuGeneration::Blackwell;
    }

    // RTX 40 series (Ada Lovelace) - GeForce RTX 4090, 4080, 4070, etc.
    if name_upper.contains("RTX 40") || name_upper.contains("RTX40") {
        return GpuGeneration::AdaLovelace;
    }

    // RTX 30 series (Ampere) - GeForce RTX 3090, 3080, 3070, etc.
    if name_upper.contains("RTX 30") || name_upper.contains("RTX30") {
        return GpuGeneration::Ampere;
    }

    // RTX 20 series (Turing) - GeForce RTX 2080, 2070, etc.
    if name_upper.contains("RTX 20") || name_upper.contains("RTX20") {
        return GpuGeneration::Turing;
    }

    // GTX 16 series (Turing without RT cores)
    if name_upper.contains("GTX 16") || name_upper.contains("GTX16") {
        return GpuGeneration::Turing;
    }

    // GTX 10 series (Pascal)
    if name_upper.contains("GTX 10") || name_upper.contains("GTX10") {
        return GpuGeneration::Legacy;
    }

    // Professional cards
    if name_upper.contains("QUADRO") || name_upper.contains("TESLA") || name_upper.contains("A100")
    {
        // Check for Ada/Blackwell professional cards
        if name_upper.contains("RTX 6000") {
            return GpuGeneration::AdaLovelace;
        }
        if name_upper.contains("RTX 5000") || name_upper.contains("RTX 4000") {
            return GpuGeneration::AdaLovelace;
        }
    }

    GpuGeneration::Unknown
}

/// Get GPU generation for a given index
pub fn get_gpu_generation(gpu_index: u32) -> Result<GpuGeneration> {
    let name = query_gpu_name(gpu_index)?;
    Ok(detect_generation(&name))
}

/// Check if AV1 encoding is supported
pub fn supports_av1(gpu_index: u32) -> Result<bool> {
    let generation = get_gpu_generation(gpu_index)?;
    Ok(generation.supports_av1())
}

/// Get recommended AV1 settings for the detected GPU
pub fn get_recommended_av1_settings(gpu_index: u32) -> Result<RecommendedAv1Settings> {
    let features = detect_rtx50_features(gpu_index)?;
    let generation = detect_generation(&features.name);

    Ok(RecommendedAv1Settings {
        generation,
        tune: if features.uhq_supported {
            "uhq"
        } else {
            "hq"
        },
        temporal_aq: features.temporal_aq_supported,
        lookahead_depth: if features.extended_lookahead { 100 } else { 20 },
        b_ref_mode: features.b_ref_supported,
        chroma: if features.yuv444_supported {
            "444"
        } else if features.yuv422_supported {
            "422"
        } else {
            "420"
        },
    })
}

/// Recommended AV1 encoder settings
#[derive(Debug, Clone)]
pub struct RecommendedAv1Settings {
    pub generation: GpuGeneration,
    pub tune: &'static str,
    pub temporal_aq: bool,
    pub lookahead_depth: u32,
    pub b_ref_mode: bool,
    pub chroma: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_generation() {
        assert_eq!(
            detect_generation("NVIDIA GeForce RTX 5090"),
            GpuGeneration::Blackwell
        );
        assert_eq!(
            detect_generation("NVIDIA GeForce RTX 4090"),
            GpuGeneration::AdaLovelace
        );
        assert_eq!(
            detect_generation("NVIDIA GeForce RTX 3080"),
            GpuGeneration::Ampere
        );
        assert_eq!(
            detect_generation("NVIDIA GeForce RTX 2080 Ti"),
            GpuGeneration::Turing
        );
        assert_eq!(
            detect_generation("NVIDIA GeForce GTX 1080"),
            GpuGeneration::Legacy
        );
    }

    #[test]
    fn test_generation_capabilities() {
        assert!(GpuGeneration::Blackwell.supports_av1());
        assert!(GpuGeneration::AdaLovelace.supports_av1());
        assert!(!GpuGeneration::Ampere.supports_av1());
        assert!(GpuGeneration::Blackwell.supports_uhq());
        assert!(!GpuGeneration::AdaLovelace.supports_uhq());
    }
}

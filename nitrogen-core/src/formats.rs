//! Pixel format constants and conversions
//!
//! Centralizes DRM fourcc format handling to avoid duplication across modules.
//! All format constants and conversion functions should be defined here.

use ghoststream::types::FrameFormat as GsFrameFormat;

/// DRM format fourcc constants
///
/// These are the standard DRM/KMS fourcc codes for pixel formats.
/// See: <https://github.com/torvalds/linux/blob/master/include/uapi/drm/drm_fourcc.h>
pub mod fourcc {
    /// XRGB8888 - 32-bit RGB with unused alpha (X = ignored)
    pub const XRGB8888: u32 = 0x34325258; // XR24
    /// XBGR8888 - 32-bit BGR with unused alpha
    pub const XBGR8888: u32 = 0x34324258; // XB24
    /// ARGB8888 - 32-bit RGB with alpha
    pub const ARGB8888: u32 = 0x34325241; // AR24
    /// ABGR8888 - 32-bit BGR with alpha
    pub const ABGR8888: u32 = 0x34324142; // AB24
    /// RGBA8888 - 32-bit RGBA
    pub const RGBA8888: u32 = 0x34324241; // BA24
    /// RGBX8888 - 32-bit RGB with padding
    pub const RGBX8888: u32 = 0x34324152; // RA24
    /// BGRX8888 - 32-bit BGR with padding
    pub const BGRX8888: u32 = 0x34325842; // BX24
    /// BGRA8888 - 32-bit BGRA
    pub const BGRA8888: u32 = 0x41524742; // BGRA
    /// RGB888 - 24-bit RGB
    pub const RGB888: u32 = 0x20424752; // RGB
    /// BGR888 - 24-bit BGR
    pub const BGR888: u32 = 0x20524742; // BGR
    /// NV12 - YUV 4:2:0 semi-planar
    pub const NV12: u32 = 0x3231564E; // NV12
    /// YUY2/YUYV - YUV 4:2:2 packed
    pub const YUY2: u32 = 0x56595559; // YUYV
    /// P010 - 10-bit YUV 4:2:0 (HDR)
    pub const P010: u32 = 0x30313050; // P010
}

/// Get the bytes per pixel for a fourcc format
///
/// Returns the number of bytes per pixel, or 4 as a safe default for unknown formats.
pub fn bytes_per_pixel(fourcc: u32) -> u32 {
    use fourcc::*;
    match fourcc {
        // 32-bit formats (4 bytes per pixel)
        XRGB8888 | XBGR8888 | ARGB8888 | ABGR8888 | RGBA8888 | RGBX8888 | BGRX8888 | BGRA8888 => 4,
        // 24-bit formats (3 bytes per pixel)
        RGB888 | BGR888 => 3,
        // YUV formats (1.5 bytes per pixel average for NV12, 2 for YUY2)
        NV12 => 2, // Actually 1.5, but we use stride-based calculations
        YUY2 => 2,
        P010 => 2, // 10-bit YUV
        // Default to 4 bytes (safe assumption for most desktop formats)
        _ => 4,
    }
}

/// Check if a fourcc format is HDR-capable (10-bit or higher)
pub fn is_hdr_format(fourcc: u32) -> bool {
    matches!(fourcc, fourcc::P010)
}

/// Convert DRM fourcc to ghoststream FrameFormat
pub fn fourcc_to_gs_format(fourcc: u32) -> GsFrameFormat {
    use fourcc::*;
    match fourcc {
        // XRGB/BGRX formats - most common from Wayland
        XRGB8888 | XBGR8888 | BGRX8888 => GsFrameFormat::Bgra,
        // ARGB/BGRA formats
        ARGB8888 | ABGR8888 | BGRA8888 => GsFrameFormat::Bgra,
        // RGBA formats
        RGBA8888 | RGBX8888 => GsFrameFormat::Rgba,
        // YUV formats
        NV12 => GsFrameFormat::Nv12,
        _ => {
            tracing::debug!("Unknown fourcc 0x{:08x}, treating as BGRA", fourcc);
            GsFrameFormat::Bgra
        }
    }
}

/// Format information for debugging
pub fn format_name(fourcc: u32) -> &'static str {
    use fourcc::*;
    match fourcc {
        XRGB8888 => "XRGB8888",
        XBGR8888 => "XBGR8888",
        ARGB8888 => "ARGB8888",
        ABGR8888 => "ABGR8888",
        RGBA8888 => "RGBA8888",
        RGBX8888 => "RGBX8888",
        BGRX8888 => "BGRX8888",
        BGRA8888 => "BGRA8888",
        RGB888 => "RGB888",
        BGR888 => "BGR888",
        NV12 => "NV12",
        YUY2 => "YUY2",
        P010 => "P010",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes_per_pixel() {
        assert_eq!(bytes_per_pixel(fourcc::XRGB8888), 4);
        assert_eq!(bytes_per_pixel(fourcc::RGB888), 3);
        assert_eq!(bytes_per_pixel(fourcc::NV12), 2);
    }

    #[test]
    fn test_fourcc_to_gs_format() {
        assert_eq!(fourcc_to_gs_format(fourcc::XRGB8888), GsFrameFormat::Bgra);
        assert_eq!(fourcc_to_gs_format(fourcc::NV12), GsFrameFormat::Nv12);
    }

    #[test]
    fn test_is_hdr_format() {
        assert!(!is_hdr_format(fourcc::XRGB8888));
        assert!(is_hdr_format(fourcc::P010));
    }

    #[test]
    fn test_format_name() {
        assert_eq!(format_name(fourcc::XRGB8888), "XRGB8888");
        assert_eq!(format_name(0xDEADBEEF), "Unknown");
    }
}

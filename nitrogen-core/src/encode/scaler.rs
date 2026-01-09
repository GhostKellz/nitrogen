//! Frame scaling and format conversion
//!
//! Handles resolution changes and pixel format conversion
//! between capture and encoding.

use ffmpeg_next::format::Pixel;
use ffmpeg_next::software::scaling::{self, Flags};
use ffmpeg_next::util::frame::video::Video;
use tracing::debug;

use crate::error::{NitrogenError, Result};

/// Frame scaler for resolution and format conversion
pub struct FrameScaler {
    /// FFmpeg scaling context
    context: scaling::Context,
    /// Output frame buffer
    output: Video,
    /// Output width
    width: u32,
    /// Output height
    height: u32,
}

impl FrameScaler {
    /// Create a new frame scaler
    pub fn new(
        src_width: u32,
        src_height: u32,
        src_format: Pixel,
        dst_width: u32,
        dst_height: u32,
        dst_format: Pixel,
    ) -> Result<Self> {
        debug!(
            "Creating scaler: {:?} {}x{} -> {:?} {}x{}",
            src_format, src_width, src_height, dst_format, dst_width, dst_height
        );

        let context = scaling::Context::get(
            src_format,
            src_width,
            src_height,
            dst_format,
            dst_width,
            dst_height,
            Flags::BILINEAR,
        )
        .map_err(|e| NitrogenError::encoder(format!("Failed to create scaler: {}", e)))?;

        let output = Video::new(dst_format, dst_width, dst_height);

        Ok(Self {
            context,
            output,
            width: dst_width,
            height: dst_height,
        })
    }

    /// Scale a frame
    pub fn scale(&mut self, input: &Video) -> Result<&Video> {
        self.context
            .run(input, &mut self.output)
            .map_err(|e| NitrogenError::encoder(format!("Scaling failed: {}", e)))?;

        Ok(&self.output)
    }

    /// Get output dimensions
    pub fn output_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Calculate scaled dimensions maintaining aspect ratio
#[allow(dead_code)]
pub fn calculate_scaled_size(
    src_width: u32,
    src_height: u32,
    max_width: u32,
    max_height: u32,
) -> (u32, u32) {
    let src_aspect = src_width as f64 / src_height as f64;
    let dst_aspect = max_width as f64 / max_height as f64;

    if src_aspect > dst_aspect {
        // Width-limited
        let height = (max_width as f64 / src_aspect) as u32;
        (max_width, height & !1) // Ensure even
    } else {
        // Height-limited
        let width = (max_height as f64 * src_aspect) as u32;
        (width & !1, max_height) // Ensure even
    }
}

/// Calculate crop region for aspect ratio conversion
#[allow(dead_code)]
pub fn calculate_crop(src_width: u32, src_height: u32, dst_aspect: f64) -> (u32, u32, u32, u32) {
    let src_aspect = src_width as f64 / src_height as f64;

    if src_aspect > dst_aspect {
        // Source is wider, crop width
        let new_width = (src_height as f64 * dst_aspect) as u32;
        let offset = (src_width - new_width) / 2;
        (offset, 0, new_width, src_height)
    } else {
        // Source is taller, crop height
        let new_height = (src_width as f64 / dst_aspect) as u32;
        let offset = (src_height - new_height) / 2;
        (0, offset, src_width, new_height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scaled_size() {
        // 4K to 1080p
        let (w, h) = calculate_scaled_size(3840, 2160, 1920, 1080);
        assert_eq!(w, 1920);
        assert_eq!(h, 1080);

        // Ultrawide to 16:9
        let (w, h) = calculate_scaled_size(3440, 1440, 1920, 1080);
        assert_eq!(w, 1920);
        // Height will be adjusted to maintain aspect ratio
        assert!(h < 1080);
    }

    #[test]
    fn test_crop() {
        // Ultrawide (21:9) to 16:9
        let (x, y, w, h) = calculate_crop(3440, 1440, 16.0 / 9.0);
        assert!(x > 0); // Should crop from sides
        assert_eq!(y, 0);
        assert!(w < 3440);
        assert_eq!(h, 1440);
    }
}

//! Latency overlay rendering for Nitrogen
//!
//! Renders performance statistics as text overlay on video frames.
//! Uses simple bitmap font rendering for minimal dependencies.

use crate::performance::LatencyStats;
use serde::{Deserialize, Serialize};

/// Overlay position on screen
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OverlayPosition {
    #[default]
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl OverlayPosition {
    /// Parse from string
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "top-right" | "topright" | "tr" => Self::TopRight,
            "bottom-left" | "bottomleft" | "bl" => Self::BottomLeft,
            "bottom-right" | "bottomright" | "br" => Self::BottomRight,
            _ => Self::TopLeft,
        }
    }
}

/// Configuration for the latency overlay
#[derive(Debug, Clone)]
pub struct OverlayConfig {
    /// Whether overlay is enabled
    pub enabled: bool,
    /// Overlay position
    pub position: OverlayPosition,
    /// Show capture latency
    pub show_capture: bool,
    /// Show encode latency
    pub show_encode: bool,
    /// Show FPS
    pub show_fps: bool,
    /// Show bitrate
    pub show_bitrate: bool,
    /// Show dropped frames
    pub show_drops: bool,
    /// Font scale (1.0 = 8px height base)
    pub font_scale: f32,
    /// Background opacity (0.0 - 1.0)
    pub background_opacity: f32,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            position: OverlayPosition::TopLeft,
            show_capture: true,
            show_encode: true,
            show_fps: true,
            show_bitrate: true,
            show_drops: true,
            font_scale: 1.0,
            background_opacity: 0.7,
        }
    }
}

/// Latency overlay renderer
#[derive(Debug)]
pub struct LatencyOverlay {
    config: OverlayConfig,
}

impl LatencyOverlay {
    /// Create a new overlay renderer with config
    pub fn new(config: OverlayConfig) -> Self {
        Self { config }
    }

    /// Create with default config
    pub fn with_defaults() -> Self {
        Self::new(OverlayConfig::default())
    }

    /// Check if overlay is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Enable or disable the overlay
    pub fn set_enabled(&mut self, enabled: bool) {
        self.config.enabled = enabled;
    }

    /// Toggle overlay on/off
    pub fn toggle(&mut self) {
        self.config.enabled = !self.config.enabled;
    }

    /// Update configuration
    pub fn set_config(&mut self, config: OverlayConfig) {
        self.config = config;
    }

    /// Get current configuration
    pub fn config(&self) -> &OverlayConfig {
        &self.config
    }

    /// Format stats into display text
    fn format_text(&self, stats: &LatencyStats) -> String {
        let mut parts = Vec::new();

        if self.config.show_capture {
            parts.push(format!("Cap:{:.1}ms", stats.capture_latency_ms));
        }
        if self.config.show_encode {
            parts.push(format!("Enc:{:.1}ms", stats.encode_latency_ms));
        }
        if self.config.show_fps {
            parts.push(format!("{:.0}fps", stats.fps));
        }
        if self.config.show_bitrate && stats.bitrate_kbps > 0 {
            parts.push(format!("{}kbps", stats.bitrate_kbps));
        }
        if self.config.show_drops && stats.frames_dropped > 0 {
            parts.push(format!("Drop:{}", stats.frames_dropped));
        }

        parts.join(" | ")
    }

    /// Render overlay onto BGRA frame
    ///
    /// # Arguments
    /// * `frame` - Mutable BGRA pixel data (width * height * 4 bytes)
    /// * `width` - Frame width in pixels
    /// * `height` - Frame height in pixels
    /// * `stats` - Latency statistics to display
    pub fn render(&self, frame: &mut [u8], width: u32, height: u32, stats: &LatencyStats) {
        if !self.config.enabled {
            return;
        }

        let text = self.format_text(stats);
        if text.is_empty() {
            return;
        }

        // Calculate text dimensions
        let char_width = (6.0 * self.config.font_scale) as u32;
        let char_height = (8.0 * self.config.font_scale) as u32;
        let padding = 4u32;
        let text_width = text.len() as u32 * char_width;
        let box_width = text_width + padding * 2;
        let box_height = char_height + padding * 2;

        // Calculate position
        let (box_x, box_y) = match self.config.position {
            OverlayPosition::TopLeft => (padding, padding),
            OverlayPosition::TopRight => (width.saturating_sub(box_width + padding), padding),
            OverlayPosition::BottomLeft => (padding, height.saturating_sub(box_height + padding)),
            OverlayPosition::BottomRight => (
                width.saturating_sub(box_width + padding),
                height.saturating_sub(box_height + padding),
            ),
        };

        // Draw semi-transparent background
        let bg_alpha = (self.config.background_opacity * 255.0) as u8;
        self.draw_rect(frame, width, height, box_x, box_y, box_width, box_height, [0, 0, 0, bg_alpha]);

        // Draw text
        let text_x = box_x + padding;
        let text_y = box_y + padding;
        self.draw_text(frame, width, height, text_x, text_y, &text, [255, 255, 255, 255]);
    }

    /// Draw a filled rectangle with alpha blending
    fn draw_rect(
        &self,
        frame: &mut [u8],
        width: u32,
        height: u32,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
        color: [u8; 4], // BGRA
    ) {
        let stride = width as usize * 4;
        let alpha = color[3] as u32;

        for py in y..y.saturating_add(h).min(height) {
            for px in x..x.saturating_add(w).min(width) {
                let idx = py as usize * stride + px as usize * 4;
                if idx + 3 < frame.len() {
                    // Alpha blend
                    for i in 0..3 {
                        let src = color[i] as u32;
                        let dst = frame[idx + i] as u32;
                        frame[idx + i] = ((src * alpha + dst * (255 - alpha)) / 255) as u8;
                    }
                }
            }
        }
    }

    /// Draw text using simple bitmap font
    fn draw_text(
        &self,
        frame: &mut [u8],
        width: u32,
        height: u32,
        x: u32,
        y: u32,
        text: &str,
        color: [u8; 4],
    ) {
        let scale = self.config.font_scale;
        let char_width = (6.0 * scale) as u32;

        for (i, ch) in text.chars().enumerate() {
            let char_x = x + i as u32 * char_width;
            self.draw_char(frame, width, height, char_x, y, ch, color, scale);
        }
    }

    /// Draw a single character using 5x7 bitmap font
    fn draw_char(
        &self,
        frame: &mut [u8],
        width: u32,
        height: u32,
        x: u32,
        y: u32,
        ch: char,
        color: [u8; 4],
        scale: f32,
    ) {
        let bitmap = get_char_bitmap(ch);
        let stride = width as usize * 4;

        for (row, &bits) in bitmap.iter().enumerate() {
            for col in 0..5 {
                if (bits >> (4 - col)) & 1 == 1 {
                    // Draw scaled pixel
                    let px = x + (col as f32 * scale) as u32;
                    let py = y + (row as f32 * scale) as u32;

                    // Draw scale x scale block
                    for dy in 0..scale.ceil() as u32 {
                        for dx in 0..scale.ceil() as u32 {
                            let fx = px + dx;
                            let fy = py + dy;
                            if fx < width && fy < height {
                                let idx = fy as usize * stride + fx as usize * 4;
                                if idx + 3 < frame.len() {
                                    frame[idx] = color[0];     // B
                                    frame[idx + 1] = color[1]; // G
                                    frame[idx + 2] = color[2]; // R
                                    frame[idx + 3] = color[3]; // A
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Get 5x7 bitmap for a character
/// Each byte represents one row, with 5 bits used (high bits)
fn get_char_bitmap(ch: char) -> [u8; 7] {
    match ch {
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111],
        '3' => [0b01110, 0b10001, 0b00001, 0b00110, 0b00001, 0b10001, 0b01110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        'a' | 'A' => [0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'b' | 'B' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110],
        'c' | 'C' => [0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110],
        'd' | 'D' => [0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110],
        'e' | 'E' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
        'f' | 'F' => [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
        'g' | 'G' => [0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110],
        'h' | 'H' => [0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001],
        'i' | 'I' => [0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        'j' | 'J' => [0b00111, 0b00010, 0b00010, 0b00010, 0b00010, 0b10010, 0b01100],
        'k' | 'K' => [0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001],
        'l' | 'L' => [0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111],
        'm' | 'M' => [0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001],
        'n' | 'N' => [0b10001, 0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001],
        'o' | 'O' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'p' | 'P' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000],
        'q' | 'Q' => [0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101],
        'r' | 'R' => [0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001],
        's' | 'S' => [0b01110, 0b10001, 0b10000, 0b01110, 0b00001, 0b10001, 0b01110],
        't' | 'T' => [0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        'u' | 'U' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110],
        'v' | 'V' => [0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100],
        'w' | 'W' => [0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001],
        'x' | 'X' => [0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001],
        'y' | 'Y' => [0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100],
        'z' | 'Z' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111],
        ':' => [0b00000, 0b00100, 0b00000, 0b00000, 0b00000, 0b00100, 0b00000],
        '.' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00100],
        '|' => [0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100],
        ' ' => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000],
        '-' => [0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000],
        '/' => [0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000],
        '%' => [0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011],
        _ => [0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overlay_position_parse() {
        assert_eq!(OverlayPosition::from_str("top-left"), OverlayPosition::TopLeft);
        assert_eq!(OverlayPosition::from_str("top-right"), OverlayPosition::TopRight);
        assert_eq!(OverlayPosition::from_str("bottom-left"), OverlayPosition::BottomLeft);
        assert_eq!(OverlayPosition::from_str("br"), OverlayPosition::BottomRight);
        assert_eq!(OverlayPosition::from_str("invalid"), OverlayPosition::TopLeft);
    }

    #[test]
    fn test_overlay_toggle() {
        let mut overlay = LatencyOverlay::with_defaults();
        assert!(!overlay.is_enabled());

        overlay.toggle();
        assert!(overlay.is_enabled());

        overlay.toggle();
        assert!(!overlay.is_enabled());
    }

    #[test]
    fn test_format_text() {
        let overlay = LatencyOverlay::new(OverlayConfig {
            enabled: true,
            show_capture: true,
            show_encode: true,
            show_fps: true,
            show_bitrate: false,
            show_drops: false,
            ..Default::default()
        });

        let stats = LatencyStats {
            capture_latency_ms: 2.5,
            encode_latency_ms: 5.0,
            fps: 60.0,
            ..Default::default()
        };

        let text = overlay.format_text(&stats);
        assert!(text.contains("Cap:2.5ms"));
        assert!(text.contains("Enc:5.0ms"));
        assert!(text.contains("60fps"));
    }

    #[test]
    fn test_render_disabled() {
        let overlay = LatencyOverlay::with_defaults();
        let mut frame = vec![0u8; 100 * 100 * 4];
        let stats = LatencyStats::default();

        overlay.render(&mut frame, 100, 100, &stats);

        // Frame should be unchanged when disabled
        assert!(frame.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_render_enabled() {
        let mut overlay = LatencyOverlay::with_defaults();
        overlay.set_enabled(true);

        let mut frame = vec![128u8; 100 * 100 * 4];
        let stats = LatencyStats {
            capture_latency_ms: 2.0,
            encode_latency_ms: 4.0,
            fps: 60.0,
            ..Default::default()
        };

        overlay.render(&mut frame, 100, 100, &stats);

        // Frame should have some changed pixels when enabled
        assert!(frame.iter().any(|&b| b != 128));
    }

    #[test]
    fn test_char_bitmap() {
        // Test that digits return non-zero bitmaps
        for c in '0'..='9' {
            let bitmap = get_char_bitmap(c);
            assert!(bitmap.iter().any(|&b| b != 0), "Digit {} has empty bitmap", c);
        }

        // Test that letters return non-zero bitmaps
        for c in 'A'..='Z' {
            let bitmap = get_char_bitmap(c);
            assert!(bitmap.iter().any(|&b| b != 0), "Letter {} has empty bitmap", c);
        }
    }
}

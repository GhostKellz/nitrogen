//! Mock infrastructure for testing
//!
//! Provides test utilities for creating mock frames and other test helpers.

use nitrogen_core::types::{Frame, FrameData, FrameFormat};
use std::sync::Arc;
use tokio::sync::broadcast;

/// BGRA pixel format (XR24)
pub const FOURCC_BGRA: u32 = 0x34325258;

/// Create a test frame with solid color
///
/// # Arguments
/// * `width` - Frame width in pixels
/// * `height` - Frame height in pixels
/// * `color` - BGRA color values [B, G, R, A]
pub fn create_test_frame(width: u32, height: u32, color: [u8; 4]) -> Frame {
    let stride = width * 4;
    let size = (stride * height) as usize;

    // Create BGRA data
    let mut data = Vec::with_capacity(size);
    for _ in 0..(width * height) {
        data.extend_from_slice(&color);
    }

    Frame {
        format: FrameFormat {
            width,
            height,
            fourcc: FOURCC_BGRA,
            stride,
        },
        data: FrameData::Memory(data),
        pts: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0),
    }
}

/// Create a test frame with a gradient pattern
pub fn create_gradient_frame(width: u32, height: u32) -> Frame {
    let stride = width * 4;
    let size = (stride * height) as usize;

    let mut data = Vec::with_capacity(size);
    for y in 0..height {
        for x in 0..width {
            // Create a diagonal gradient
            let r = ((x as f32 / width as f32) * 255.0) as u8;
            let g = ((y as f32 / height as f32) * 255.0) as u8;
            let b = (((x + y) as f32 / (width + height) as f32) * 255.0) as u8;
            data.extend_from_slice(&[b, g, r, 255]); // BGRA order
        }
    }

    Frame {
        format: FrameFormat {
            width,
            height,
            fourcc: FOURCC_BGRA,
            stride,
        },
        data: FrameData::Memory(data),
        pts: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0),
    }
}

/// Mock frame source that broadcasts test frames
pub struct MockFrameSource {
    sender: broadcast::Sender<Arc<Frame>>,
}

impl MockFrameSource {
    /// Create a new mock frame source
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(16);
        Self { sender }
    }

    /// Subscribe to frames from this source
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Frame>> {
        self.sender.subscribe()
    }

    /// Send a frame to all subscribers
    pub fn send_frame(
        &self,
        frame: Frame,
    ) -> Result<usize, broadcast::error::SendError<Arc<Frame>>> {
        self.sender.send(Arc::new(frame))
    }

    /// Get the number of active receivers
    pub fn receiver_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for MockFrameSource {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_test_frame_dimensions() {
        let frame = create_test_frame(1920, 1080, [255, 0, 0, 255]);
        assert_eq!(frame.format.width, 1920);
        assert_eq!(frame.format.height, 1080);
        assert_eq!(frame.format.stride, 1920 * 4);
    }

    #[test]
    fn test_create_test_frame_data_size() {
        let frame = create_test_frame(100, 100, [0, 255, 0, 255]);
        if let FrameData::Memory(data) = &frame.data {
            assert_eq!(data.len(), 100 * 100 * 4);
        } else {
            panic!("Expected Memory frame data");
        }
    }

    #[test]
    fn test_create_test_frame_color() {
        let color = [10, 20, 30, 40];
        let frame = create_test_frame(1, 1, color);
        if let FrameData::Memory(data) = &frame.data {
            assert_eq!(&data[..], &color);
        } else {
            panic!("Expected Memory frame data");
        }
    }

    #[test]
    fn test_gradient_frame_dimensions() {
        let frame = create_gradient_frame(640, 480);
        assert_eq!(frame.format.width, 640);
        assert_eq!(frame.format.height, 480);
    }

    #[test]
    fn test_mock_frame_source() {
        let source = MockFrameSource::new();
        let mut rx = source.subscribe();

        let frame = create_test_frame(100, 100, [0, 0, 0, 255]);
        source.send_frame(frame).expect("Should send frame");

        let received = rx.try_recv().expect("Should receive frame");
        assert_eq!(received.format.width, 100);
    }
}

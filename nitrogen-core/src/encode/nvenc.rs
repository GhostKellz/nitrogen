//! NVENC encoder implementation via FFmpeg
//!
//! Provides H.264, HEVC, and AV1 encoding using NVIDIA GPUs.

use ffmpeg_next as ffmpeg;
use ffmpeg_next::codec::{self, encoder};
use ffmpeg_next::format::Pixel;
use ffmpeg_next::software::scaling::{self, Flags};
use ffmpeg_next::util::frame::video::Video;
use ffmpeg_next::{Dictionary, Rational};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, info, trace, warn};

use crate::config::{CaptureConfig, Codec, EncoderPreset};
use crate::error::{NitrogenError, Result};
use crate::types::{Frame, FrameData, FrameFormat};

/// NVENC hardware encoder
pub struct NvencEncoder {
    /// FFmpeg encoder context
    encoder: encoder::Video,
    /// Scaler for format conversion if needed
    scaler: Option<scaling::Context>,
    /// Input frame buffer
    frame: Video,
    /// Output packet buffer
    packet: ffmpeg::Packet,
    /// Encoded data sender
    output_tx: broadcast::Sender<Arc<EncodedPacket>>,
    /// Frame counter
    frame_count: u64,
    /// Time base for PTS calculation
    time_base: Rational,
}

/// Encoded video packet
#[derive(Debug, Clone)]
pub struct EncodedPacket {
    /// Encoded data
    pub data: Vec<u8>,
    /// Presentation timestamp
    pub pts: i64,
    /// Decode timestamp
    pub dts: i64,
    /// Is this a keyframe?
    pub keyframe: bool,
}

impl NvencEncoder {
    /// Create a new NVENC encoder
    pub fn new(config: &CaptureConfig) -> Result<Self> {
        // Initialize FFmpeg
        ffmpeg::init().map_err(|e| NitrogenError::encoder(format!("FFmpeg init failed: {}", e)))?;

        let encoder_name = config.codec.nvenc_encoder();
        info!("Initializing encoder: {}", encoder_name);

        // Find the encoder
        let codec = encoder::find_by_name(encoder_name)
            .ok_or_else(|| NitrogenError::nvenc(format!("Encoder {} not found", encoder_name)))?;

        // Create encoder context
        let mut encoder = codec::context::Context::new_with_codec(codec)
            .encoder()
            .video()
            .map_err(|e| NitrogenError::nvenc(format!("Failed to create encoder context: {}", e)))?;

        // Configure encoder
        let width = config.width();
        let height = config.height();
        let fps = config.fps();
        let bitrate = config.effective_bitrate() as usize * 1000; // kbps to bps

        encoder.set_width(width);
        encoder.set_height(height);
        encoder.set_format(Pixel::NV12); // NVENC prefers NV12
        encoder.set_time_base(Rational::new(1, fps as i32));
        encoder.set_frame_rate(Some(Rational::new(fps as i32, 1)));
        encoder.set_bit_rate(bitrate);
        encoder.set_max_bit_rate(bitrate * 2); // Allow some headroom

        // Set up encoder options
        let mut opts = Dictionary::new();

        // NVENC preset
        opts.set("preset", config.encoder_preset.nvenc_preset());

        // Low latency options
        if config.low_latency {
            opts.set("tune", "ll"); // Low latency tune
            opts.set("zerolatency", "1");
            opts.set("rc", "cbr"); // Constant bitrate for consistent latency
        } else {
            opts.set("rc", "vbr"); // Variable bitrate for quality
        }

        // NVENC-specific options
        opts.set("gpu", "0"); // Use first GPU
        opts.set("surfaces", "8"); // Number of surfaces for async encode

        // Codec-specific options
        match config.codec {
            Codec::H264 => {
                opts.set("profile", "high");
                opts.set("level", "auto");
            }
            Codec::Hevc => {
                opts.set("profile", "main");
            }
            Codec::Av1 => {
                // AV1 specific
            }
        }

        // Open encoder
        let encoder = encoder
            .open_with(opts)
            .map_err(|e| NitrogenError::nvenc(format!("Failed to open encoder: {}", e)))?;

        info!(
            "NVENC encoder opened: {}x{} @ {}fps, {}kbps",
            width,
            height,
            fps,
            bitrate / 1000
        );

        // Create output channel
        let (output_tx, _) = broadcast::channel(16);

        // Create input frame
        let frame = Video::new(Pixel::NV12, width, height);

        Ok(Self {
            encoder,
            scaler: None,
            frame,
            packet: ffmpeg::Packet::empty(),
            output_tx,
            frame_count: 0,
            time_base: Rational::new(1, fps as i32),
        })
    }

    /// Subscribe to encoded packets
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<EncodedPacket>> {
        self.output_tx.subscribe()
    }

    /// Encode a frame
    ///
    /// Takes a raw frame and produces encoded packets.
    pub fn encode(&mut self, input: &Frame) -> Result<()> {
        // Convert frame data to FFmpeg format
        match &input.data {
            FrameData::Memory(data) => {
                self.encode_memory_frame(data, &input.format)?;
            }
            FrameData::DmaBuf { fd: _, offset: _, modifier: _ } => {
                // For DMA-BUF, we would need to use CUDA/NVDEC for zero-copy
                // For now, this is not implemented
                return Err(NitrogenError::Unsupported(
                    "DMA-BUF encoding not yet implemented".into(),
                ));
            }
        }

        Ok(())
    }

    /// Encode a frame from memory
    fn encode_memory_frame(&mut self, data: &[u8], format: &FrameFormat) -> Result<()> {
        // Ensure scaler is set up for format conversion
        self.ensure_scaler(format)?;

        // Copy input data to a source frame
        let mut src_frame = Video::new(
            pixel_format_from_fourcc(format.fourcc),
            format.width,
            format.height,
        );

        // Copy data to source frame
        // This assumes the input is tightly packed
        let plane = src_frame.data_mut(0);
        let copy_len = plane.len().min(data.len());
        plane[..copy_len].copy_from_slice(&data[..copy_len]);

        // Scale/convert to encoder format
        if let Some(ref mut scaler) = self.scaler {
            scaler
                .run(&src_frame, &mut self.frame)
                .map_err(|e| NitrogenError::encoder(format!("Scaling failed: {}", e)))?;
        } else {
            // Direct copy if formats match (unlikely)
            return Err(NitrogenError::encoder("No scaler configured"));
        }

        // Set frame PTS
        self.frame
            .set_pts(Some(self.frame_count as i64));
        self.frame_count += 1;

        // Send to encoder
        self.encoder
            .send_frame(&self.frame)
            .map_err(|e| NitrogenError::nvenc(format!("Failed to send frame: {}", e)))?;

        // Receive encoded packets
        self.receive_packets()?;

        Ok(())
    }

    /// Ensure the scaler is configured for the input format
    fn ensure_scaler(&mut self, format: &FrameFormat) -> Result<()> {
        let src_format = pixel_format_from_fourcc(format.fourcc);
        let dst_format = Pixel::NV12;

        if self.scaler.is_none()
            || self.scaler.as_ref().map(|s| s.input().format) != Some(src_format)
        {
            debug!(
                "Creating scaler: {:?} {}x{} -> {:?} {}x{}",
                src_format,
                format.width,
                format.height,
                dst_format,
                self.encoder.width(),
                self.encoder.height()
            );

            let scaler = scaling::Context::get(
                src_format,
                format.width,
                format.height,
                dst_format,
                self.encoder.width(),
                self.encoder.height(),
                Flags::BILINEAR,
            )
            .map_err(|e| NitrogenError::encoder(format!("Failed to create scaler: {}", e)))?;

            self.scaler = Some(scaler);
        }

        Ok(())
    }

    /// Receive encoded packets from the encoder
    fn receive_packets(&mut self) -> Result<()> {
        loop {
            match self.encoder.receive_packet(&mut self.packet) {
                Ok(()) => {
                    let packet = EncodedPacket {
                        data: self.packet.data().map(|d| d.to_vec()).unwrap_or_default(),
                        pts: self.packet.pts().unwrap_or(0),
                        dts: self.packet.dts().unwrap_or(0),
                        keyframe: self.packet.is_key(),
                    };

                    trace!(
                        "Encoded packet: pts={}, size={}, keyframe={}",
                        packet.pts,
                        packet.data.len(),
                        packet.keyframe
                    );

                    let _ = self.output_tx.send(Arc::new(packet));
                }
                Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::error::EAGAIN => {
                    // Need more input
                    break;
                }
                Err(e) => {
                    return Err(NitrogenError::nvenc(format!(
                        "Failed to receive packet: {}",
                        e
                    )));
                }
            }
        }

        Ok(())
    }

    /// Flush remaining packets
    pub fn flush(&mut self) -> Result<()> {
        self.encoder
            .send_eof()
            .map_err(|e| NitrogenError::nvenc(format!("Failed to send EOF: {}", e)))?;
        self.receive_packets()
    }
}

/// Convert DRM fourcc to FFmpeg pixel format
fn pixel_format_from_fourcc(fourcc: u32) -> Pixel {
    match fourcc {
        0x34325258 => Pixel::BGRA,   // XR24 (XRGB8888) - actually BGRX
        0x34324258 => Pixel::BGRA,   // XB24
        0x34325241 => Pixel::RGBA,   // AR24
        0x34324152 => Pixel::ARGB,   // RA24
        0x56595559 => Pixel::YUYV422, // YUYV
        0x32315559 => Pixel::NV12,   // NV12
        _ => {
            warn!("Unknown fourcc: 0x{:08x}, defaulting to BGRA", fourcc);
            Pixel::BGRA
        }
    }
}

/// Check if NVENC is available
pub fn check_nvenc_available() -> bool {
    ffmpeg::init().ok();
    encoder::find_by_name("h264_nvenc").is_some()
}

/// Check if a specific encoder is available
pub fn encoder_available(codec: Codec) -> bool {
    ffmpeg::init().ok();
    encoder::find_by_name(codec.nvenc_encoder()).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_format_conversion() {
        assert_eq!(pixel_format_from_fourcc(0x34325258), Pixel::BGRA);
    }
}

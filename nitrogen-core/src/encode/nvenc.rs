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

use crate::config::{CaptureConfig, Codec};
use crate::error::{NitrogenError, Result};
use crate::types::{Frame, FrameData, FrameFormat};

/// NVENC hardware encoder
pub struct NvencEncoder {
    /// FFmpeg encoder context
    encoder: encoder::Video,
    /// Scaler for format conversion if needed
    scaler: Option<scaling::Context>,
    /// Intermediate frame for input (before scaling)
    src_frame: Option<Video>,
    /// Input frame buffer (after scaling, for encoder)
    dst_frame: Video,
    /// Output packet buffer
    packet: ffmpeg::Packet,
    /// Encoded data sender
    output_tx: broadcast::Sender<Arc<EncodedPacket>>,
    /// Frame counter
    frame_count: u64,
    /// Output width
    output_width: u32,
    /// Output height
    output_height: u32,
    /// Output pixel format (NV12 or P010LE for 10-bit)
    output_format: Pixel,
    /// Last input format (for scaler cache)
    last_input_format: Option<(u32, u32, Pixel)>,
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
            .map_err(|e| {
                NitrogenError::nvenc(format!("Failed to create encoder context: {}", e))
            })?;

        // Configure encoder
        let width = config.width();
        let height = config.height();
        let fps = config.fps();
        let bitrate = config.effective_bitrate() as usize * 1000; // kbps to bps

        // Select pixel format: P010LE for 10-bit AV1, NV12 for everything else
        let use_10bit = config.codec == Codec::Av1 && config.av1.ten_bit;
        let pixel_format = if use_10bit {
            Pixel::P010LE // 10-bit 4:2:0 planar
        } else {
            Pixel::NV12 // 8-bit 4:2:0 semi-planar
        };

        encoder.set_width(width);
        encoder.set_height(height);
        encoder.set_format(pixel_format);
        encoder.set_time_base(Rational::new(1, fps as i32));
        encoder.set_frame_rate(Some(Rational::new(fps as i32, 1)));
        encoder.set_bit_rate(bitrate);
        encoder.set_max_bit_rate(bitrate + bitrate / 2); // 1.5x headroom

        if use_10bit {
            info!("Using 10-bit encoding (P010LE) for AV1 main10 profile");
        }

        // Set up encoder options
        let mut opts = Dictionary::new();

        // NVENC preset
        opts.set("preset", config.encoder_preset.nvenc_preset());

        // Low latency options
        if config.low_latency {
            opts.set("tune", "ll"); // Low latency tune
            opts.set("zerolatency", "1");
            opts.set("delay", "0");
            opts.set("rc", "cbr"); // Constant bitrate for consistent latency
        } else {
            opts.set("rc", "vbr"); // Variable bitrate for quality
        }

        // NVENC-specific options
        opts.set("gpu", &config.gpu.to_string());
        opts.set("surfaces", "8"); // Number of surfaces for async encode

        // Codec-specific options
        match config.codec {
            Codec::H264 => {
                opts.set("profile", "high");
                opts.set("level", "auto");
                // B-frames can add latency, disable for low-latency
                if config.low_latency {
                    opts.set("bf", "0");
                }
            }
            Codec::Hevc => {
                opts.set("profile", "main");
                if config.low_latency {
                    opts.set("bf", "0");
                }
            }
            Codec::Av1 => {
                // AV1 specific options from Av1Config
                // Supports RTX 40 (Ada) and RTX 50 (Blackwell) features
                let av1 = &config.av1;

                // Tier selection (main for compatibility, high for RTX 40+)
                opts.set("tier", av1.tier.ffmpeg_value());

                // Profile: main or main10 for 10-bit
                if av1.ten_bit {
                    opts.set("profile", "main10");
                } else {
                    opts.set("profile", "main");
                }

                // GOP length (keyframe interval)
                let gop = av1.resolved_gop(fps);
                opts.set("g", &gop.to_string());

                // Tuning mode (hq, uhq for RTX 50, ll, ull)
                // UHQ provides ~5% better compression on Blackwell
                if !config.low_latency {
                    opts.set("tune", av1.tune.ffmpeg_value());
                } else {
                    opts.set("tune", "ll"); // Force low-latency tune
                }

                // Lookahead for better quality (if not low-latency)
                if av1.lookahead && !config.low_latency {
                    // RTX 50 supports up to 250 frames lookahead
                    let depth = av1.lookahead_depth.min(250).max(1);
                    opts.set("rc-lookahead", &depth.to_string());
                }

                // Spatial AQ (adaptive quantization for better quality at same bitrate)
                if av1.spatial_aq {
                    opts.set("spatial_aq", "1");
                }

                // Temporal AQ - RTX 50 series feature (~4-5% efficiency gain)
                if av1.temporal_aq {
                    opts.set("temporal_aq", "1");
                }

                // Multipass encoding for better quality
                if let Some(multipass) = av1.multipass.ffmpeg_value() {
                    opts.set("multipass", multipass);
                }

                // B-frame reference mode (RTX 50 series)
                if av1.b_ref_mode {
                    opts.set("b_ref_mode", "middle");
                }

                // AV1 NVENC doesn't support B-frames in traditional sense
                opts.set("bf", "0");

                // Log if using Blackwell features
                if av1.uses_blackwell_features() {
                    info!("Using RTX 50 series (Blackwell) AV1 features");
                }
            }
        }

        // Open encoder
        let encoder = encoder
            .open_with(opts)
            .map_err(|e| NitrogenError::nvenc(format!("Failed to open encoder: {}", e)))?;

        info!(
            "NVENC encoder opened: {}x{} @ {}fps, {}kbps, codec={}",
            width,
            height,
            fps,
            bitrate / 1000,
            config.codec
        );

        // Create output channel
        let (output_tx, _) = broadcast::channel(16);

        // Create destination frame (matching encoder pixel format)
        let dst_frame = Video::new(pixel_format, width, height);

        Ok(Self {
            encoder,
            scaler: None,
            src_frame: None,
            dst_frame,
            packet: ffmpeg::Packet::empty(),
            output_tx,
            frame_count: 0,
            output_width: width,
            output_height: height,
            output_format: pixel_format,
            last_input_format: None,
        })
    }

    /// Subscribe to encoded packets
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<EncodedPacket>> {
        self.output_tx.subscribe()
    }

    /// Get the output resolution
    pub fn output_size(&self) -> (u32, u32) {
        (self.output_width, self.output_height)
    }

    /// Get the frame count
    pub fn frame_count(&self) -> u64 {
        self.frame_count
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
            FrameData::DmaBuf {
                fd: _,
                offset: _,
                modifier: _,
            } => {
                // For DMA-BUF, we would need to use CUDA/NVDEC for zero-copy
                // This requires nvenc CUDA device setup which is more complex
                return Err(NitrogenError::Unsupported(
                    "DMA-BUF encoding not yet implemented - use memory frames".into(),
                ));
            }
        }

        Ok(())
    }

    /// Encode a frame from memory
    fn encode_memory_frame(&mut self, data: &[u8], format: &FrameFormat) -> Result<()> {
        let src_pixel_format = pixel_format_from_fourcc(format.fourcc);

        // Ensure scaler is set up for format conversion
        self.ensure_scaler(format.width, format.height, src_pixel_format)?;

        // Ensure source frame exists with correct format
        self.ensure_src_frame(format.width, format.height, src_pixel_format);

        // Copy input data to source frame with proper stride handling
        if let Some(ref mut src_frame) = self.src_frame {
            copy_frame_data(src_frame, data, format)?;
        }

        // Scale/convert to encoder format (NV12)
        // We need to do this in a block to satisfy the borrow checker
        {
            let src_frame = self
                .src_frame
                .as_ref()
                .ok_or_else(|| NitrogenError::encoder("Source frame not initialized"))?;
            let scaler = self
                .scaler
                .as_mut()
                .ok_or_else(|| NitrogenError::encoder("No scaler configured"))?;

            scaler
                .run(src_frame, &mut self.dst_frame)
                .map_err(|e| NitrogenError::encoder(format!("Scaling failed: {}", e)))?;
        }

        // Set frame PTS
        self.dst_frame.set_pts(Some(self.frame_count as i64));
        self.frame_count += 1;

        // Send to encoder
        self.encoder
            .send_frame(&self.dst_frame)
            .map_err(|e| NitrogenError::nvenc(format!("Failed to send frame: {}", e)))?;

        // Receive encoded packets
        self.receive_packets()?;

        Ok(())
    }

    /// Ensure source frame exists with correct format
    fn ensure_src_frame(&mut self, width: u32, height: u32, format: Pixel) {
        let needs_new = match &self.src_frame {
            Some(frame) => {
                frame.width() != width || frame.height() != height || frame.format() != format
            }
            None => true,
        };

        if needs_new {
            debug!("Creating source frame: {:?} {}x{}", format, width, height);
            self.src_frame = Some(Video::new(format, width, height));
        }
    }

    /// Ensure the scaler is configured for the input format
    fn ensure_scaler(&mut self, width: u32, height: u32, src_format: Pixel) -> Result<()> {
        let dst_format = self.output_format;
        let current_input = (width, height, src_format);

        // Check if we need to recreate the scaler
        let needs_new_scaler = match self.last_input_format {
            Some(last) => last != current_input,
            None => true,
        };

        if needs_new_scaler {
            debug!(
                "Creating scaler: {:?} {}x{} -> {:?} {}x{}",
                src_format, width, height, dst_format, self.output_width, self.output_height
            );

            let scaler = scaling::Context::get(
                src_format,
                width,
                height,
                dst_format,
                self.output_width,
                self.output_height,
                Flags::BILINEAR,
            )
            .map_err(|e| NitrogenError::encoder(format!("Failed to create scaler: {}", e)))?;

            self.scaler = Some(scaler);
            self.last_input_format = Some(current_input);
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

                    // Send packet (ignore error if no receivers)
                    let _ = self.output_tx.send(Arc::new(packet));
                }
                Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::error::EAGAIN => {
                    // Need more input frames
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

    /// Flush remaining packets from the encoder
    pub fn flush(&mut self) -> Result<()> {
        debug!("Flushing encoder ({} frames encoded)", self.frame_count);
        self.encoder
            .send_eof()
            .map_err(|e| NitrogenError::nvenc(format!("Failed to send EOF: {}", e)))?;
        self.receive_packets()
    }
}

/// Copy frame data to FFmpeg Video frame with stride handling
fn copy_frame_data(frame: &mut Video, data: &[u8], format: &FrameFormat) -> Result<()> {
    let pixel_format = frame.format();
    let src_stride = format.stride as usize;
    let width = format.width as usize;
    let height = format.height as usize;

    // For packed formats (BGRA, RGBA, etc.), we have one plane
    match pixel_format {
        Pixel::BGRA | Pixel::RGBA | Pixel::ARGB | Pixel::RGB24 | Pixel::BGR24 => {
            let dst_stride = frame.stride(0);
            let plane = frame.data_mut(0);
            let bytes_per_pixel = match pixel_format {
                Pixel::RGB24 | Pixel::BGR24 => 3,
                _ => 4,
            };
            let row_bytes = width * bytes_per_pixel;

            // Copy row by row handling different strides
            for y in 0..height {
                let src_offset = y * src_stride;
                let dst_offset = y * dst_stride;

                let src_end = (src_offset + row_bytes).min(data.len());
                let dst_end = (dst_offset + row_bytes).min(plane.len());

                if src_end > src_offset && dst_end > dst_offset {
                    let copy_len = (src_end - src_offset).min(dst_end - dst_offset);
                    plane[dst_offset..dst_offset + copy_len]
                        .copy_from_slice(&data[src_offset..src_offset + copy_len]);
                }
            }
        }
        _ => {
            // For other formats, try a simple copy
            let plane = frame.data_mut(0);
            let copy_len = plane.len().min(data.len());
            plane[..copy_len].copy_from_slice(&data[..copy_len]);
        }
    }

    Ok(())
}

/// Convert DRM fourcc to FFmpeg pixel format
fn pixel_format_from_fourcc(fourcc: u32) -> Pixel {
    match fourcc {
        // XRGB8888 / BGRX - common from Wayland compositors
        0x34325258 => Pixel::BGRA, // XR24 (DRM_FORMAT_XRGB8888)
        0x34325842 => Pixel::BGRA, // BX24 (DRM_FORMAT_BGRX8888)

        // ARGB8888 / BGRA
        0x34325241 => Pixel::BGRA, // AR24 (DRM_FORMAT_ARGB8888)
        0x34324142 => Pixel::BGRA, // AB24 (DRM_FORMAT_ABGR8888)

        // RGBA / RGBX
        0x34324241 => Pixel::RGBA, // BA24 (DRM_FORMAT_RGBA8888)
        0x34324258 => Pixel::RGBA, // BX24

        // RGB formats
        0x20424752 => Pixel::RGB24, // RGB
        0x20524742 => Pixel::BGR24, // BGR

        // YUV formats
        0x56595559 => Pixel::YUYV422, // YUYV
        0x3231564E => Pixel::NV12,    // NV12

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

/// Get list of available NVENC encoders
pub fn list_available_encoders() -> Vec<(Codec, &'static str)> {
    ffmpeg::init().ok();

    let mut available = Vec::new();

    for codec in [Codec::H264, Codec::Hevc, Codec::Av1] {
        if encoder_available(codec) {
            available.push((codec, codec.nvenc_encoder()));
        }
    }

    available
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_format_conversion() {
        assert_eq!(pixel_format_from_fourcc(0x34325258), Pixel::BGRA);
        assert_eq!(pixel_format_from_fourcc(0x3231564E), Pixel::NV12);
    }

    #[test]
    fn test_nvenc_detection() {
        // This test just checks the function doesn't panic
        let _ = check_nvenc_available();
    }
}

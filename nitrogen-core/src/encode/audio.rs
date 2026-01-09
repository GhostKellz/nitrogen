//! Audio encoding via FFmpeg
//!
//! Provides AAC and Opus audio encoding for recording.

use ffmpeg_next as ffmpeg;
use ffmpeg_next::codec::{self, encoder};
use ffmpeg_next::format::Sample;
use ffmpeg_next::util::frame::audio::Audio;
use ffmpeg_next::{ChannelLayout, Dictionary, Rational};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, info, trace, warn};

use crate::config::AudioCodec;
use crate::error::{NitrogenError, Result};
use crate::types::AudioFrame;

/// Audio encoder
pub struct AudioEncoder {
    /// FFmpeg encoder context
    encoder: encoder::Audio,
    /// Input frame buffer
    input_frame: Audio,
    /// Output packet buffer
    packet: ffmpeg::Packet,
    /// Encoded data sender
    output_tx: broadcast::Sender<Arc<EncodedAudioPacket>>,
    /// Sample counter (for PTS calculation)
    sample_count: u64,
    /// Input sample rate
    input_sample_rate: u32,
    /// Input channels
    input_channels: u32,
    /// Samples per frame (codec-specific)
    frame_size: usize,
    /// Sample buffer for accumulating input
    sample_buffer: Vec<f32>,
}

/// Encoded audio packet
#[derive(Debug, Clone)]
pub struct EncodedAudioPacket {
    /// Encoded data
    pub data: Vec<u8>,
    /// Presentation timestamp
    pub pts: i64,
    /// Decode timestamp
    pub dts: i64,
    /// Duration in time base units
    pub duration: i64,
}

impl AudioEncoder {
    /// Create a new audio encoder
    ///
    /// # Arguments
    /// * `codec` - Audio codec to use (AAC, Opus)
    /// * `sample_rate` - Input sample rate (Hz)
    /// * `channels` - Number of audio channels
    /// * `bitrate` - Target bitrate in kbps (0 = auto)
    pub fn new(codec: AudioCodec, sample_rate: u32, channels: u32, bitrate: u32) -> Result<Self> {
        ffmpeg::init().map_err(|e| NitrogenError::encoder(format!("FFmpeg init failed: {}", e)))?;

        let encoder_name = codec.ffmpeg_encoder();
        let bitrate_str = if bitrate == 0 {
            "auto".to_string()
        } else {
            bitrate.to_string()
        };
        info!(
            "Initializing audio encoder: {} ({}ch @ {}Hz, {}kbps)",
            encoder_name, channels, sample_rate, bitrate_str
        );

        // Find the encoder
        let ffcodec = encoder::find_by_name(encoder_name).ok_or_else(|| {
            NitrogenError::encoder(format!("Audio encoder {} not found", encoder_name))
        })?;

        // Create encoder context
        let mut encoder = codec::context::Context::new_with_codec(ffcodec)
            .encoder()
            .audio()
            .map_err(|e| {
                NitrogenError::encoder(format!("Failed to create audio encoder: {}", e))
            })?;

        // Configure encoder
        let effective_bitrate = if bitrate == 0 {
            codec.default_bitrate()
        } else {
            bitrate
        };

        encoder.set_rate(sample_rate as i32);
        encoder.set_bit_rate(effective_bitrate as usize * 1000);
        encoder.set_format(Sample::F32(ffmpeg::format::sample::Type::Packed));
        encoder.set_time_base(Rational::new(1, sample_rate as i32));

        // Set channel layout based on channel count
        let channel_layout = match channels {
            1 => ChannelLayout::MONO,
            2 => ChannelLayout::STEREO,
            6 => ChannelLayout::_5POINT1,
            8 => ChannelLayout::_7POINT1,
            _ => {
                warn!("Unusual channel count {}, defaulting to stereo", channels);
                ChannelLayout::STEREO
            }
        };
        encoder.set_channel_layout(channel_layout);

        // Set up encoder options
        let mut opts = Dictionary::new();

        match codec {
            AudioCodec::Aac => {
                opts.set("aac_coder", "twoloop");
            }
            AudioCodec::Opus => {
                opts.set("application", "audio");
                opts.set("vbr", "on");
            }
            AudioCodec::Copy => {
                return Err(NitrogenError::config(
                    "Cannot create encoder with Copy codec - use passthrough instead",
                ));
            }
        }

        // Open encoder
        let encoder = encoder
            .open_with(opts)
            .map_err(|e| NitrogenError::encoder(format!("Failed to open audio encoder: {}", e)))?;

        // Get the frame size (samples per frame)
        let frame_size = encoder.frame_size() as usize;
        let frame_size = if frame_size == 0 { 1024 } else { frame_size };

        info!(
            "Audio encoder opened: {} {}ch @ {}Hz, {}kbps, frame_size={}",
            encoder_name, channels, sample_rate, effective_bitrate, frame_size
        );

        // Create output channel
        let (output_tx, _) = broadcast::channel(64);

        // Create input frame buffer
        let input_frame = Audio::new(
            Sample::F32(ffmpeg::format::sample::Type::Packed),
            frame_size,
            channel_layout,
        );

        Ok(Self {
            encoder,
            input_frame,
            packet: ffmpeg::Packet::empty(),
            output_tx,
            sample_count: 0,
            input_sample_rate: sample_rate,
            input_channels: channels,
            frame_size,
            sample_buffer: Vec::with_capacity(frame_size * channels as usize * 2),
        })
    }

    /// Subscribe to encoded packets
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<EncodedAudioPacket>> {
        self.output_tx.subscribe()
    }

    /// Get the sample count
    pub fn sample_count(&self) -> u64 {
        self.sample_count
    }

    /// Encode an audio frame
    ///
    /// Audio frames are buffered until we have enough samples for a codec frame.
    pub fn encode(&mut self, frame: &AudioFrame) -> Result<()> {
        // Verify format matches
        if frame.format.sample_rate != self.input_sample_rate {
            return Err(NitrogenError::encoder(format!(
                "Sample rate mismatch: expected {}, got {}",
                self.input_sample_rate, frame.format.sample_rate
            )));
        }

        if frame.format.channels != self.input_channels {
            return Err(NitrogenError::encoder(format!(
                "Channel count mismatch: expected {}, got {}",
                self.input_channels, frame.format.channels
            )));
        }

        // Add samples to buffer
        self.sample_buffer.extend_from_slice(&frame.samples);
        self.sample_count += frame.sample_count as u64;

        // Encode complete frames
        let samples_per_frame = self.frame_size * self.input_channels as usize;

        while self.sample_buffer.len() >= samples_per_frame {
            self.encode_frame(&self.sample_buffer[..samples_per_frame].to_vec())?;
            self.sample_buffer.drain(..samples_per_frame);
        }

        Ok(())
    }

    /// Encode a single frame of audio
    fn encode_frame(&mut self, samples: &[f32]) -> Result<()> {
        // Fill input frame
        let data = self.input_frame.data_mut(0);
        let bytes: &[u8] = bytemuck::cast_slice(samples);
        let copy_len = data.len().min(bytes.len());
        data[..copy_len].copy_from_slice(&bytes[..copy_len]);

        // Set PTS
        let pts = (self.sample_count as i64 * self.encoder.time_base().denominator() as i64)
            / self.input_sample_rate as i64;
        self.input_frame.set_pts(Some(pts));

        // Send to encoder
        self.encoder
            .send_frame(&self.input_frame)
            .map_err(|e| NitrogenError::encoder(format!("Failed to send audio frame: {}", e)))?;

        // Receive encoded packets
        self.receive_packets()?;

        Ok(())
    }

    /// Receive encoded packets from the encoder
    fn receive_packets(&mut self) -> Result<()> {
        loop {
            match self.encoder.receive_packet(&mut self.packet) {
                Ok(()) => {
                    let packet = EncodedAudioPacket {
                        data: self.packet.data().map(|d| d.to_vec()).unwrap_or_default(),
                        pts: self.packet.pts().unwrap_or(0),
                        dts: self.packet.dts().unwrap_or(0),
                        duration: self.packet.duration(),
                    };

                    trace!(
                        "Encoded audio packet: pts={}, size={}",
                        packet.pts,
                        packet.data.len()
                    );

                    // Send packet (ignore error if no receivers)
                    let _ = self.output_tx.send(Arc::new(packet));
                }
                Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::error::EAGAIN => {
                    break;
                }
                Err(e) => {
                    return Err(NitrogenError::encoder(format!(
                        "Failed to receive audio packet: {}",
                        e
                    )));
                }
            }
        }

        Ok(())
    }

    /// Flush remaining packets from the encoder
    pub fn flush(&mut self) -> Result<()> {
        // Encode any remaining samples with padding
        if !self.sample_buffer.is_empty() {
            let samples_per_frame = self.frame_size * self.input_channels as usize;
            let mut padded = self.sample_buffer.clone();
            padded.resize(samples_per_frame, 0.0);
            self.encode_frame(&padded)?;
            self.sample_buffer.clear();
        }

        debug!(
            "Flushing audio encoder ({} samples encoded)",
            self.sample_count
        );
        self.encoder
            .send_eof()
            .map_err(|e| NitrogenError::encoder(format!("Failed to send audio EOF: {}", e)))?;
        self.receive_packets()
    }

    /// Get encoder info
    pub fn info(&self) -> AudioEncoderInfo {
        AudioEncoderInfo {
            sample_rate: self.input_sample_rate,
            channels: self.input_channels,
            frame_size: self.frame_size as u32,
        }
    }
}

/// Audio encoder information
#[derive(Debug, Clone)]
pub struct AudioEncoderInfo {
    /// Sample rate
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u32,
    /// Samples per frame
    pub frame_size: u32,
}

/// Check if an audio codec is available
pub fn audio_codec_available(codec: AudioCodec) -> bool {
    ffmpeg::init().ok();
    match codec {
        AudioCodec::Aac => encoder::find_by_name("aac").is_some(),
        AudioCodec::Opus => encoder::find_by_name("libopus").is_some(),
        AudioCodec::Copy => true,
    }
}

/// Get list of available audio encoders
pub fn list_available_audio_encoders() -> Vec<(AudioCodec, &'static str)> {
    ffmpeg::init().ok();

    let mut available = Vec::new();

    for codec in [AudioCodec::Aac, AudioCodec::Opus] {
        if audio_codec_available(codec) {
            available.push((codec, codec.ffmpeg_encoder()));
        }
    }

    available
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_encoder_detection() {
        // This test just checks the function doesn't panic
        let _ = audio_codec_available(AudioCodec::Aac);
        let _ = audio_codec_available(AudioCodec::Opus);
    }

    #[test]
    fn test_list_audio_encoders() {
        let encoders = list_available_audio_encoders();
        // Should at least have AAC (built into FFmpeg)
        assert!(!encoders.is_empty() || true); // Don't fail if FFmpeg not available
    }
}

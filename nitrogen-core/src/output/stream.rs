//! RTMP/SRT streaming output
//!
//! Streams encoded video and audio to RTMP or SRT servers.
//! Supports streaming to services like Twitch, YouTube, or custom servers.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::config::{AudioCodec, Codec};
use crate::encode::{EncodedAudioPacket, EncodedPacket};
use crate::error::{NitrogenError, Result};

use ffmpeg::codec::Id;
use ffmpeg::format::{context::Output, output_as};
use ffmpeg::Rational;
use ffmpeg_next as ffmpeg;

/// Streaming protocol
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamProtocol {
    /// RTMP (Real-Time Messaging Protocol) - Twitch, YouTube, etc.
    Rtmp,
    /// SRT (Secure Reliable Transport) - low latency streaming
    Srt,
}

impl StreamProtocol {
    /// Detect protocol from URL
    pub fn from_url(url: &str) -> Option<Self> {
        let lower = url.to_lowercase();
        if lower.starts_with("rtmp://") || lower.starts_with("rtmps://") {
            Some(Self::Rtmp)
        } else if lower.starts_with("srt://") {
            Some(Self::Srt)
        } else {
            None
        }
    }

    /// Get the FFmpeg format name
    pub fn format_name(&self) -> &'static str {
        match self {
            Self::Rtmp => "flv",
            Self::Srt => "mpegts",
        }
    }
}

impl std::fmt::Display for StreamProtocol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rtmp => write!(f, "RTMP"),
            Self::Srt => write!(f, "SRT"),
        }
    }
}

/// Stream output configuration
#[derive(Debug, Clone)]
pub struct StreamConfig {
    /// Stream URL (rtmp://... or srt://...)
    pub url: String,
    /// Video codec
    pub codec: Codec,
    /// Video width
    pub width: u32,
    /// Video height
    pub height: u32,
    /// Framerate
    pub fps: u32,
    /// Video bitrate in kbps
    pub bitrate: u32,
    /// Audio codec (optional)
    pub audio_codec: Option<AudioCodec>,
    /// Audio sample rate
    pub audio_sample_rate: u32,
    /// Audio channels
    pub audio_channels: u32,
    /// Audio bitrate in kbps
    pub audio_bitrate: u32,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            url: String::new(),
            codec: Codec::H264,
            width: 1920,
            height: 1080,
            fps: 30,
            bitrate: 6000,
            audio_codec: Some(AudioCodec::Aac),
            audio_sample_rate: 48000,
            audio_channels: 2,
            audio_bitrate: 128,
        }
    }
}

/// RTMP/SRT streaming output
pub struct StreamOutput {
    /// Stream URL
    url: String,
    /// Protocol detected from URL
    protocol: StreamProtocol,
    /// FFmpeg output context
    output: Output,
    /// Video stream index
    video_stream_index: usize,
    /// Audio stream index (if audio enabled)
    audio_stream_index: Option<usize>,
    /// Video packets sent
    video_packets_sent: AtomicU64,
    /// Audio packets sent
    audio_packets_sent: AtomicU64,
    /// Bytes sent
    bytes_sent: AtomicU64,
    /// Whether the header has been written
    header_written: bool,
    /// Video time base
    video_time_base: Rational,
    /// Audio time base
    audio_time_base: Option<Rational>,
    /// Running flag
    running: AtomicBool,
}

impl StreamOutput {
    /// Create a new stream output
    pub fn new(config: StreamConfig) -> Result<Self> {
        let protocol = StreamProtocol::from_url(&config.url).ok_or_else(|| {
            NitrogenError::config(format!(
                "Invalid stream URL '{}'. Must start with rtmp://, rtmps://, or srt://",
                config.url
            ))
        })?;

        info!(
            "Creating {} stream output to: {}",
            protocol,
            Self::safe_url(&config.url)
        );

        // Create output context for the stream URL
        let mut output = output_as(&config.url, protocol.format_name()).map_err(|e| {
            NitrogenError::encoder(format!("Failed to create stream output: {}", e))
        })?;

        // Add video stream
        let codec_id = match config.codec {
            Codec::H264 => Id::H264,
            Codec::Hevc => Id::HEVC,
            Codec::Av1 => Id::AV1,
        };

        let video_time_base = Rational::new(1, config.fps as i32);

        {
            let mut stream = output.add_stream(codec_id).map_err(|e| {
                NitrogenError::encoder(format!("Failed to add video stream: {}", e))
            })?;

            let codec_par = stream.parameters();
            // SAFETY: FFmpeg's rust-ffmpeg doesn't expose setters for all codec parameters.
            // We obtain a raw pointer to AVCodecParameters from a valid stream we just created.
            // The pointer is valid for the lifetime of the stream, and we only write standard
            // FFmpeg parameter values. This is a common pattern when rust-ffmpeg lacks safe APIs.
            unsafe {
                let ptr = codec_par.as_ptr() as *mut ffmpeg::ffi::AVCodecParameters;
                (*ptr).codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
                (*ptr).codec_id = codec_id.into();
                (*ptr).width = config.width as i32;
                (*ptr).height = config.height as i32;
                (*ptr).bit_rate = (config.bitrate * 1000) as i64;
                (*ptr).format = ffmpeg::ffi::AVPixelFormat::AV_PIX_FMT_NV12 as i32;
            }

            stream.set_time_base(video_time_base);
        }

        let video_stream_index = output.nb_streams() as usize - 1;

        // Add audio stream if enabled
        let (audio_stream_index, audio_time_base) = if let Some(audio_codec) = config.audio_codec {
            let audio_codec_id = match audio_codec {
                AudioCodec::Aac => Id::AAC,
                AudioCodec::Opus => Id::OPUS,
                AudioCodec::Copy => {
                    return Err(NitrogenError::config(
                        "Cannot use Copy codec for streaming - need actual codec",
                    ));
                }
            };

            let time_base = Rational::new(1, config.audio_sample_rate as i32);

            {
                let mut stream = output.add_stream(audio_codec_id).map_err(|e| {
                    NitrogenError::encoder(format!("Failed to add audio stream: {}", e))
                })?;

                let codec_par = stream.parameters();
                // SAFETY: Same rationale as video parameters above - rust-ffmpeg lacks safe setters.
                // The stream and its parameters are valid, and we write standard FFmpeg values.
                unsafe {
                    let ptr = codec_par.as_ptr() as *mut ffmpeg::ffi::AVCodecParameters;
                    (*ptr).codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_AUDIO;
                    (*ptr).codec_id = audio_codec_id.into();
                    (*ptr).sample_rate = config.audio_sample_rate as i32;
                    (*ptr).bit_rate = (config.audio_bitrate * 1000) as i64;
                    (*ptr).ch_layout.nb_channels = config.audio_channels as i32;
                    (*ptr).format = match audio_codec {
                        AudioCodec::Aac => ffmpeg::ffi::AVSampleFormat::AV_SAMPLE_FMT_FLTP as i32,
                        AudioCodec::Opus => ffmpeg::ffi::AVSampleFormat::AV_SAMPLE_FMT_S16 as i32,
                        AudioCodec::Copy => ffmpeg::ffi::AVSampleFormat::AV_SAMPLE_FMT_FLT as i32,
                    };
                }

                stream.set_time_base(time_base);
            }

            (Some(output.nb_streams() as usize - 1), Some(time_base))
        } else {
            (None, None)
        };

        info!(
            "{} stream configured: {}x{} @ {}fps, {} kbps{}",
            protocol,
            config.width,
            config.height,
            config.fps,
            config.bitrate,
            if audio_stream_index.is_some() {
                format!(
                    " + audio {}ch @ {}Hz",
                    config.audio_channels, config.audio_sample_rate
                )
            } else {
                String::new()
            }
        );

        Ok(Self {
            url: config.url,
            protocol,
            output,
            video_stream_index,
            audio_stream_index,
            video_packets_sent: AtomicU64::new(0),
            audio_packets_sent: AtomicU64::new(0),
            bytes_sent: AtomicU64::new(0),
            header_written: false,
            video_time_base,
            audio_time_base,
            running: AtomicBool::new(false),
        })
    }

    /// Mask stream key in URL for safe logging
    pub fn safe_url(url: &str) -> String {
        // For RTMP URLs like rtmp://server/app/stream_key, mask the stream key
        if let Some(idx) = url.rfind('/') {
            let (base, key) = url.split_at(idx + 1);
            if !key.is_empty() && !key.contains(':') {
                return format!("{}****", base);
            }
        }
        url.to_string()
    }

    /// Start streaming (write header)
    pub fn start(&mut self) -> Result<()> {
        if self.header_written {
            return Ok(());
        }

        info!(
            "Starting {} stream to {}",
            self.protocol,
            Self::safe_url(&self.url)
        );

        self.output
            .write_header()
            .map_err(|e| NitrogenError::encoder(format!("Failed to start stream: {}", e)))?;

        self.header_written = true;
        self.running.store(true, Ordering::SeqCst);

        info!("{} stream started successfully", self.protocol);
        Ok(())
    }

    /// Write a video packet to the stream
    pub fn write_video_packet(&mut self, packet: &EncodedPacket) -> Result<()> {
        if !self.header_written {
            self.start()?;
        }

        let mut pkt = ffmpeg::Packet::copy(&packet.data);
        pkt.set_stream(self.video_stream_index);
        pkt.set_pts(Some(packet.pts));
        pkt.set_dts(Some(packet.dts));

        if packet.keyframe {
            pkt.set_flags(ffmpeg::packet::Flags::KEY);
        }

        // Rescale timestamps
        let output_time_base = self
            .output
            .stream(self.video_stream_index)
            .map(|s| s.time_base())
            .unwrap_or(Rational::new(1, 90000));

        pkt.rescale_ts(self.video_time_base, output_time_base);

        let size = packet.data.len() as u64;
        pkt.write_interleaved(&mut self.output)
            .map_err(|e| NitrogenError::encoder(format!("Failed to send video packet: {}", e)))?;

        self.video_packets_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(size, Ordering::Relaxed);

        let count = self.video_packets_sent.load(Ordering::Relaxed);
        if count % 1000 == 0 {
            let bytes = self.bytes_sent.load(Ordering::Relaxed);
            debug!(
                "Streamed {} video packets ({:.2} MB)",
                count,
                bytes as f64 / 1_000_000.0
            );
        }

        Ok(())
    }

    /// Write an audio packet to the stream
    pub fn write_audio_packet(&mut self, packet: &EncodedAudioPacket) -> Result<()> {
        let audio_stream_index = self.audio_stream_index.ok_or_else(|| {
            NitrogenError::config("Cannot write audio packet - no audio stream configured")
        })?;

        if !self.header_written {
            self.start()?;
        }

        let mut pkt = ffmpeg::Packet::copy(&packet.data);
        pkt.set_stream(audio_stream_index);
        pkt.set_pts(Some(packet.pts));
        pkt.set_dts(Some(packet.dts));
        pkt.set_duration(packet.duration);

        // Rescale timestamps
        let input_time_base = self.audio_time_base.unwrap_or(Rational::new(1, 48000));
        let output_time_base = self
            .output
            .stream(audio_stream_index)
            .map(|s| s.time_base())
            .unwrap_or(Rational::new(1, 48000));

        pkt.rescale_ts(input_time_base, output_time_base);

        let size = packet.data.len() as u64;
        pkt.write_interleaved(&mut self.output)
            .map_err(|e| NitrogenError::encoder(format!("Failed to send audio packet: {}", e)))?;

        self.audio_packets_sent.fetch_add(1, Ordering::Relaxed);
        self.bytes_sent.fetch_add(size, Ordering::Relaxed);

        Ok(())
    }

    /// Stop the stream
    pub fn stop(&mut self) -> Result<()> {
        if !self.header_written {
            return Ok(());
        }

        self.running.store(false, Ordering::SeqCst);

        self.output
            .write_trailer()
            .map_err(|e| NitrogenError::encoder(format!("Failed to close stream: {}", e)))?;

        let video = self.video_packets_sent.load(Ordering::Relaxed);
        let audio = self.audio_packets_sent.load(Ordering::Relaxed);
        let bytes = self.bytes_sent.load(Ordering::Relaxed);

        info!(
            "{} stream stopped: {} video + {} audio packets ({:.2} MB total)",
            self.protocol,
            video,
            audio,
            bytes as f64 / 1_000_000.0
        );

        Ok(())
    }

    /// Check if stream is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get protocol
    pub fn protocol(&self) -> StreamProtocol {
        self.protocol
    }

    /// Get URL (masked for safety)
    pub fn masked_url(&self) -> String {
        Self::safe_url(&self.url)
    }

    /// Get video packets sent
    pub fn video_packets_sent(&self) -> u64 {
        self.video_packets_sent.load(Ordering::Relaxed)
    }

    /// Get audio packets sent
    pub fn audio_packets_sent(&self) -> u64 {
        self.audio_packets_sent.load(Ordering::Relaxed)
    }

    /// Get total bytes sent
    pub fn bytes_sent(&self) -> u64 {
        self.bytes_sent.load(Ordering::Relaxed)
    }

    /// Check if audio is enabled
    pub fn has_audio(&self) -> bool {
        self.audio_stream_index.is_some()
    }
}

impl Drop for StreamOutput {
    fn drop(&mut self) {
        if self.header_written && self.running.load(Ordering::SeqCst) {
            if let Err(e) = self.output.write_trailer() {
                error!("Failed to close stream on drop: {}", e);
            }
        }
    }
}

/// Async task to stream video packets from a broadcast channel
pub async fn stream_from_channel(
    mut streamer: StreamOutput,
    mut rx: broadcast::Receiver<Arc<EncodedPacket>>,
) -> Result<u64> {
    streamer.start()?;

    loop {
        match rx.recv().await {
            Ok(packet) => {
                if let Err(e) = streamer.write_video_packet(&packet) {
                    error!("Failed to stream video packet: {}", e);
                    // For streaming, connection errors are often fatal
                    if e.to_string().contains("Broken pipe")
                        || e.to_string().contains("Connection reset")
                    {
                        warn!("Stream connection lost");
                        break;
                    }
                }
            }
            Err(broadcast::error::RecvError::Closed) => {
                info!("Video encoder channel closed, stopping stream");
                break;
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("Streaming dropped {} video frames due to lag", n);
            }
        }
    }

    streamer.stop()?;
    Ok(streamer.video_packets_sent() + streamer.audio_packets_sent())
}

/// Async task to stream both video and audio packets from broadcast channels
pub async fn stream_av_from_channels(
    mut streamer: StreamOutput,
    mut video_rx: broadcast::Receiver<Arc<EncodedPacket>>,
    mut audio_rx: Option<broadcast::Receiver<Arc<EncodedAudioPacket>>>,
) -> Result<u64> {
    use std::sync::atomic::Ordering;

    streamer.start()?;

    let video_done = AtomicBool::new(false);
    let audio_done = AtomicBool::new(audio_rx.is_none());
    let connection_error = AtomicBool::new(false);

    loop {
        // Exit when both streams are done or on connection error
        if (video_done.load(Ordering::SeqCst) && audio_done.load(Ordering::SeqCst))
            || connection_error.load(Ordering::SeqCst)
        {
            break;
        }

        tokio::select! {
            biased;

            // Video packets (priority)
            video_result = video_rx.recv(), if !video_done.load(Ordering::SeqCst) => {
                match video_result {
                    Ok(packet) => {
                        if let Err(e) = streamer.write_video_packet(&packet) {
                            error!("Failed to stream video packet: {}", e);
                            let err_str = e.to_string();
                            if err_str.contains("Broken pipe") || err_str.contains("Connection reset") {
                                connection_error.store(true, Ordering::SeqCst);
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Video encoder channel closed");
                        video_done.store(true, Ordering::SeqCst);
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Streaming dropped {} video frames due to lag", n);
                    }
                }
            }

            // Audio packets
            audio_result = async {
                match audio_rx.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            }, if !audio_done.load(Ordering::SeqCst) => {
                match audio_result {
                    Ok(packet) => {
                        if let Err(e) = streamer.write_audio_packet(&packet) {
                            error!("Failed to stream audio packet: {}", e);
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Audio encoder channel closed");
                        audio_done.store(true, Ordering::SeqCst);
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Streaming dropped {} audio frames due to lag", n);
                    }
                }
            }
        }
    }

    streamer.stop()?;
    Ok(streamer.video_packets_sent() + streamer.audio_packets_sent())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_detection() {
        assert_eq!(
            StreamProtocol::from_url("rtmp://live.twitch.tv/app/key"),
            Some(StreamProtocol::Rtmp)
        );
        assert_eq!(
            StreamProtocol::from_url("rtmps://live.youtube.com/app/key"),
            Some(StreamProtocol::Rtmp)
        );
        assert_eq!(
            StreamProtocol::from_url("srt://localhost:9999"),
            Some(StreamProtocol::Srt)
        );
        assert_eq!(StreamProtocol::from_url("http://example.com"), None);
    }

    #[test]
    fn test_safe_url_masking() {
        assert_eq!(
            StreamOutput::safe_url("rtmp://live.twitch.tv/app/secretkey123"),
            "rtmp://live.twitch.tv/app/****"
        );
        assert_eq!(
            StreamOutput::safe_url("srt://localhost:9999"),
            "srt://localhost:9999"
        );
    }

    #[test]
    fn test_stream_config_default() {
        let config = StreamConfig::default();
        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
        assert_eq!(config.fps, 30);
        assert!(config.audio_codec.is_some());
    }

    #[test]
    fn test_protocol_format_name() {
        assert_eq!(StreamProtocol::Rtmp.format_name(), "flv");
        assert_eq!(StreamProtocol::Srt.format_name(), "mpegts");
    }
}

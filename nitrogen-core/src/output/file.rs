//! File recording output
//!
//! Records encoded video and audio to MP4, MKV, or other container formats.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::config::{AudioCodec, Codec};
use crate::encode::{EncodedAudioPacket, EncodedPacket};
use crate::error::{NitrogenError, Result};

use ffmpeg::codec::Id;
use ffmpeg::format::{context::Output, output};
use ffmpeg::Rational;
use ffmpeg_next as ffmpeg;

/// File recorder for saving encoded video and audio to disk
pub struct FileRecorder {
    /// Output path
    path: PathBuf,
    /// FFmpeg output context
    output: Output,
    /// Video stream index
    video_stream_index: usize,
    /// Audio stream index (if audio enabled)
    audio_stream_index: Option<usize>,
    /// Video packets received
    video_packets_written: u64,
    /// Audio packets received
    audio_packets_written: u64,
    /// Whether the header has been written
    header_written: bool,
    /// Video time base
    video_time_base: Rational,
    /// Audio time base (if audio enabled)
    audio_time_base: Option<Rational>,
}

impl FileRecorder {
    /// Create a new file recorder
    pub fn new(
        path: impl Into<PathBuf>,
        codec: Codec,
        width: u32,
        height: u32,
        fps: u32,
        bitrate: u32,
    ) -> Result<Self> {
        let path = path.into();

        info!("Creating file recorder: {:?}", path);

        // Determine container format from extension
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("mp4")
            .to_lowercase();

        // Create output context
        let mut output = output(&path)
            .map_err(|e| NitrogenError::encoder(format!("Failed to create output file: {}", e)))?;

        // Add video stream
        let codec_id = match codec {
            Codec::H264 => Id::H264,
            Codec::Hevc => Id::HEVC,
            Codec::Av1 => Id::AV1,
        };

        let _global_header = output
            .format()
            .flags()
            .contains(ffmpeg::format::Flags::GLOBAL_HEADER);
        let video_time_base = Rational::new(1, fps as i32);

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
                (*ptr).width = width as i32;
                (*ptr).height = height as i32;
                (*ptr).bit_rate = (bitrate * 1000) as i64;
                // Set pixel format to NV12 (common for NVENC output)
                (*ptr).format = ffmpeg::ffi::AVPixelFormat::AV_PIX_FMT_NV12 as i32;
            }

            stream.set_time_base(video_time_base);
        }

        let video_stream_index = output.nb_streams() as usize - 1;

        info!(
            "File recorder configured: {} ({}x{} @ {}fps, {} kbps)",
            extension, width, height, fps, bitrate
        );

        Ok(Self {
            path,
            output,
            video_stream_index,
            audio_stream_index: None,
            video_packets_written: 0,
            audio_packets_written: 0,
            header_written: false,
            video_time_base,
            audio_time_base: None,
        })
    }

    /// Add an audio stream to the recording
    ///
    /// Must be called before `write_header()`.
    pub fn add_audio_stream(
        &mut self,
        audio_codec: AudioCodec,
        sample_rate: u32,
        channels: u32,
        bitrate: u32,
    ) -> Result<()> {
        if self.header_written {
            return Err(NitrogenError::config(
                "Cannot add audio stream after header is written",
            ));
        }

        if self.audio_stream_index.is_some() {
            return Err(NitrogenError::config("Audio stream already added"));
        }

        let codec_id = match audio_codec {
            AudioCodec::Aac => Id::AAC,
            AudioCodec::Opus => Id::OPUS,
            AudioCodec::Copy => {
                return Err(NitrogenError::config(
                    "Cannot use Copy codec for recording - need actual codec",
                ));
            }
        };

        let audio_time_base = Rational::new(1, sample_rate as i32);

        {
            let mut stream = self.output.add_stream(codec_id).map_err(|e| {
                NitrogenError::encoder(format!("Failed to add audio stream: {}", e))
            })?;

            let codec_par = stream.parameters();
            // SAFETY: Same rationale as video parameters above - rust-ffmpeg lacks safe setters.
            // The stream and its parameters are valid, and we write standard FFmpeg values.
            unsafe {
                let ptr = codec_par.as_ptr() as *mut ffmpeg::ffi::AVCodecParameters;
                (*ptr).codec_type = ffmpeg::ffi::AVMediaType::AVMEDIA_TYPE_AUDIO;
                (*ptr).codec_id = codec_id.into();
                (*ptr).sample_rate = sample_rate as i32;
                (*ptr).bit_rate = (bitrate * 1000) as i64;

                // Set channel layout (FFmpeg 7+ uses ch_layout)
                (*ptr).ch_layout.nb_channels = channels as i32;

                // Set sample format based on codec
                (*ptr).format = match audio_codec {
                    AudioCodec::Aac => ffmpeg::ffi::AVSampleFormat::AV_SAMPLE_FMT_FLTP as i32,
                    AudioCodec::Opus => ffmpeg::ffi::AVSampleFormat::AV_SAMPLE_FMT_S16 as i32,
                    AudioCodec::Copy => ffmpeg::ffi::AVSampleFormat::AV_SAMPLE_FMT_FLT as i32,
                };
            }

            stream.set_time_base(audio_time_base);
        }

        self.audio_stream_index = Some(self.output.nb_streams() as usize - 1);
        self.audio_time_base = Some(audio_time_base);

        info!(
            "Audio stream added: {:?} {}ch @ {}Hz, {}kbps",
            audio_codec, channels, sample_rate, bitrate
        );

        Ok(())
    }

    /// Check if audio is enabled
    pub fn has_audio(&self) -> bool {
        self.audio_stream_index.is_some()
    }

    /// Write header to file (must be called before writing packets)
    pub fn write_header(&mut self) -> Result<()> {
        if self.header_written {
            return Ok(());
        }

        self.output
            .write_header()
            .map_err(|e| NitrogenError::encoder(format!("Failed to write file header: {}", e)))?;

        self.header_written = true;
        debug!("File header written");
        Ok(())
    }

    /// Write an encoded video packet to the file
    pub fn write_video_packet(&mut self, packet: &EncodedPacket) -> Result<()> {
        if !self.header_written {
            self.write_header()?;
        }

        let mut pkt = ffmpeg::Packet::copy(&packet.data);
        pkt.set_stream(self.video_stream_index);
        pkt.set_pts(Some(packet.pts));
        pkt.set_dts(Some(packet.dts));

        if packet.keyframe {
            pkt.set_flags(ffmpeg::packet::Flags::KEY);
        }

        // Rescale timestamps to output stream timebase
        let time_base = self
            .output
            .stream(self.video_stream_index)
            .map(|s| s.time_base())
            .unwrap_or(Rational::new(1, 90000));

        pkt.rescale_ts(self.video_time_base, time_base);

        pkt.write_interleaved(&mut self.output)
            .map_err(|e| NitrogenError::encoder(format!("Failed to write video packet: {}", e)))?;

        self.video_packets_written += 1;

        if self.video_packets_written % 1000 == 0 {
            debug!(
                "Written {} video packets to file",
                self.video_packets_written
            );
        }

        Ok(())
    }

    /// Write an encoded audio packet to the file
    pub fn write_audio_packet(&mut self, packet: &EncodedAudioPacket) -> Result<()> {
        let audio_stream_index = self.audio_stream_index.ok_or_else(|| {
            NitrogenError::config("Cannot write audio packet - no audio stream configured")
        })?;

        if !self.header_written {
            self.write_header()?;
        }

        let mut pkt = ffmpeg::Packet::copy(&packet.data);
        pkt.set_stream(audio_stream_index);
        pkt.set_pts(Some(packet.pts));
        pkt.set_dts(Some(packet.dts));
        pkt.set_duration(packet.duration);

        // Rescale timestamps to output stream timebase
        let input_time_base = self.audio_time_base.unwrap_or(Rational::new(1, 48000));
        let output_time_base = self
            .output
            .stream(audio_stream_index)
            .map(|s| s.time_base())
            .unwrap_or(Rational::new(1, 48000));

        pkt.rescale_ts(input_time_base, output_time_base);

        pkt.write_interleaved(&mut self.output)
            .map_err(|e| NitrogenError::encoder(format!("Failed to write audio packet: {}", e)))?;

        self.audio_packets_written += 1;

        if self.audio_packets_written % 1000 == 0 {
            debug!(
                "Written {} audio packets to file",
                self.audio_packets_written
            );
        }

        Ok(())
    }

    /// Write an encoded packet (alias for write_video_packet for backwards compatibility)
    pub fn write_packet(&mut self, packet: &EncodedPacket) -> Result<()> {
        self.write_video_packet(packet)
    }

    /// Finalize the file (write trailer)
    pub fn finalize(&mut self) -> Result<()> {
        if !self.header_written {
            warn!("Finalizing file without writing header");
            return Ok(());
        }

        self.output
            .write_trailer()
            .map_err(|e| NitrogenError::encoder(format!("Failed to write file trailer: {}", e)))?;

        let total_packets = self.video_packets_written + self.audio_packets_written;
        info!(
            "File recording complete: {:?} ({} video + {} audio = {} packets)",
            self.path, self.video_packets_written, self.audio_packets_written, total_packets
        );

        Ok(())
    }

    /// Get the output path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Get the number of video packets written
    pub fn video_packets_written(&self) -> u64 {
        self.video_packets_written
    }

    /// Get the number of audio packets written
    pub fn audio_packets_written(&self) -> u64 {
        self.audio_packets_written
    }

    /// Get the total number of packets written
    pub fn packets_written(&self) -> u64 {
        self.video_packets_written + self.audio_packets_written
    }
}

impl Drop for FileRecorder {
    fn drop(&mut self) {
        if self.header_written {
            if let Err(e) = self.output.write_trailer() {
                error!("Failed to write file trailer on drop: {}", e);
            }
        }
    }
}

/// Async task to record video packets from a broadcast channel
pub async fn record_from_channel(
    mut recorder: FileRecorder,
    mut rx: broadcast::Receiver<Arc<EncodedPacket>>,
) -> Result<u64> {
    recorder.write_header()?;

    loop {
        match rx.recv().await {
            Ok(packet) => {
                if let Err(e) = recorder.write_video_packet(&packet) {
                    error!("Failed to write video packet: {}", e);
                    // Continue trying to write more packets
                }
            }
            Err(broadcast::error::RecvError::Closed) => {
                info!("Video encoder channel closed, finalizing recording");
                break;
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("Recording dropped {} video frames due to lag", n);
            }
        }
    }

    recorder.finalize()?;
    Ok(recorder.packets_written())
}

/// Async task to record both video and audio packets from broadcast channels
///
/// This function uses tokio::select! to interleave video and audio packet writing.
pub async fn record_av_from_channels(
    mut recorder: FileRecorder,
    mut video_rx: broadcast::Receiver<Arc<EncodedPacket>>,
    mut audio_rx: Option<broadcast::Receiver<Arc<EncodedAudioPacket>>>,
) -> Result<u64> {
    use std::sync::atomic::{AtomicBool, Ordering};

    recorder.write_header()?;

    let video_done = AtomicBool::new(false);
    let audio_done = AtomicBool::new(audio_rx.is_none());

    loop {
        // Exit when both streams are done
        if video_done.load(Ordering::SeqCst) && audio_done.load(Ordering::SeqCst) {
            break;
        }

        tokio::select! {
            biased;

            // Video packets
            video_result = video_rx.recv(), if !video_done.load(Ordering::SeqCst) => {
                match video_result {
                    Ok(packet) => {
                        if let Err(e) = recorder.write_video_packet(&packet) {
                            error!("Failed to write video packet: {}", e);
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Video encoder channel closed");
                        video_done.store(true, Ordering::SeqCst);
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Recording dropped {} video frames due to lag", n);
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
                        if let Err(e) = recorder.write_audio_packet(&packet) {
                            error!("Failed to write audio packet: {}", e);
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => {
                        info!("Audio encoder channel closed");
                        audio_done.store(true, Ordering::SeqCst);
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        warn!("Recording dropped {} audio frames due to lag", n);
                    }
                }
            }
        }
    }

    recorder.finalize()?;
    Ok(recorder.packets_written())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_extension_handling() {
        // Just test that we can extract extensions properly
        let path = PathBuf::from("/tmp/test.mp4");
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
        assert_eq!(ext, "mp4");

        let path = PathBuf::from("/tmp/test.mkv");
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("mp4");
        assert_eq!(ext, "mkv");
    }
}

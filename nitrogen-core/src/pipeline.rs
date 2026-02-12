//! Main capture-encode-output pipeline
//!
//! Orchestrates the flow from screen capture to virtual camera output.
//!
//! For Discord streaming, we send raw frames directly to the virtual camera
//! (Discord does its own encoding). For RTMP/SRT streaming (future), we would
//! use ghoststream's encoding pipeline.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, trace, warn};

use crate::capture::portal::{CaptureType, PortalCapture, SessionInfo};
use crate::capture::{AudioCaptureStream, CaptureStream};
use crate::config::{AudioSource, CaptureConfig};
use crate::encode::{AudioEncoder, NvencEncoder, TonemapConfig, Tonemapper};
use crate::error::{NitrogenError, Result};
use crate::output::{
    create_camera, record_av_from_channels, start_signaling_server, stream_av_from_channels,
    FileRecorder, RawOutputSink, StreamConfig, StreamOutput, StreamProtocol, VirtualCamera,
    VirtualMicrophone, WebRTCConfig, WebRTCOutput,
};
use tokio::sync::RwLock;
use crate::overlay::{LatencyOverlay, OverlayConfig};
use crate::performance::{create_metrics, PerformanceMetrics};
use crate::formats::fourcc_to_gs_format;
use crate::types::{AudioFrame, Frame, FrameData, Handle};

// Re-export ghoststream types for frame conversion and scaling
use ghoststream::processing::{convert_colorspace, scale_frame};
use ghoststream::types::{Frame as GsFrame, FrameFormat as GsFrameFormat, Resolution};

/// Main Nitrogen pipeline
///
/// Manages the complete flow from screen capture to virtual camera output.
///
/// For Discord, raw frames are sent directly to the virtual camera.
/// Discord handles its own encoding/compression.
pub struct Pipeline {
    /// Pipeline handle
    handle: Handle,
    /// Configuration
    config: CaptureConfig,
    /// Portal capture
    portal: PortalCapture,
    /// Capture stream (when active)
    capture: Option<CaptureStream>,
    /// Frame receiver (reused across process calls)
    frame_rx: Option<broadcast::Receiver<Arc<Frame>>>,
    /// Virtual camera (when active) - uses ghoststream's RawOutputSink
    camera: Option<VirtualCamera>,
    /// Pipeline state
    state: PipelineState,
    /// Capture resolution (from portal)
    capture_resolution: Option<(u32, u32)>,
    /// Output resolution (from config preset)
    output_resolution: (u32, u32),
    /// Frame counter
    frames_processed: AtomicU64,
    /// Frames dropped (channel lag)
    frames_dropped: AtomicU64,
    /// Frames failed to write to camera
    frames_failed: AtomicU64,
    /// Start time
    start_time: Option<Instant>,
    /// NVENC encoder for file recording
    encoder: Option<NvencEncoder>,
    /// Audio encoder for file recording
    audio_encoder: Option<AudioEncoder>,
    /// Audio capture stream
    audio_capture: Option<AudioCaptureStream>,
    /// Audio frame receiver
    audio_frame_rx: Option<broadcast::Receiver<Arc<AudioFrame>>>,
    /// Virtual microphone for Discord audio passthrough
    virtual_mic: Option<VirtualMicrophone>,
    /// File recorder task handle
    recorder_handle: Option<JoinHandle<Result<u64>>>,
    /// RTMP/SRT stream output task handle
    streamer_handle: Option<JoinHandle<Result<u64>>>,
    /// WebRTC output
    webrtc_output: Option<Arc<RwLock<WebRTCOutput>>>,
    /// WebRTC signaling server task handle
    webrtc_server_handle: Option<JoinHandle<Result<()>>>,
    /// Recording file path
    record_path: Option<PathBuf>,
    /// Audio samples processed
    audio_samples_processed: AtomicU64,
    /// Performance metrics collector
    metrics: Arc<PerformanceMetrics>,
    /// HDR tonemapper
    tonemapper: Tonemapper,
    /// Latency overlay renderer
    overlay: LatencyOverlay,
    /// Last frame time for FPS tracking
    last_frame_time: Option<Instant>,
}

/// Pipeline state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineState {
    /// Pipeline created but not started
    Idle,
    /// Waiting for user to select source via portal
    SelectingSource,
    /// Capture active, waiting for stream to start
    WaitingForStream,
    /// Capture active, processing frames
    Running,
    /// Pipeline stopping
    Stopping,
    /// Pipeline stopped
    Stopped,
    /// Error state
    Error,
}

impl Pipeline {
    /// Create a new pipeline with the given configuration
    pub async fn new(config: CaptureConfig) -> Result<Self> {
        // Validate that at least one output is enabled
        if !config.camera_enabled && config.record_path.is_none() && config.stream_url.is_none() {
            return Err(NitrogenError::config(
                "At least one output must be enabled (virtual camera, file recording, or streaming)".to_string(),
            ));
        }

        let portal = PortalCapture::new().await?;
        let output_resolution = (config.width(), config.height());
        let record_path = config.record_path.clone();

        // Create encoder if file recording is enabled
        let encoder = if record_path.is_some() {
            info!("File recording enabled, initializing NVENC encoder");
            match NvencEncoder::new(&config) {
                Ok(enc) => Some(enc),
                Err(e) => {
                    warn!(
                        "Failed to create NVENC encoder for recording: {}. Recording disabled.",
                        e
                    );
                    None
                }
            }
        } else {
            None
        };

        // Create audio encoder if audio recording is enabled
        let audio_encoder = if record_path.is_some() && config.audio_source != AudioSource::None {
            let audio_bitrate = if config.audio_bitrate == 0 {
                config.audio_codec.default_bitrate()
            } else {
                config.audio_bitrate
            };

            info!(
                "Audio recording enabled, initializing {:?} encoder",
                config.audio_codec
            );
            match AudioEncoder::new(config.audio_codec, 48000, 2, audio_bitrate) {
                Ok(enc) => Some(enc),
                Err(e) => {
                    warn!(
                        "Failed to create audio encoder: {}. Audio recording disabled.",
                        e
                    );
                    None
                }
            }
        } else {
            None
        };

        let has_audio = config.audio_source != AudioSource::None;

        // Create performance metrics
        let metrics = create_metrics();

        // Create tonemapper
        let tonemap_config = TonemapConfig {
            mode: config.hdr_tonemap,
            algorithm: config.hdr_algorithm,
            peak_luminance: config.hdr_peak_luminance,
            sdr_white_point: 100,
        };
        let tonemapper = Tonemapper::new(tonemap_config);

        // Create overlay
        let overlay_config = OverlayConfig {
            enabled: config.overlay_enabled,
            position: config.overlay_position,
            show_capture: true,
            show_encode: true,
            show_fps: true,
            show_bitrate: true,
            show_drops: true,
            font_scale: 1.0,
            background_opacity: 0.7,
        };
        let overlay = LatencyOverlay::new(overlay_config);

        info!(
            "Pipeline configured for {}x{} @ {}fps output{}{}{}",
            output_resolution.0,
            output_resolution.1,
            config.fps(),
            if record_path.is_some() {
                " with file recording"
            } else {
                ""
            },
            if has_audio { " with audio" } else { "" },
            if config.overlay_enabled {
                " with overlay"
            } else {
                ""
            }
        );

        Ok(Self {
            handle: Handle::new(),
            config,
            portal,
            capture: None,
            frame_rx: None,
            camera: None,
            state: PipelineState::Idle,
            capture_resolution: None,
            output_resolution,
            frames_processed: AtomicU64::new(0),
            frames_dropped: AtomicU64::new(0),
            frames_failed: AtomicU64::new(0),
            start_time: None,
            encoder,
            audio_encoder,
            audio_capture: None,
            audio_frame_rx: None,
            virtual_mic: None,
            recorder_handle: None,
            streamer_handle: None,
            webrtc_output: None,
            webrtc_server_handle: None,
            record_path,
            audio_samples_processed: AtomicU64::new(0),
            metrics,
            tonemapper,
            overlay,
            last_frame_time: None,
        })
    }

    /// Get the pipeline handle
    pub fn handle(&self) -> Handle {
        self.handle
    }

    /// Get the current state
    pub fn state(&self) -> PipelineState {
        self.state
    }

    /// Start the pipeline
    ///
    /// This will:
    /// 1. Prompt the user to select a screen/window via the portal
    /// 2. Start the PipeWire capture stream
    /// 3. Create the virtual camera (using ghoststream)
    /// 4. Begin processing frames (raw frames to virtual camera)
    pub async fn start(&mut self) -> Result<SessionInfo> {
        if self.state != PipelineState::Idle {
            return Err(NitrogenError::SessionAlreadyRunning);
        }

        self.state = PipelineState::SelectingSource;
        info!(
            "Starting pipeline {} with config: {:?}",
            self.handle, self.config
        );

        // Determine capture type from config
        let capture_type = match &self.config.source {
            crate::types::CaptureSource::Monitor { .. } => CaptureType::Monitor,
            crate::types::CaptureSource::Window { .. } => CaptureType::Window,
        };

        // Start portal session (will prompt user)
        let session_info = self
            .portal
            .start_session(capture_type, self.config.cursor_mode, false)
            .await?;

        // Store capture resolution
        self.capture_resolution = Some((session_info.width, session_info.height));

        let needs_scaling = session_info.width != self.output_resolution.0
            || session_info.height != self.output_resolution.1;

        info!(
            "Portal session started: {}x{} capture -> {}x{} output{}",
            session_info.width,
            session_info.height,
            self.output_resolution.0,
            self.output_resolution.1,
            if needs_scaling {
                " (scaling enabled)"
            } else {
                ""
            }
        );

        // Get PipeWire fd and start capture stream
        let fd = self.portal.take_pipewire_fd().await?;
        let capture = CaptureStream::new(fd, session_info.node_id)?;

        // Subscribe to frames
        let frame_rx = capture.subscribe();
        self.frame_rx = Some(frame_rx);
        self.capture = Some(capture);

        // Create virtual camera using ghoststream at OUTPUT resolution (if enabled)
        if self.config.camera_enabled {
            let mut camera = create_camera(Some(&self.config.camera_name));

            camera
                .init_raw(
                    Resolution::new(self.output_resolution.0, self.output_resolution.1),
                    GsFrameFormat::Bgra,
                )
                .await
                .map_err(|e| NitrogenError::pipewire(format!("Camera init failed: {}", e)))?;

            self.camera = Some(camera);
            info!("Virtual camera output enabled: {}", self.config.camera_name);
        } else {
            info!("Virtual camera output disabled");
        }
        self.state = PipelineState::WaitingForStream;
        self.start_time = Some(Instant::now());

        // Start audio capture if enabled
        if self.config.audio_source != AudioSource::None {
            match AudioCaptureStream::new(self.config.audio_source) {
                Ok(audio) => {
                    let audio_rx = audio.subscribe();
                    self.audio_frame_rx = Some(audio_rx);

                    // Create virtual microphone for Discord audio passthrough
                    let mic_rx = audio.subscribe();
                    match VirtualMicrophone::new(
                        Some("Nitrogen Audio"),
                        mic_rx,
                        48000, // sample rate
                        2,     // stereo channels
                    ) {
                        Ok(mic) => {
                            self.virtual_mic = Some(mic);
                            info!("Virtual microphone created for Discord audio passthrough");
                        }
                        Err(e) => {
                            warn!(
                                "Failed to create virtual microphone: {}. Discord audio disabled.",
                                e
                            );
                        }
                    }

                    self.audio_capture = Some(audio);
                    info!("Audio capture started: {:?}", self.config.audio_source);
                }
                Err(e) => {
                    warn!("Failed to start audio capture: {}. Audio disabled.", e);
                }
            }
        }

        // Start file recorder if path specified and encoder is available
        if let (Some(encoder), Some(path)) = (&self.encoder, &self.record_path) {
            match FileRecorder::new(
                path,
                self.config.codec,
                self.config.width(),
                self.config.height(),
                self.config.fps(),
                self.config.effective_bitrate(),
            ) {
                Ok(mut recorder) => {
                    // Add audio stream if audio encoder is available
                    let audio_rx = if let Some(ref audio_encoder) = self.audio_encoder {
                        let audio_bitrate = if self.config.audio_bitrate == 0 {
                            self.config.audio_codec.default_bitrate()
                        } else {
                            self.config.audio_bitrate
                        };

                        if let Err(e) = recorder.add_audio_stream(
                            self.config.audio_codec,
                            48000,
                            2,
                            audio_bitrate,
                        ) {
                            warn!("Failed to add audio stream: {}", e);
                            None
                        } else {
                            Some(audio_encoder.subscribe())
                        }
                    } else {
                        None
                    };

                    let video_rx = encoder.subscribe();
                    let handle = tokio::spawn(async move {
                        record_av_from_channels(recorder, video_rx, audio_rx).await
                    });
                    self.recorder_handle = Some(handle);
                    info!("File recording started: {:?}", path);
                }
                Err(e) => {
                    warn!("Failed to create file recorder: {}. Recording disabled.", e);
                }
            }
        }

        // Start RTMP/SRT streaming if URL provided
        if let Some(ref stream_url) = self.config.stream_url {
            // Validate and detect protocol
            if let Some(protocol) = StreamProtocol::from_url(stream_url) {
                // Streaming requires an encoder - create one if we don't have one for recording
                let encoder_for_stream = if self.encoder.is_some() {
                    // Reuse existing encoder's broadcast channel
                    self.encoder.as_ref()
                } else {
                    // Need to create an encoder just for streaming
                    info!("Creating NVENC encoder for streaming");
                    match NvencEncoder::new(&self.config) {
                        Ok(enc) => {
                            self.encoder = Some(enc);
                            self.encoder.as_ref()
                        }
                        Err(e) => {
                            warn!("Failed to create encoder for streaming: {}. Streaming disabled.", e);
                            None
                        }
                    }
                };

                if let Some(encoder) = encoder_for_stream {
                    let stream_config = StreamConfig {
                        url: stream_url.clone(),
                        codec: self.config.codec,
                        width: self.config.width(),
                        height: self.config.height(),
                        fps: self.config.fps(),
                        bitrate: self.config.effective_bitrate(),
                        audio_codec: if self.config.audio_source != AudioSource::None {
                            Some(self.config.audio_codec)
                        } else {
                            None
                        },
                        audio_sample_rate: 48000,
                        audio_channels: 2,
                        audio_bitrate: self.config.effective_audio_bitrate(),
                    };

                    match StreamOutput::new(stream_config) {
                        Ok(streamer) => {
                            let video_rx = encoder.subscribe();
                            let audio_rx = self.audio_encoder.as_ref().map(|e| e.subscribe());

                            let handle = tokio::spawn(async move {
                                stream_av_from_channels(streamer, video_rx, audio_rx).await
                            });
                            self.streamer_handle = Some(handle);

                            // Mask stream key for logging
                            let safe_url = StreamOutput::safe_url(stream_url);
                            info!("{} stream started to {}", protocol, safe_url);
                        }
                        Err(e) => {
                            warn!("Failed to create stream output: {}. Streaming disabled.", e);
                        }
                    }
                }
            } else {
                warn!(
                    "Invalid stream URL: {}. Must be rtmp://, rtmps://, or srt://",
                    stream_url
                );
            }
        }

        // Start WebRTC output if enabled
        if self.config.webrtc_enabled {
            // WebRTC requires an encoder - create one if we don't have one
            if self.encoder.is_none() {
                info!("Creating NVENC encoder for WebRTC");
                match NvencEncoder::new(&self.config) {
                    Ok(enc) => {
                        self.encoder = Some(enc);
                    }
                    Err(e) => {
                        warn!("Failed to create encoder for WebRTC: {}. WebRTC disabled.", e);
                    }
                }
            }

            if self.encoder.is_some() {
                let webrtc_config = WebRTCConfig {
                    ice_servers: vec!["stun:stun.l.google.com:19302".to_string()],
                    video_codec: "h264".to_string(),
                    video_payload_type: 96,
                    audio_enabled: self.config.audio_source != AudioSource::None,
                };

                match WebRTCOutput::new(webrtc_config).await {
                    Ok(mut output) => {
                        if let Err(e) = output.init().await {
                            warn!("Failed to initialize WebRTC: {}", e);
                        } else {
                            let output = Arc::new(RwLock::new(output));
                            self.webrtc_output = Some(output.clone());

                            // Start the signaling server
                            let port = self.config.webrtc_port;
                            let server_handle = tokio::spawn(async move {
                                start_signaling_server(output, port).await
                            });
                            self.webrtc_server_handle = Some(server_handle);

                            info!("WebRTC output enabled - view at http://localhost:{}", port);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to create WebRTC output: {}", e);
                    }
                }
            }
        }

        info!(
            "Pipeline {} waiting for stream - '{}' camera ready at {}x{}",
            self.handle,
            self.config.camera_name,
            self.output_resolution.0,
            self.output_resolution.1
        );

        Ok(session_info)
    }

    /// Process frames in the pipeline
    ///
    /// This should be called in a loop while the pipeline is running.
    /// Returns false when the pipeline should stop.
    pub async fn process(&mut self) -> Result<bool> {
        match self.state {
            PipelineState::Stopped | PipelineState::Error | PipelineState::Idle => {
                return Ok(false);
            }
            PipelineState::Stopping => {
                return Ok(false);
            }
            PipelineState::SelectingSource => {
                // Still waiting for portal selection
                tokio::time::sleep(Duration::from_millis(100)).await;
                return Ok(true);
            }
            PipelineState::WaitingForStream => {
                // Check if capture stream has started
                if let Some(ref capture) = self.capture {
                    if capture.is_running() {
                        self.state = PipelineState::Running;
                        info!("Pipeline {} now streaming", self.handle);
                    } else {
                        // Wait a bit for stream to start
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        return Ok(true);
                    }
                } else {
                    return Ok(false);
                }
            }
            PipelineState::Running => {
                // Continue to process frames below
            }
        }

        // Get frame receiver (create new one if needed due to lag)
        if self.frame_rx.is_none() {
            if let Some(ref capture) = self.capture {
                self.frame_rx = Some(capture.subscribe());
            } else {
                return Ok(false);
            }
        }
        let frame_rx = match self.frame_rx.as_mut() {
            Some(rx) => rx,
            None => return Ok(false), // Should not happen, but handle gracefully
        };

        // Check if capture is still running
        if let Some(ref capture) = self.capture {
            if !capture.is_running() {
                warn!("Capture stream stopped unexpectedly");
                self.state = PipelineState::Error;
                return Ok(false);
            }
        }

        // Try to receive a frame with timeout
        let recv_result = tokio::time::timeout(Duration::from_millis(100), frame_rx.recv()).await;

        match recv_result {
            Ok(Ok(frame)) => {
                // Process the frame
                self.process_frame(&frame).await?;
                Ok(true)
            }
            Ok(Err(broadcast::error::RecvError::Lagged(n))) => {
                let dropped = self.frames_dropped.fetch_add(n, Ordering::Relaxed);
                warn!("Dropped {} frames (total: {})", n, dropped + n);
                // Re-subscribe to get latest frames
                if let Some(ref capture) = self.capture {
                    self.frame_rx = Some(capture.subscribe());
                }
                Ok(true)
            }
            Ok(Err(broadcast::error::RecvError::Closed)) => {
                info!("Capture stream closed");
                self.state = PipelineState::Stopped;
                Ok(false)
            }
            Err(_) => {
                // Timeout - no frame available, but keep running
                trace!("No frame available (timeout)");
                Ok(true)
            }
        }
    }

    /// Process a single frame
    async fn process_frame(&mut self, frame: &Frame) -> Result<()> {
        let frame_start = Instant::now();

        // Record frame time for FPS calculation
        if let Some(last_time) = self.last_frame_time {
            self.metrics.record_frame_time(frame_start.duration_since(last_time));
        }
        self.last_frame_time = Some(frame_start);

        // Encode video frame for file recording if encoder is active
        let encode_start = Instant::now();
        if let Some(ref mut encoder) = self.encoder {
            if let Err(e) = encoder.encode(frame) {
                // Log but don't fail - camera output can still work
                trace!("Video encoding failed: {}", e);
            } else {
                self.metrics.record_encode_timing(encode_start, Instant::now());
            }
        }

        // Process any available audio frames
        self.process_audio_frames();

        let capture_start = Instant::now();

        let gs_frame = match &frame.data {
            FrameData::Memory(data) => {
                let src_width = frame.format.width;
                let src_height = frame.format.height;
                let src_format = fourcc_to_gs_format(frame.format.fourcc);
                let (dst_width, dst_height) = self.output_resolution;

                // Process frame: convert to BGRA if needed, then scale if needed
                let mut processed_data = process_frame_data(
                    data, src_width, src_height, src_format, dst_width, dst_height,
                )?;

                // Apply HDR tonemapping if needed
                self.tonemapper.tonemap(
                    &mut processed_data,
                    dst_width,
                    dst_height,
                    frame.hdr_metadata.as_ref(),
                );

                // Apply latency overlay if enabled
                if self.overlay.is_enabled() {
                    let stats = self.metrics.get_stats();
                    self.overlay.render(&mut processed_data, dst_width, dst_height, &stats);
                }

                Some(GsFrame {
                    data: processed_data,
                    width: dst_width,
                    height: dst_height,
                    stride: dst_width * 4, // BGRA
                    format: GsFrameFormat::Bgra,
                    pts: frame.pts as i64,
                    duration: 0,
                    is_keyframe: true,
                    dmabuf_fd: None,
                })
            }
            FrameData::DmaBuf { .. } => {
                // Try to map the DMA-BUF to CPU memory
                let src_width = frame.format.width;
                let src_height = frame.format.height;
                let expected_size = (frame.format.stride * src_height) as usize;

                match frame.data.try_map_dmabuf(expected_size) {
                    Ok(data) => {
                        let src_format = fourcc_to_gs_format(frame.format.fourcc);
                        let (dst_width, dst_height) = self.output_resolution;

                        match process_frame_data(
                            &data, src_width, src_height, src_format, dst_width, dst_height,
                        ) {
                            Ok(mut processed_data) => {
                                // Apply HDR tonemapping if needed
                                self.tonemapper.tonemap(
                                    &mut processed_data,
                                    dst_width,
                                    dst_height,
                                    frame.hdr_metadata.as_ref(),
                                );

                                // Apply latency overlay if enabled
                                if self.overlay.is_enabled() {
                                    let stats = self.metrics.get_stats();
                                    self.overlay.render(&mut processed_data, dst_width, dst_height, &stats);
                                }

                                Some(GsFrame {
                                    data: processed_data,
                                    width: dst_width,
                                    height: dst_height,
                                    stride: dst_width * 4, // BGRA
                                    format: GsFrameFormat::Bgra,
                                    pts: frame.pts as i64,
                                    duration: 0,
                                    is_keyframe: true,
                                    dmabuf_fd: None,
                                })
                            }
                            Err(e) => {
                                debug!("Failed to process DMA-BUF frame: {}", e);
                                None
                            }
                        }
                    }
                    Err(e) => {
                        // DMA-BUF mapping failed - this can happen with certain modifiers
                        // or when the buffer isn't mappable. Skip the frame.
                        debug!("DMA-BUF mapping failed: {}", e);
                        None
                    }
                }
            }
        };

        self.metrics.record_capture_timing(capture_start, Instant::now());

        // Send to camera
        let output_start = Instant::now();
        if let (Some(camera), Some(gs_frame)) = (&mut self.camera, gs_frame) {
            if let Err(e) = camera.write_frame(&gs_frame).await {
                let failed = self.frames_failed.fetch_add(1, Ordering::Relaxed) + 1;
                error!(
                    "Failed to write frame to camera: {} (total failures: {})",
                    e, failed
                );
            } else {
                self.metrics.record_output_timing(output_start, Instant::now());
                let count = self.frames_processed.fetch_add(1, Ordering::Relaxed) + 1;
                if count % 300 == 0 {
                    // Log stats every ~5 seconds at 60fps
                    let elapsed = self
                        .start_time
                        .map(|t| t.elapsed().as_secs_f64())
                        .unwrap_or(1.0);
                    let fps = count as f64 / elapsed;
                    let dropped = self.frames_dropped.load(Ordering::Relaxed);
                    let failed = self.frames_failed.load(Ordering::Relaxed);
                    let stats = self.metrics.get_stats();
                    debug!(
                        "Pipeline {}: {} frames ({:.1} fps), {} dropped, {} failed | Latency: cap={:.1}ms enc={:.1}ms out={:.1}ms",
                        self.handle, count, fps, dropped, failed,
                        stats.capture_latency_ms, stats.encode_latency_ms, stats.output_latency_ms
                    );
                }
            }
        }

        Ok(())
    }

    /// Stop the pipeline
    pub async fn stop(&mut self) -> Result<()> {
        if self.state == PipelineState::Stopped {
            return Ok(());
        }

        self.state = PipelineState::Stopping;
        info!("Stopping pipeline {}", self.handle);

        // Drop frame receivers first
        self.frame_rx = None;
        self.audio_frame_rx = None;

        // Stop video capture
        if let Some(mut capture) = self.capture.take() {
            capture.stop();
        }

        // Stop audio capture
        if let Some(mut audio_capture) = self.audio_capture.take() {
            audio_capture.stop();
        }

        // Stop virtual microphone
        if let Some(mut virtual_mic) = self.virtual_mic.take() {
            virtual_mic.stop();
            info!("Virtual microphone stopped");
        }

        // Flush video encoder and drop it (closes the broadcast channel)
        if let Some(mut encoder) = self.encoder.take() {
            info!("Flushing video encoder...");
            if let Err(e) = encoder.flush() {
                warn!("Video encoder flush failed: {}", e);
            }
            // Encoder dropped here, closing the broadcast channel
        }

        // Flush audio encoder and drop it
        if let Some(mut audio_encoder) = self.audio_encoder.take() {
            info!("Flushing audio encoder...");
            if let Err(e) = audio_encoder.flush() {
                warn!("Audio encoder flush failed: {}", e);
            }
        }

        // Wait for recorder to finish
        if let Some(handle) = self.recorder_handle.take() {
            info!("Waiting for file recording to complete...");
            match handle.await {
                Ok(Ok(packets)) => info!("Recording complete: {} packets written", packets),
                Ok(Err(e)) => warn!("Recording finished with error: {}", e),
                Err(e) => warn!("Recorder task panicked: {}", e),
            }
        }

        // Wait for streaming to finish
        if let Some(handle) = self.streamer_handle.take() {
            info!("Waiting for streaming to complete...");
            match handle.await {
                Ok(Ok(packets)) => info!("Streaming complete: {} packets sent", packets),
                Ok(Err(e)) => warn!("Streaming finished with error: {}", e),
                Err(e) => warn!("Streamer task panicked: {}", e),
            }
        }

        // Stop WebRTC signaling server
        if let Some(handle) = self.webrtc_server_handle.take() {
            info!("Stopping WebRTC signaling server...");
            handle.abort(); // Signaling server runs forever, so abort it
        }

        // Stop WebRTC output
        if let Some(webrtc) = self.webrtc_output.take() {
            let mut output = webrtc.write().await;
            if let Err(e) = output.stop().await {
                warn!("Failed to stop WebRTC output: {}", e);
            }
        }

        // Stop camera (using RawOutputSink::finish)
        if let Some(mut camera) = self.camera.take() {
            if let Err(e) = camera.finish().await {
                warn!("Failed to cleanly stop virtual camera: {}", e);
            }
        }

        // Stop portal session
        if let Err(e) = self.portal.stop_session().await {
            warn!("Failed to cleanly stop portal session: {}", e);
        }

        self.state = PipelineState::Stopped;

        let elapsed = self
            .start_time
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0);
        let frames = self.frames_processed.load(Ordering::Relaxed);
        let dropped = self.frames_dropped.load(Ordering::Relaxed);
        let audio_samples = self.audio_samples_processed.load(Ordering::Relaxed);

        if audio_samples > 0 {
            info!(
                "Pipeline {} stopped - {} frames in {:.1}s ({:.1} fps), {} dropped, {} audio samples",
                self.handle,
                frames,
                elapsed,
                if elapsed > 0.0 { frames as f64 / elapsed } else { 0.0 },
                dropped,
                audio_samples
            );
        } else {
            info!(
                "Pipeline {} stopped - {} frames in {:.1}s ({:.1} fps), {} dropped",
                self.handle,
                frames,
                elapsed,
                if elapsed > 0.0 {
                    frames as f64 / elapsed
                } else {
                    0.0
                },
                dropped
            );
        }

        Ok(())
    }

    /// Process available audio frames
    fn process_audio_frames(&mut self) {
        // Skip if no audio encoder
        let Some(ref mut audio_encoder) = self.audio_encoder else {
            return;
        };

        // Get audio receiver, re-subscribing if needed
        if self.audio_frame_rx.is_none() {
            if let Some(ref audio_capture) = self.audio_capture {
                self.audio_frame_rx = Some(audio_capture.subscribe());
            } else {
                return;
            }
        }

        // Process all available audio frames (non-blocking)
        let mut needs_resubscribe = false;

        if let Some(ref mut audio_rx) = self.audio_frame_rx {
            loop {
                match audio_rx.try_recv() {
                    Ok(audio_frame) => {
                        if let Err(e) = audio_encoder.encode(&audio_frame) {
                            trace!("Audio encoding failed: {}", e);
                        } else {
                            self.audio_samples_processed
                                .fetch_add(audio_frame.sample_count as u64, Ordering::Relaxed);
                        }
                    }
                    Err(broadcast::error::TryRecvError::Empty) => {
                        // No more frames available
                        break;
                    }
                    Err(broadcast::error::TryRecvError::Lagged(n)) => {
                        warn!("Dropped {} audio frames due to lag", n);
                        needs_resubscribe = true;
                        break;
                    }
                    Err(broadcast::error::TryRecvError::Closed) => {
                        debug!("Audio capture channel closed");
                        break;
                    }
                }
            }
        }

        // Re-subscribe outside of the borrow
        if needs_resubscribe {
            if let Some(ref audio_capture) = self.audio_capture {
                self.audio_frame_rx = Some(audio_capture.subscribe());
            }
        }
    }

    /// Check if the pipeline is running
    pub fn is_running(&self) -> bool {
        matches!(
            self.state,
            PipelineState::Running | PipelineState::WaitingForStream
        )
    }

    /// Get the number of frames processed
    pub fn frames_processed(&self) -> u64 {
        self.frames_processed.load(Ordering::Relaxed)
    }

    /// Get the number of frames dropped
    pub fn frames_dropped(&self) -> u64 {
        self.frames_dropped.load(Ordering::Relaxed)
    }

    /// Get the number of frames that failed to write
    pub fn frames_failed(&self) -> u64 {
        self.frames_failed.load(Ordering::Relaxed)
    }

    /// Get performance metrics
    pub fn metrics(&self) -> Arc<PerformanceMetrics> {
        self.metrics.clone()
    }

    /// Toggle the latency overlay on/off
    pub fn toggle_overlay(&mut self) {
        self.overlay.toggle();
        info!("Latency overlay: {}", if self.overlay.is_enabled() { "enabled" } else { "disabled" });
    }

    /// Check if overlay is enabled
    pub fn overlay_enabled(&self) -> bool {
        self.overlay.is_enabled()
    }

    /// Get pipeline statistics
    pub fn stats(&self) -> PipelineStats {
        let elapsed = self
            .start_time
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0);
        let frames = self.frames_processed.load(Ordering::Relaxed);
        let dropped = self.frames_dropped.load(Ordering::Relaxed);
        let failed = self.frames_failed.load(Ordering::Relaxed);

        PipelineStats {
            handle: self.handle,
            state: self.state,
            resolution: (self.config.width(), self.config.height()),
            fps: self.config.fps(),
            codec: self.config.codec.display_name().to_string(),
            bitrate: self.config.effective_bitrate(),
            frames_processed: frames,
            frames_dropped: dropped,
            frames_failed: failed,
            actual_fps: if elapsed > 0.0 {
                frames as f64 / elapsed
            } else {
                0.0
            },
            elapsed_seconds: elapsed,
        }
    }
}

/// Process frame data: convert colorspace and scale as needed
fn process_frame_data(
    data: &[u8],
    src_width: u32,
    src_height: u32,
    src_format: GsFrameFormat,
    dst_width: u32,
    dst_height: u32,
) -> Result<Vec<u8>> {
    // Step 1: Convert to BGRA if not already
    let bgra_data = if src_format != GsFrameFormat::Bgra {
        convert_colorspace(data, src_format, GsFrameFormat::Bgra, src_width, src_height)
            .map_err(|e| NitrogenError::encoder(format!("Colorspace conversion failed: {}", e)))?
    } else {
        data.to_vec()
    };

    // Step 2: Scale if dimensions differ
    if src_width != dst_width || src_height != dst_height {
        scale_frame(&bgra_data, src_width, src_height, dst_width, dst_height)
            .map_err(|e| NitrogenError::encoder(format!("Scaling failed: {}", e)))
    } else {
        Ok(bgra_data)
    }
}

/// Pipeline statistics
#[derive(Debug, Clone)]
pub struct PipelineStats {
    /// Pipeline handle
    pub handle: Handle,
    /// Current state
    pub state: PipelineState,
    /// Output resolution
    pub resolution: (u32, u32),
    /// Target framerate
    pub fps: u32,
    /// Codec name
    pub codec: String,
    /// Target bitrate in kbps
    pub bitrate: u32,
    /// Number of frames processed
    pub frames_processed: u64,
    /// Number of frames dropped (channel lag)
    pub frames_dropped: u64,
    /// Number of frames failed to write
    pub frames_failed: u64,
    /// Actual measured FPS
    pub actual_fps: f64,
    /// Elapsed time in seconds
    pub elapsed_seconds: f64,
}

impl std::fmt::Display for PipelineStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Pipeline {}: {:?} - {}x{} @ {}fps (actual: {:.1}fps), {} @ {}kbps, {} frames, {} dropped, {} failed",
            self.handle,
            self.state,
            self.resolution.0,
            self.resolution.1,
            self.fps,
            self.actual_fps,
            self.codec,
            self.bitrate,
            self.frames_processed,
            self.frames_dropped,
            self.frames_failed
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_state() {
        assert_eq!(PipelineState::Idle, PipelineState::Idle);
        assert_ne!(PipelineState::Idle, PipelineState::Running);
    }
}

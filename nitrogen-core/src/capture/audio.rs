//! PipeWire audio capture
//!
//! Captures desktop audio (monitor) or microphone input via PipeWire.

use pipewire as pw;
use pw::spa::param::audio::AudioFormat as SpaAudioFormat;
use pw::spa::param::audio::AudioInfoRaw;
use pw::spa::param::format::{MediaSubtype, MediaType};
use pw::spa::param::format_utils;
use pw::spa::pod::Pod;
use pw::spa::utils::Direction;
use pw::stream::{Stream, StreamFlags, StreamState};

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, trace, warn};

use crate::config::AudioSource;
use crate::error::{NitrogenError, Result};
use crate::types::{AudioFormat, AudioFrame, AudioSampleFormat};

/// Shared state between PipeWire thread and main thread
struct SharedState {
    /// Audio format information
    format: parking_lot::Mutex<Option<AudioInfo>>,
    /// Sample counter
    samples_captured: AtomicU64,
    /// Whether the stream is running
    running: AtomicBool,
}

/// Parsed audio format information
#[derive(Debug, Clone, Copy)]
struct AudioInfo {
    format: SpaAudioFormat,
    rate: u32,
    channels: u32,
}

/// PipeWire audio capture stream
pub struct AudioCaptureStream {
    /// Sender for audio frames
    frame_tx: broadcast::Sender<Arc<AudioFrame>>,
    /// Thread handle for the PipeWire main loop
    pw_thread: Option<std::thread::JoinHandle<()>>,
    /// Channel to signal shutdown
    shutdown_tx: Option<mpsc::Sender<()>>,
    /// Shared state
    shared: Arc<SharedState>,
}

impl AudioCaptureStream {
    /// Create a new audio capture stream
    ///
    /// # Arguments
    /// * `source` - What audio to capture (desktop, microphone, or both)
    pub fn new(source: AudioSource) -> Result<Self> {
        if source == AudioSource::None {
            return Err(NitrogenError::config(
                "Cannot create audio stream with AudioSource::None",
            ));
        }

        let (frame_tx, _) = broadcast::channel(16);
        let (shutdown_tx, shutdown_rx) = mpsc::channel();

        let shared = Arc::new(SharedState {
            format: parking_lot::Mutex::new(None),
            samples_captured: AtomicU64::new(0),
            running: AtomicBool::new(false),
        });

        let frame_tx_clone = frame_tx.clone();
        let shared_clone = shared.clone();
        let is_desktop = matches!(source, AudioSource::Desktop | AudioSource::Both);

        let pw_thread = std::thread::Builder::new()
            .name("nitrogen-audio".to_string())
            .spawn(move || {
                if let Err(e) =
                    run_audio_loop(is_desktop, frame_tx_clone, shutdown_rx, shared_clone)
                {
                    error!("Audio capture loop error: {}", e);
                }
            })
            .map_err(|e| NitrogenError::pipewire(format!("Failed to spawn audio thread: {}", e)))?;

        Ok(Self {
            frame_tx,
            pw_thread: Some(pw_thread),
            shutdown_tx: Some(shutdown_tx),
            shared,
        })
    }

    /// Subscribe to audio frames from this stream
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<AudioFrame>> {
        self.frame_tx.subscribe()
    }

    /// Check if the stream is running
    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::SeqCst)
            && self
                .pw_thread
                .as_ref()
                .map(|t| !t.is_finished())
                .unwrap_or(false)
    }

    /// Get the total samples captured
    pub fn samples_captured(&self) -> u64 {
        self.shared.samples_captured.load(Ordering::Relaxed)
    }

    /// Get the current audio format if known
    pub fn format(&self) -> Option<AudioFormat> {
        self.shared.format.lock().map(|info| AudioFormat {
            sample_rate: info.rate,
            channels: info.channels,
            format: spa_to_sample_format(info.format),
        })
    }

    /// Stop the audio capture stream
    pub fn stop(&mut self) {
        info!("Stopping audio capture stream");

        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        if let Some(thread) = self.pw_thread.take() {
            let _ = thread.join();
        }

        self.shared.running.store(false, Ordering::SeqCst);
        info!("Audio capture stream stopped");
    }
}

impl Drop for AudioCaptureStream {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Convert SPA audio format to our format
fn spa_to_sample_format(spa: SpaAudioFormat) -> AudioSampleFormat {
    match spa {
        SpaAudioFormat::F32LE | SpaAudioFormat::F32BE | SpaAudioFormat::F32P => {
            AudioSampleFormat::F32LE
        }
        SpaAudioFormat::S16LE | SpaAudioFormat::S16BE | SpaAudioFormat::S16P => {
            AudioSampleFormat::S16LE
        }
        SpaAudioFormat::S32LE | SpaAudioFormat::S32BE | SpaAudioFormat::S32P => {
            AudioSampleFormat::S32LE
        }
        _ => AudioSampleFormat::F32LE, // Default
    }
}

/// Run the PipeWire audio main loop
fn run_audio_loop(
    is_desktop: bool,
    frame_tx: broadcast::Sender<Arc<AudioFrame>>,
    shutdown_rx: mpsc::Receiver<()>,
    shared: Arc<SharedState>,
) -> Result<()> {
    pw::init();

    info!(
        "Initializing PipeWire audio capture (desktop={})",
        is_desktop
    );

    let mainloop = pw::main_loop::MainLoop::new(None)
        .map_err(|e| NitrogenError::pipewire(format!("Failed to create audio main loop: {}", e)))?;

    let loop_ = mainloop.loop_();

    let context = pw::context::Context::new(&mainloop)
        .map_err(|e| NitrogenError::pipewire(format!("Failed to create audio context: {}", e)))?;

    let core = context
        .connect(None)
        .map_err(|e| NitrogenError::pipewire(format!("Failed to connect to PipeWire: {}", e)))?;

    // Build stream properties
    let props = if is_desktop {
        // Capture desktop audio (what you hear)
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
            *pw::keys::STREAM_CAPTURE_SINK => "true",
        }
    } else {
        // Capture microphone
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Communication",
        }
    };

    let stream = Stream::new(&core, "nitrogen-audio", props)
        .map_err(|e| NitrogenError::pipewire(format!("Failed to create audio stream: {}", e)))?;

    // User data for callbacks
    struct UserData {
        format: Option<AudioInfo>,
        frame_tx: broadcast::Sender<Arc<AudioFrame>>,
        shared: Arc<SharedState>,
    }

    let user_data = UserData {
        format: None,
        frame_tx,
        shared: shared.clone(),
    };

    let _listener = stream
        .add_local_listener_with_user_data(user_data)
        .state_changed(|_, user_data, old, new| {
            debug!("Audio stream state: {:?} -> {:?}", old, new);
            match new {
                StreamState::Streaming => {
                    info!("Audio stream is now streaming");
                    user_data.shared.running.store(true, Ordering::SeqCst);
                }
                StreamState::Error(msg) => {
                    error!("Audio stream error: {}", msg);
                    user_data.shared.running.store(false, Ordering::SeqCst);
                }
                StreamState::Paused => {
                    warn!("Audio stream paused");
                }
                StreamState::Unconnected => {
                    info!("Audio stream disconnected");
                    user_data.shared.running.store(false, Ordering::SeqCst);
                }
                _ => {}
            }
        })
        .param_changed(|_, user_data, id, param| {
            let Some(param) = param else { return };
            if id != pw::spa::param::ParamType::Format.as_raw() {
                return;
            }

            let (media_type, media_subtype) = match format_utils::parse_format(param) {
                Ok(v) => v,
                Err(_) => return,
            };

            if media_type != MediaType::Audio || media_subtype != MediaSubtype::Raw {
                return;
            }

            let mut audio_info = AudioInfoRaw::new();
            if audio_info.parse(param).is_err() {
                warn!("Failed to parse audio format");
                return;
            }

            let info = AudioInfo {
                format: audio_info.format(),
                rate: audio_info.rate(),
                channels: audio_info.channels(),
            };

            info!(
                "Audio format negotiated: {:?} {} channels @ {} Hz",
                info.format, info.channels, info.rate
            );

            user_data.format = Some(info);
            *user_data.shared.format.lock() = Some(info);
        })
        .process(|stream, user_data| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                trace!("No audio buffer available");
                return;
            };

            let Some(format) = user_data.format else {
                trace!("No audio format yet");
                return;
            };

            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }

            let data = &mut datas[0];
            let chunk_size = data.chunk().size() as usize;

            if chunk_size == 0 {
                return;
            }

            if let Some(samples_raw) = data.data() {
                let samples_raw = &samples_raw[..chunk_size];

                // Convert to f32 samples
                let samples: Vec<f32> = match format.format {
                    SpaAudioFormat::F32LE | SpaAudioFormat::F32P => samples_raw
                        .chunks_exact(4)
                        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        .collect(),
                    SpaAudioFormat::F32BE => samples_raw
                        .chunks_exact(4)
                        .map(|b| f32::from_be_bytes([b[0], b[1], b[2], b[3]]))
                        .collect(),
                    SpaAudioFormat::S16LE | SpaAudioFormat::S16P => samples_raw
                        .chunks_exact(2)
                        .map(|b| {
                            let i = i16::from_le_bytes([b[0], b[1]]);
                            i as f32 / 32768.0
                        })
                        .collect(),
                    SpaAudioFormat::S16BE => samples_raw
                        .chunks_exact(2)
                        .map(|b| {
                            let i = i16::from_be_bytes([b[0], b[1]]);
                            i as f32 / 32768.0
                        })
                        .collect(),
                    SpaAudioFormat::S32LE | SpaAudioFormat::S32P => samples_raw
                        .chunks_exact(4)
                        .map(|b| {
                            let i = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
                            i as f32 / 2147483648.0
                        })
                        .collect(),
                    SpaAudioFormat::S32BE => samples_raw
                        .chunks_exact(4)
                        .map(|b| {
                            let i = i32::from_be_bytes([b[0], b[1], b[2], b[3]]);
                            i as f32 / 2147483648.0
                        })
                        .collect(),
                    _ => {
                        debug!(
                            "Unsupported audio format {:?}, treating as F32LE",
                            format.format
                        );
                        samples_raw
                            .chunks_exact(4)
                            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                            .collect()
                    }
                };

                let sample_count = samples.len() / format.channels as usize;

                let frame = AudioFrame {
                    format: AudioFormat {
                        sample_rate: format.rate,
                        channels: format.channels,
                        format: spa_to_sample_format(format.format),
                    },
                    samples,
                    pts: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos() as u64)
                        .unwrap_or(0),
                    sample_count: sample_count as u32,
                };

                let total = user_data
                    .shared
                    .samples_captured
                    .fetch_add(sample_count as u64, Ordering::Relaxed);
                if user_data.frame_tx.send(Arc::new(frame)).is_err() {
                    // No receivers
                }

                if total % 48000 == 0 {
                    trace!("Captured {} audio samples", total + sample_count as u64);
                }
            }
        })
        .register()
        .map_err(|e| {
            NitrogenError::pipewire(format!("Failed to register audio listener: {}", e))
        })?;

    // Build format parameters - prefer F32LE stereo at 48kHz
    // Use simple format specification (PipeWire will negotiate compatible format)
    let obj = pw::spa::pod::object!(
        pw::spa::utils::SpaTypes::ObjectParamFormat,
        pw::spa::param::ParamType::EnumFormat,
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::MediaType,
            Id,
            MediaType::Audio
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::MediaSubtype,
            Id,
            MediaSubtype::Raw
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::AudioFormat,
            Choice,
            Enum,
            Id,
            SpaAudioFormat::F32LE,
            SpaAudioFormat::F32LE,
            SpaAudioFormat::S32LE,
            SpaAudioFormat::S16LE,
            SpaAudioFormat::F32BE,
            SpaAudioFormat::S32BE,
            SpaAudioFormat::S16BE
        ),
    );

    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )
    .map_err(|e| NitrogenError::pipewire(format!("Failed to serialize audio format: {:?}", e)))?
    .0
    .into_inner();

    let pod = Pod::from_bytes(&values)
        .ok_or_else(|| NitrogenError::pipewire("Failed to create audio Pod"))?;
    let mut params = [pod];

    stream
        .connect(
            Direction::Input,
            None, // Auto-connect to default source
            StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS,
            &mut params,
        )
        .map_err(|e| NitrogenError::pipewire(format!("Failed to connect audio stream: {}", e)))?;

    info!("Audio stream connected");

    // Shutdown handling
    let mainloop_weak = mainloop.downgrade();
    let _source = loop_.add_idle(true, move || {
        if shutdown_rx.try_recv().is_ok() {
            info!("Audio shutdown signal received");
            if let Some(mainloop) = mainloop_weak.upgrade() {
                mainloop.quit();
            }
        }
    });

    mainloop.run();

    info!("Audio main loop ended");
    shared.running.store(false, Ordering::SeqCst);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spa_format_conversion() {
        assert_eq!(
            spa_to_sample_format(SpaAudioFormat::F32LE),
            AudioSampleFormat::F32LE
        );
        assert_eq!(
            spa_to_sample_format(SpaAudioFormat::S16LE),
            AudioSampleFormat::S16LE
        );
        assert_eq!(
            spa_to_sample_format(SpaAudioFormat::S32LE),
            AudioSampleFormat::S32LE
        );
    }
}

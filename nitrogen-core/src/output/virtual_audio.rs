//! Virtual Audio Output
//!
//! Creates a PipeWire virtual microphone that outputs captured system audio.
//! This allows Discord and other apps to use system audio as a microphone input.

use pipewire as pw;
use pw::spa::param::audio::AudioFormat as SpaAudioFormat;
use pw::spa::param::format::{MediaSubtype, MediaType};
use pw::spa::pod::Pod;
use pw::spa::utils::Direction;
use pw::stream::StreamFlags;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{error, info};

use crate::error::{NitrogenError, Result};
use crate::types::AudioFrame;

/// Virtual microphone name
pub const DEFAULT_VIRTUAL_MIC_NAME: &str = "Nitrogen Audio";

/// Shared state between PipeWire thread and main thread
struct SharedState {
    /// Sample rate (for stats/debugging)
    #[allow(dead_code)]
    sample_rate: u32,
    /// Number of channels (for stats/debugging)
    #[allow(dead_code)]
    channels: u32,
    /// Samples written
    samples_written: AtomicU64,
    /// Whether the stream is running
    running: AtomicBool,
}

/// Virtual microphone that outputs captured audio to PipeWire
pub struct VirtualMicrophone {
    /// Thread handle for the PipeWire main loop
    pw_thread: Option<std::thread::JoinHandle<()>>,
    /// Channel to signal shutdown
    shutdown_tx: Option<mpsc::Sender<()>>,
    /// Shared state
    shared: Arc<SharedState>,
}

impl VirtualMicrophone {
    /// Create a new virtual microphone
    ///
    /// # Arguments
    /// * `name` - Name of the virtual device (appears in PipeWire/PulseAudio)
    /// * `audio_rx` - Receiver for audio frames from capture
    /// * `sample_rate` - Audio sample rate (typically 48000)
    /// * `channels` - Number of audio channels (typically 2)
    pub fn new(
        name: Option<&str>,
        audio_rx: broadcast::Receiver<Arc<AudioFrame>>,
        sample_rate: u32,
        channels: u32,
    ) -> Result<Self> {
        let device_name = name.unwrap_or(DEFAULT_VIRTUAL_MIC_NAME).to_string();
        let (shutdown_tx, shutdown_rx) = mpsc::channel();

        let shared = Arc::new(SharedState {
            sample_rate,
            channels,
            samples_written: AtomicU64::new(0),
            running: AtomicBool::new(false),
        });

        let shared_clone = shared.clone();

        let pw_thread = std::thread::Builder::new()
            .name("nitrogen-virtual-mic".to_string())
            .spawn(move || {
                if let Err(e) = run_virtual_mic_loop(
                    &device_name,
                    audio_rx,
                    shutdown_rx,
                    shared_clone,
                    sample_rate,
                    channels,
                ) {
                    error!("Virtual microphone loop error: {}", e);
                }
            })
            .map_err(|e| {
                NitrogenError::pipewire(format!("Failed to spawn virtual mic thread: {}", e))
            })?;

        Ok(Self {
            pw_thread: Some(pw_thread),
            shutdown_tx: Some(shutdown_tx),
            shared,
        })
    }

    /// Check if the virtual microphone is running
    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::Relaxed)
    }

    /// Get the number of samples written
    pub fn samples_written(&self) -> u64 {
        self.shared.samples_written.load(Ordering::Relaxed)
    }

    /// Stop the virtual microphone
    pub fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(thread) = self.pw_thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for VirtualMicrophone {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Run the PipeWire main loop for virtual microphone output
fn run_virtual_mic_loop(
    device_name: &str,
    mut audio_rx: broadcast::Receiver<Arc<AudioFrame>>,
    shutdown_rx: mpsc::Receiver<()>,
    shared: Arc<SharedState>,
    sample_rate: u32,
    channels: u32,
) -> Result<()> {
    // Initialize PipeWire
    pw::init();

    let mainloop = pw::main_loop::MainLoop::new(None)
        .map_err(|e| NitrogenError::pipewire(format!("Failed to create main loop: {:?}", e)))?;

    let context = pw::context::Context::new(&mainloop)
        .map_err(|e| NitrogenError::pipewire(format!("Failed to create context: {:?}", e)))?;

    let core = context
        .connect(None)
        .map_err(|e| NitrogenError::pipewire(format!("Failed to connect to PipeWire: {:?}", e)))?;

    // Create audio output stream (appears as a capture device to other apps)
    let props = pw::properties::properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Communication",
        *pw::keys::NODE_NAME => device_name,
        *pw::keys::NODE_DESCRIPTION => "Nitrogen System Audio Output",
        *pw::keys::STREAM_IS_LIVE => "true",
        // Make this appear as a virtual microphone
        "stream.capture.sink" => "false",
        "media.class" => "Audio/Source/Virtual",
    };

    let shared_for_process = shared.clone();

    let stream = pw::stream::Stream::new(&core, device_name, props)
        .map_err(|e| NitrogenError::pipewire(format!("Failed to create stream: {:?}", e)))?;

    // Build format parameters for F32 stereo audio
    let format_bytes = build_audio_format_pod(sample_rate, channels)?;
    let pod = Pod::from_bytes(&format_bytes)
        .ok_or_else(|| NitrogenError::pipewire("Failed to create audio format Pod"))?;

    // Stream callbacks - handle process callback for writing data
    let _listener = stream
        .add_local_listener_with_user_data(())
        .state_changed(|_, _, old_state, new_state| {
            info!(
                "Virtual mic stream state: {:?} -> {:?}",
                old_state, new_state
            );
        })
        .process(move |stream, _| {
            // Get output buffer
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };

            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }

            let data = &mut datas[0];
            let stride = (channels * std::mem::size_of::<f32>() as u32) as usize;

            // Get audio data from channel (non-blocking)
            match audio_rx.try_recv() {
                Ok(frame) => {
                    // Write audio samples to buffer
                    if let Some(slice) = data.data() {
                        let samples = &frame.samples;
                        let max_samples = slice.len() / std::mem::size_of::<f32>();
                        let copy_samples = samples.len().min(max_samples);
                        let copy_bytes = copy_samples * std::mem::size_of::<f32>();

                        // Safety: we're writing to the PipeWire buffer
                        unsafe {
                            let dst = slice.as_ptr() as *mut f32;
                            std::ptr::copy_nonoverlapping(samples.as_ptr(), dst, copy_samples);

                            // Zero remaining if buffer is larger
                            if copy_samples < max_samples {
                                let remaining = max_samples - copy_samples;
                                std::ptr::write_bytes(
                                    dst.add(copy_samples),
                                    0,
                                    remaining * std::mem::size_of::<f32>(),
                                );
                            }
                        }

                        // Set chunk metadata
                        let chunk = data.chunk_mut();
                        *chunk.offset_mut() = 0;
                        *chunk.stride_mut() = stride as i32;
                        *chunk.size_mut() = copy_bytes as u32;

                        shared_for_process
                            .samples_written
                            .fetch_add(copy_samples as u64, Ordering::Relaxed);
                    }
                }
                Err(_) => {
                    // No audio available, output silence
                    if let Some(slice) = data.data() {
                        let buffer_size = slice.len();

                        unsafe {
                            let dst = slice.as_ptr() as *mut u8;
                            std::ptr::write_bytes(dst, 0, buffer_size);
                        }

                        let chunk = data.chunk_mut();
                        *chunk.offset_mut() = 0;
                        *chunk.stride_mut() = stride as i32;
                        *chunk.size_mut() = buffer_size as u32;
                    }
                }
            }
            // Buffer is auto-queued when dropped
        })
        .register()
        .map_err(|e| NitrogenError::pipewire(format!("Failed to register listener: {:?}", e)))?;

    // Connect stream
    stream
        .connect(
            Direction::Output,
            None,
            StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::RT_PROCESS,
            &mut [pod],
        )
        .map_err(|e| NitrogenError::pipewire(format!("Failed to connect stream: {:?}", e)))?;

    shared.running.store(true, Ordering::SeqCst);
    info!(
        "Virtual microphone '{}' started ({}Hz, {} ch)",
        device_name, sample_rate, channels
    );

    // Run main loop until shutdown
    loop {
        // Check for shutdown signal
        if shutdown_rx.try_recv().is_ok() {
            info!("Virtual microphone shutdown requested");
            break;
        }

        // Run one iteration of the main loop
        mainloop
            .loop_()
            .iterate(std::time::Duration::from_millis(10));
    }

    shared.running.store(false, Ordering::SeqCst);
    info!("Virtual microphone stopped");
    Ok(())
}

/// Build audio format POD for PipeWire F32LE output
fn build_audio_format_pod(sample_rate: u32, channels: u32) -> Result<Vec<u8>> {
    // Build format object for F32LE stereo audio output
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
            SpaAudioFormat::F32LE
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::AudioRate,
            Int,
            sample_rate as i32
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::AudioChannels,
            Int,
            channels as i32
        ),
    );

    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )
    .map_err(|e| NitrogenError::pipewire(format!("Failed to serialize audio format: {:?}", e)))?
    .0
    .into_inner();

    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_name() {
        assert_eq!(DEFAULT_VIRTUAL_MIC_NAME, "Nitrogen Audio");
    }
}

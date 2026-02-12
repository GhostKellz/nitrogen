//! Audio mixing for multiple sources
//!
//! Combines multiple audio streams (desktop + microphone) into a single output
//! with configurable volume levels for each source.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::config::AudioSource;
use crate::error::{NitrogenError, Result};
use crate::types::{AudioFormat, AudioFrame, AudioSampleFormat};

use super::AudioCaptureStream;

/// Volume control for an audio source
#[derive(Debug, Clone, Copy)]
pub struct VolumeControl {
    /// Volume level (0.0 = muted, 1.0 = normal, 2.0 = doubled, etc.)
    pub volume: f32,
    /// Muted flag
    pub muted: bool,
}

impl Default for VolumeControl {
    fn default() -> Self {
        Self {
            volume: 1.0,
            muted: false,
        }
    }
}

impl VolumeControl {
    /// Create a new volume control with specified volume
    pub fn new(volume: f32) -> Self {
        Self {
            volume: volume.max(0.0),
            muted: false,
        }
    }

    /// Get the effective volume (0.0 if muted)
    pub fn effective_volume(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.volume
        }
    }
}

/// Audio mixer configuration
#[derive(Debug, Clone)]
pub struct MixerConfig {
    /// Desktop audio volume
    pub desktop_volume: VolumeControl,
    /// Microphone volume
    pub mic_volume: VolumeControl,
    /// Output sample rate
    pub output_sample_rate: u32,
    /// Output channels
    pub output_channels: u32,
    /// Apply ducking (reduce desktop volume when mic is active)
    pub ducking_enabled: bool,
    /// Ducking amount (how much to reduce desktop when mic active, 0.0-1.0)
    pub ducking_amount: f32,
    /// Ducking threshold (mic amplitude to trigger ducking)
    pub ducking_threshold: f32,
}

impl Default for MixerConfig {
    fn default() -> Self {
        Self {
            desktop_volume: VolumeControl::default(),
            mic_volume: VolumeControl::default(),
            output_sample_rate: 48000,
            output_channels: 2,
            ducking_enabled: false,
            ducking_amount: 0.5,
            ducking_threshold: 0.05,
        }
    }
}

/// Audio mixer that combines multiple sources
pub struct AudioMixer {
    /// Configuration
    config: MixerConfig,
    /// Desktop audio stream
    desktop_stream: Option<AudioCaptureStream>,
    /// Microphone stream
    mic_stream: Option<AudioCaptureStream>,
    /// Output sender
    output_tx: broadcast::Sender<Arc<AudioFrame>>,
    /// Running flag
    running: AtomicBool,
    /// Frame counter
    frame_count: std::sync::atomic::AtomicU64,
}

impl AudioMixer {
    /// Create a new audio mixer
    pub fn new(source: AudioSource, config: MixerConfig) -> Result<Self> {
        if source == AudioSource::None {
            return Err(NitrogenError::config(
                "Cannot create mixer with AudioSource::None",
            ));
        }

        let (output_tx, _) = broadcast::channel(32);

        // Create streams based on source
        let desktop_stream = if matches!(source, AudioSource::Desktop | AudioSource::Both) {
            info!("Creating desktop audio capture for mixer");
            Some(AudioCaptureStream::new(AudioSource::Desktop)?)
        } else {
            None
        };

        let mic_stream = if matches!(source, AudioSource::Microphone | AudioSource::Both) {
            info!("Creating microphone capture for mixer");
            Some(AudioCaptureStream::new(AudioSource::Microphone)?)
        } else {
            None
        };

        info!(
            "Audio mixer created: desktop={}, mic={}",
            desktop_stream.is_some(),
            mic_stream.is_some()
        );

        Ok(Self {
            config,
            desktop_stream,
            mic_stream,
            output_tx,
            running: AtomicBool::new(false),
            frame_count: std::sync::atomic::AtomicU64::new(0),
        })
    }

    /// Subscribe to mixed audio output
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<AudioFrame>> {
        self.output_tx.subscribe()
    }

    /// Start the mixer
    pub async fn start(&mut self) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        info!("Starting audio mixer");
        self.running.store(true, Ordering::SeqCst);

        Ok(())
    }

    /// Run the mixing loop
    pub async fn run(&self) -> Result<()> {
        let desktop_rx = self.desktop_stream.as_ref().map(|s| s.subscribe());
        let mic_rx = self.mic_stream.as_ref().map(|s| s.subscribe());

        // Route based on available sources using pattern matching (no unwrap panic risk)
        match (desktop_rx, mic_rx) {
            (Some(desktop), None) => {
                self.forward_single_source(desktop, self.config.desktop_volume)
                    .await?;
            }
            (None, Some(mic)) => {
                self.forward_single_source(mic, self.config.mic_volume)
                    .await?;
            }
            (Some(desktop), Some(mic)) => {
                self.mix_sources(desktop, mic).await?;
            }
            (None, None) => {
                // No sources available - nothing to mix
            }
        }

        Ok(())
    }

    /// Forward a single source with volume applied
    async fn forward_single_source(
        &self,
        mut rx: broadcast::Receiver<Arc<AudioFrame>>,
        volume: VolumeControl,
    ) -> Result<()> {
        while self.running.load(Ordering::SeqCst) {
            match rx.recv().await {
                Ok(frame) => {
                    let adjusted = self.apply_volume(&frame, volume.effective_volume());
                    if self.output_tx.send(Arc::new(adjusted)).is_err() {
                        // No receivers
                    }
                    self.frame_count.fetch_add(1, Ordering::Relaxed);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("Audio source closed");
                    break;
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("Mixer dropped {} audio frames due to lag", n);
                }
            }
        }
        Ok(())
    }

    /// Mix two audio sources together
    async fn mix_sources(
        &self,
        mut desktop_rx: broadcast::Receiver<Arc<AudioFrame>>,
        mut mic_rx: broadcast::Receiver<Arc<AudioFrame>>,
    ) -> Result<()> {
        use std::collections::VecDeque;

        // Buffer for synchronizing sources
        let mut desktop_buffer: VecDeque<Arc<AudioFrame>> = VecDeque::with_capacity(8);
        let mut mic_buffer: VecDeque<Arc<AudioFrame>> = VecDeque::with_capacity(8);

        let desktop_done = std::sync::atomic::AtomicBool::new(false);
        let mic_done = std::sync::atomic::AtomicBool::new(false);

        while self.running.load(Ordering::SeqCst) {
            // Collect frames from both sources
            tokio::select! {
                biased;

                desktop_result = desktop_rx.recv(), if !desktop_done.load(Ordering::SeqCst) => {
                    match desktop_result {
                        Ok(frame) => {
                            desktop_buffer.push_back(frame);
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            info!("Desktop audio closed");
                            desktop_done.store(true, Ordering::SeqCst);
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!("Mixer dropped {} desktop frames", n);
                        }
                    }
                }

                mic_result = mic_rx.recv(), if !mic_done.load(Ordering::SeqCst) => {
                    match mic_result {
                        Ok(frame) => {
                            mic_buffer.push_back(frame);
                        }
                        Err(broadcast::error::RecvError::Closed) => {
                            info!("Microphone closed");
                            mic_done.store(true, Ordering::SeqCst);
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            warn!("Mixer dropped {} mic frames", n);
                        }
                    }
                }

                // Short timeout to process buffers
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(5)) => {}
            }

            // Process buffered frames
            while let (Some(desktop), Some(mic)) = (desktop_buffer.front(), mic_buffer.front()) {
                let mixed = self.mix_frames(desktop, mic);
                if self.output_tx.send(Arc::new(mixed)).is_err() {
                    // No receivers
                }
                self.frame_count.fetch_add(1, Ordering::Relaxed);

                desktop_buffer.pop_front();
                mic_buffer.pop_front();
            }

            // If one source is done, drain the other
            if desktop_done.load(Ordering::SeqCst) {
                while let Some(mic) = mic_buffer.pop_front() {
                    let adjusted = self.apply_volume(&mic, self.config.mic_volume.effective_volume());
                    let _ = self.output_tx.send(Arc::new(adjusted));
                }
            }
            if mic_done.load(Ordering::SeqCst) {
                while let Some(desktop) = desktop_buffer.pop_front() {
                    let adjusted =
                        self.apply_volume(&desktop, self.config.desktop_volume.effective_volume());
                    let _ = self.output_tx.send(Arc::new(adjusted));
                }
            }

            // Exit when both sources are done
            if desktop_done.load(Ordering::SeqCst) && mic_done.load(Ordering::SeqCst) {
                break;
            }
        }

        Ok(())
    }

    /// Mix two audio frames together
    fn mix_frames(&self, desktop: &AudioFrame, mic: &AudioFrame) -> AudioFrame {
        let desktop_vol = self.config.desktop_volume.effective_volume();
        let mic_vol = self.config.mic_volume.effective_volume();

        // Apply ducking if enabled
        let effective_desktop_vol = if self.config.ducking_enabled {
            let mic_amplitude = self.calculate_amplitude(&mic.samples);
            if mic_amplitude > self.config.ducking_threshold {
                desktop_vol * (1.0 - self.config.ducking_amount)
            } else {
                desktop_vol
            }
        } else {
            desktop_vol
        };

        // Mix samples - use the longer frame's length
        let max_len = desktop.samples.len().max(mic.samples.len());
        let mut mixed = Vec::with_capacity(max_len);

        for i in 0..max_len {
            let d = desktop.samples.get(i).copied().unwrap_or(0.0) * effective_desktop_vol;
            let m = mic.samples.get(i).copied().unwrap_or(0.0) * mic_vol;

            // Simple additive mixing with soft clipping
            let sum = d + m;
            let clipped = soft_clip(sum);
            mixed.push(clipped);
        }

        AudioFrame {
            format: AudioFormat {
                sample_rate: self.config.output_sample_rate,
                channels: self.config.output_channels,
                format: AudioSampleFormat::F32LE,
            },
            samples: mixed,
            pts: desktop.pts.min(mic.pts),
            sample_count: (max_len / self.config.output_channels as usize) as u32,
        }
    }

    /// Apply volume to audio frame
    fn apply_volume(&self, frame: &AudioFrame, volume: f32) -> AudioFrame {
        let samples = if (volume - 1.0).abs() < 0.001 {
            // Volume is ~1.0, no need to modify
            frame.samples.clone()
        } else {
            frame.samples.iter().map(|s| soft_clip(s * volume)).collect()
        };

        AudioFrame {
            format: frame.format.clone(),
            samples,
            pts: frame.pts,
            sample_count: frame.sample_count,
        }
    }

    /// Calculate RMS amplitude of samples
    fn calculate_amplitude(&self, samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum: f32 = samples.iter().map(|s| s * s).sum();
        (sum / samples.len() as f32).sqrt()
    }

    /// Stop the mixer
    pub fn stop(&mut self) {
        info!("Stopping audio mixer");
        self.running.store(false, Ordering::SeqCst);

        if let Some(ref mut stream) = self.desktop_stream {
            stream.stop();
        }
        if let Some(ref mut stream) = self.mic_stream {
            stream.stop();
        }
    }

    /// Check if mixer is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Set desktop volume
    pub fn set_desktop_volume(&mut self, volume: f32) {
        self.config.desktop_volume.volume = volume.max(0.0);
        debug!("Desktop volume set to {}", volume);
    }

    /// Set microphone volume
    pub fn set_mic_volume(&mut self, volume: f32) {
        self.config.mic_volume.volume = volume.max(0.0);
        debug!("Mic volume set to {}", volume);
    }

    /// Mute/unmute desktop
    pub fn set_desktop_muted(&mut self, muted: bool) {
        self.config.desktop_volume.muted = muted;
        debug!("Desktop muted: {}", muted);
    }

    /// Mute/unmute microphone
    pub fn set_mic_muted(&mut self, muted: bool) {
        self.config.mic_volume.muted = muted;
        debug!("Mic muted: {}", muted);
    }

    /// Get frame count
    pub fn frame_count(&self) -> u64 {
        self.frame_count.load(Ordering::Relaxed)
    }

    /// Get configuration
    pub fn config(&self) -> &MixerConfig {
        &self.config
    }
}

impl Drop for AudioMixer {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Soft clip function to prevent harsh clipping using tanh
fn soft_clip(x: f32) -> f32 {
    // Use tanh for smooth soft clipping
    // This maps any input to (-1, 1) range smoothly
    if x.abs() <= 0.5 {
        x // Linear region for low levels
    } else {
        x.tanh()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volume_control_default() {
        let vol = VolumeControl::default();
        assert_eq!(vol.volume, 1.0);
        assert!(!vol.muted);
        assert_eq!(vol.effective_volume(), 1.0);
    }

    #[test]
    fn test_volume_control_muted() {
        let vol = VolumeControl {
            volume: 1.0,
            muted: true,
        };
        assert_eq!(vol.effective_volume(), 0.0);
    }

    #[test]
    fn test_mixer_config_default() {
        let config = MixerConfig::default();
        assert_eq!(config.output_sample_rate, 48000);
        assert_eq!(config.output_channels, 2);
        assert!(!config.ducking_enabled);
    }

    #[test]
    fn test_soft_clip() {
        // Linear region
        assert_eq!(soft_clip(0.3), 0.3);
        assert_eq!(soft_clip(-0.3), -0.3);
        // Soft clipping region (uses tanh)
        let clipped = soft_clip(2.0);
        assert!(clipped < 1.0, "soft_clip(2.0) = {} should be < 1.0", clipped);
        assert!(clipped > 0.9, "soft_clip(2.0) = {} should be > 0.9", clipped);
        let neg_clipped = soft_clip(-2.0);
        assert!(neg_clipped > -1.0, "soft_clip(-2.0) = {} should be > -1.0", neg_clipped);
        assert!(neg_clipped < -0.9, "soft_clip(-2.0) = {} should be < -0.9", neg_clipped);
    }
}

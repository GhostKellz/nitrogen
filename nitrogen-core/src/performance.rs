//! Performance metrics and latency tracking for Nitrogen
//!
//! Provides:
//! - Frame time tracking (capture, encode, output stages)
//! - Rolling averages for latency statistics
//! - Dropped frame counting
//! - GPU monitoring (temperature, power, utilization)

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::RwLock;

/// Maximum number of samples to keep for rolling averages
const MAX_SAMPLES: usize = 120;

/// Latency statistics snapshot
#[derive(Debug, Clone)]
pub struct LatencyStats {
    /// Average capture latency in milliseconds
    pub capture_latency_ms: f64,
    /// Average encode latency in milliseconds
    pub encode_latency_ms: f64,
    /// Average output latency in milliseconds
    pub output_latency_ms: f64,
    /// Total end-to-end latency in milliseconds
    pub total_latency_ms: f64,
    /// Current frames per second
    pub fps: f64,
    /// Current bitrate in kbps (if available)
    pub bitrate_kbps: u64,
    /// Total frames processed
    pub frames_processed: u64,
    /// Total frames dropped
    pub frames_dropped: u64,
    /// Timestamp of this snapshot
    pub timestamp: Instant,
}

impl Default for LatencyStats {
    fn default() -> Self {
        Self {
            capture_latency_ms: 0.0,
            encode_latency_ms: 0.0,
            output_latency_ms: 0.0,
            total_latency_ms: 0.0,
            fps: 0.0,
            bitrate_kbps: 0,
            frames_processed: 0,
            frames_dropped: 0,
            timestamp: Instant::now(),
        }
    }
}

impl LatencyStats {
    /// Format stats as a single-line string for overlay
    pub fn format_overlay(&self) -> String {
        format!(
            "Capture: {:.1}ms | Encode: {:.1}ms | Total: {:.1}ms | {:.1}fps | Drops: {}",
            self.capture_latency_ms,
            self.encode_latency_ms,
            self.total_latency_ms,
            self.fps,
            self.frames_dropped
        )
    }

    /// Format stats as multi-line string for logging
    pub fn format_detailed(&self) -> String {
        format!(
            "Latency: capture={:.2}ms encode={:.2}ms output={:.2}ms total={:.2}ms\n\
             Performance: fps={:.1} bitrate={}kbps processed={} dropped={}",
            self.capture_latency_ms,
            self.encode_latency_ms,
            self.output_latency_ms,
            self.total_latency_ms,
            self.fps,
            self.bitrate_kbps,
            self.frames_processed,
            self.frames_dropped
        )
    }
}

/// Rolling average calculator for timing data
#[derive(Debug)]
struct RollingAverage {
    samples: VecDeque<Duration>,
    max_samples: usize,
}

impl RollingAverage {
    fn new(max_samples: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(max_samples),
            max_samples,
        }
    }

    fn add(&mut self, duration: Duration) {
        if self.samples.len() >= self.max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back(duration);
    }

    fn average(&self) -> Duration {
        if self.samples.is_empty() {
            return Duration::ZERO;
        }
        let total: Duration = self.samples.iter().sum();
        total / self.samples.len() as u32
    }

    fn average_ms(&self) -> f64 {
        self.average().as_secs_f64() * 1000.0
    }

    fn clear(&mut self) {
        self.samples.clear();
    }
}

/// Performance metrics collector
///
/// Thread-safe metrics collection for all pipeline stages.
#[derive(Debug)]
pub struct PerformanceMetrics {
    /// Capture stage latency samples
    capture_latency: RwLock<RollingAverage>,
    /// Encode stage latency samples
    encode_latency: RwLock<RollingAverage>,
    /// Output stage latency samples
    output_latency: RwLock<RollingAverage>,
    /// Frame time samples (for FPS calculation)
    frame_times: RwLock<RollingAverage>,
    /// Total frames processed
    frames_processed: AtomicU64,
    /// Total frames dropped
    frames_dropped: AtomicU64,
    /// Total bytes encoded (for bitrate calculation)
    bytes_encoded: AtomicU64,
    /// Last bitrate calculation time
    last_bitrate_time: RwLock<Instant>,
    /// Last bitrate value in kbps
    last_bitrate_kbps: AtomicU64,
    /// Start time for session
    start_time: Instant,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceMetrics {
    /// Create a new metrics collector
    pub fn new() -> Self {
        Self {
            capture_latency: RwLock::new(RollingAverage::new(MAX_SAMPLES)),
            encode_latency: RwLock::new(RollingAverage::new(MAX_SAMPLES)),
            output_latency: RwLock::new(RollingAverage::new(MAX_SAMPLES)),
            frame_times: RwLock::new(RollingAverage::new(MAX_SAMPLES)),
            frames_processed: AtomicU64::new(0),
            frames_dropped: AtomicU64::new(0),
            bytes_encoded: AtomicU64::new(0),
            last_bitrate_time: RwLock::new(Instant::now()),
            last_bitrate_kbps: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    /// Record capture stage timing
    pub fn record_capture(&self, duration: Duration) {
        self.capture_latency.write().add(duration);
    }

    /// Record capture stage timing from start/end instants
    pub fn record_capture_timing(&self, start: Instant, end: Instant) {
        self.record_capture(end.duration_since(start));
    }

    /// Record encode stage timing
    pub fn record_encode(&self, duration: Duration) {
        self.encode_latency.write().add(duration);
    }

    /// Record encode stage timing from start/end instants
    pub fn record_encode_timing(&self, start: Instant, end: Instant) {
        self.record_encode(end.duration_since(start));
    }

    /// Record output stage timing
    pub fn record_output(&self, duration: Duration) {
        self.output_latency.write().add(duration);
    }

    /// Record output stage timing from start/end instants
    pub fn record_output_timing(&self, start: Instant, end: Instant) {
        self.record_output(end.duration_since(start));
    }

    /// Record a complete frame time (for FPS calculation)
    pub fn record_frame_time(&self, duration: Duration) {
        self.frame_times.write().add(duration);
        self.frames_processed.fetch_add(1, Ordering::Relaxed);
    }

    /// Record frame processing completion
    pub fn record_frame_processed(&self) {
        self.frames_processed.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a dropped frame
    pub fn record_frame_dropped(&self) {
        self.frames_dropped.fetch_add(1, Ordering::Relaxed);
    }

    /// Record encoded bytes (for bitrate calculation)
    pub fn record_bytes_encoded(&self, bytes: u64) {
        self.bytes_encoded.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Get current latency statistics
    pub fn get_stats(&self) -> LatencyStats {
        let capture_ms = self.capture_latency.read().average_ms();
        let encode_ms = self.encode_latency.read().average_ms();
        let output_ms = self.output_latency.read().average_ms();

        // Calculate FPS from frame times
        let avg_frame_time = self.frame_times.read().average();
        let fps = if avg_frame_time.as_secs_f64() > 0.0 {
            1.0 / avg_frame_time.as_secs_f64()
        } else {
            0.0
        };

        // Calculate bitrate
        let bitrate_kbps = self.calculate_bitrate();

        LatencyStats {
            capture_latency_ms: capture_ms,
            encode_latency_ms: encode_ms,
            output_latency_ms: output_ms,
            total_latency_ms: capture_ms + encode_ms + output_ms,
            fps,
            bitrate_kbps,
            frames_processed: self.frames_processed.load(Ordering::Relaxed),
            frames_dropped: self.frames_dropped.load(Ordering::Relaxed),
            timestamp: Instant::now(),
        }
    }

    /// Calculate current bitrate in kbps
    fn calculate_bitrate(&self) -> u64 {
        let now = Instant::now();
        let mut last_time = self.last_bitrate_time.write();
        let elapsed = now.duration_since(*last_time);

        // Only recalculate every 500ms
        if elapsed.as_millis() >= 500 {
            let bytes = self.bytes_encoded.swap(0, Ordering::Relaxed);
            let bits = bytes * 8;
            let kbps = (bits as f64 / elapsed.as_secs_f64() / 1000.0) as u64;
            self.last_bitrate_kbps.store(kbps, Ordering::Relaxed);
            *last_time = now;
        }

        self.last_bitrate_kbps.load(Ordering::Relaxed)
    }

    /// Get total frames processed
    pub fn frames_processed(&self) -> u64 {
        self.frames_processed.load(Ordering::Relaxed)
    }

    /// Get total frames dropped
    pub fn frames_dropped(&self) -> u64 {
        self.frames_dropped.load(Ordering::Relaxed)
    }

    /// Get session duration
    pub fn session_duration(&self) -> Duration {
        Instant::now().duration_since(self.start_time)
    }

    /// Reset all metrics
    pub fn reset(&self) {
        self.capture_latency.write().clear();
        self.encode_latency.write().clear();
        self.output_latency.write().clear();
        self.frame_times.write().clear();
        self.frames_processed.store(0, Ordering::Relaxed);
        self.frames_dropped.store(0, Ordering::Relaxed);
        self.bytes_encoded.store(0, Ordering::Relaxed);
        self.last_bitrate_kbps.store(0, Ordering::Relaxed);
        *self.last_bitrate_time.write() = Instant::now();
    }
}

/// GPU monitoring information
#[derive(Debug, Clone, Default)]
pub struct GpuStats {
    /// GPU temperature in Celsius
    pub temperature: u32,
    /// GPU power usage in Watts
    pub power_watts: u32,
    /// GPU utilization percentage (0-100)
    pub utilization: u32,
    /// VRAM used in MB
    pub vram_used_mb: u64,
    /// VRAM total in MB
    pub vram_total_mb: u64,
    /// Encoder utilization percentage
    pub encoder_utilization: u32,
}

impl GpuStats {
    /// Format as single-line string
    pub fn format_line(&self) -> String {
        format!(
            "GPU: {}°C {}W {}% | VRAM: {}/{}MB | Enc: {}%",
            self.temperature,
            self.power_watts,
            self.utilization,
            self.vram_used_mb,
            self.vram_total_mb,
            self.encoder_utilization
        )
    }
}

/// Query GPU statistics using nvidia-smi
pub fn query_gpu_stats(gpu_index: u32) -> Option<GpuStats> {
    let output = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=temperature.gpu,power.draw,utilization.gpu,memory.used,memory.total,utilization.encoder",
            "--format=csv,noheader,nounits",
            &format!("--id={}", gpu_index),
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parts: Vec<&str> = stdout.trim().split(", ").collect();

    if parts.len() >= 6 {
        Some(GpuStats {
            temperature: parts[0].trim().parse().unwrap_or(0),
            power_watts: parts[1].trim().parse::<f32>().unwrap_or(0.0) as u32,
            utilization: parts[2].trim().parse().unwrap_or(0),
            vram_used_mb: parts[3].trim().parse().unwrap_or(0),
            vram_total_mb: parts[4].trim().parse().unwrap_or(0),
            encoder_utilization: parts[5].trim().parse().unwrap_or(0),
        })
    } else {
        None
    }
}

/// Create a shared performance metrics instance
pub fn create_metrics() -> Arc<PerformanceMetrics> {
    Arc::new(PerformanceMetrics::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rolling_average() {
        let mut avg = RollingAverage::new(3);
        avg.add(Duration::from_millis(10));
        avg.add(Duration::from_millis(20));
        avg.add(Duration::from_millis(30));

        // Average of 10, 20, 30 = 20
        assert!((avg.average_ms() - 20.0).abs() < 0.1);

        // Add one more, should drop oldest
        avg.add(Duration::from_millis(40));
        // Average of 20, 30, 40 = 30
        assert!((avg.average_ms() - 30.0).abs() < 0.1);
    }

    #[test]
    fn test_performance_metrics() {
        let metrics = PerformanceMetrics::new();

        metrics.record_capture(Duration::from_millis(5));
        metrics.record_encode(Duration::from_millis(10));
        metrics.record_output(Duration::from_millis(2));
        metrics.record_frame_processed();

        let stats = metrics.get_stats();
        assert!(stats.capture_latency_ms > 0.0);
        assert!(stats.encode_latency_ms > 0.0);
        assert_eq!(stats.frames_processed, 1);
        assert_eq!(stats.frames_dropped, 0);
    }

    #[test]
    fn test_frame_dropping() {
        let metrics = PerformanceMetrics::new();

        metrics.record_frame_dropped();
        metrics.record_frame_dropped();
        metrics.record_frame_processed();

        assert_eq!(metrics.frames_dropped(), 2);
        assert_eq!(metrics.frames_processed(), 1);
    }

    #[test]
    fn test_stats_formatting() {
        let stats = LatencyStats {
            capture_latency_ms: 2.5,
            encode_latency_ms: 5.0,
            output_latency_ms: 1.0,
            total_latency_ms: 8.5,
            fps: 60.0,
            bitrate_kbps: 6000,
            frames_processed: 1000,
            frames_dropped: 5,
            timestamp: Instant::now(),
        };

        let overlay = stats.format_overlay();
        assert!(overlay.contains("2.5ms"));
        assert!(overlay.contains("60.0fps"));
        assert!(overlay.contains("Drops: 5"));
    }

    #[test]
    fn test_reset() {
        let metrics = PerformanceMetrics::new();
        metrics.record_frame_processed();
        metrics.record_frame_dropped();

        metrics.reset();

        assert_eq!(metrics.frames_processed(), 0);
        assert_eq!(metrics.frames_dropped(), 0);
    }
}

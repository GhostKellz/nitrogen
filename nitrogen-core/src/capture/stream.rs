//! PipeWire stream handling for video capture
//!
//! Connects to the PipeWire graph and receives video frames from
//! the screencast portal session.

use pipewire as pw;
use pw::spa::param::format::{MediaSubtype, MediaType};
use pw::spa::param::format_utils;
use pw::spa::param::video::VideoFormat;
use pw::spa::pod::Pod;
use pw::spa::utils::{Direction, Fraction, Rectangle};
use pw::stream::{Stream, StreamFlags, StreamState};

use std::os::fd::OwnedFd;
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, trace, warn};

use crate::error::{NitrogenError, Result};
use crate::types::{Frame, FrameData, FrameFormat};

/// Shared state between PipeWire thread and main thread
struct SharedState {
    /// Video format information
    format: parking_lot::Mutex<Option<VideoInfo>>,
    /// Frame counter
    frame_count: AtomicU64,
    /// Whether the stream is running
    running: AtomicBool,
}

/// Parsed video format information
#[derive(Debug, Clone, Copy)]
struct VideoInfo {
    format: VideoFormat,
    width: u32,
    height: u32,
    framerate_num: u32,
    framerate_denom: u32,
}

impl VideoInfo {
    /// Convert SPA VideoFormat to DRM fourcc
    fn to_fourcc(&self) -> u32 {
        match self.format {
            VideoFormat::BGRx => 0x34325258, // XR24
            VideoFormat::BGRA => 0x34324142, // AB24
            VideoFormat::RGBx => 0x34325842, // BX24
            VideoFormat::RGBA => 0x34324241, // BA24
            VideoFormat::xRGB => 0x34325258, // XR24 (same memory layout as BGRx on LE)
            VideoFormat::ARGB => 0x34325241, // AR24
            VideoFormat::xBGR => 0x34324258, // XB24
            VideoFormat::ABGR => 0x34324241, // BA24
            VideoFormat::RGB => 0x20424752,  // RGB
            VideoFormat::BGR => 0x20524742,  // BGR
            VideoFormat::YUY2 => 0x56595559, // YUYV
            VideoFormat::NV12 => 0x3231564E, // NV12
            _ => 0x34325258,                 // Default to XR24
        }
    }
}

/// PipeWire capture stream
///
/// Receives video frames from a screencast portal session via PipeWire.
pub struct CaptureStream {
    /// Sender for frames to the processing pipeline
    frame_tx: broadcast::Sender<Arc<Frame>>,
    /// Thread handle for the PipeWire main loop
    pw_thread: Option<std::thread::JoinHandle<()>>,
    /// Channel to signal shutdown
    shutdown_tx: Option<mpsc::Sender<()>>,
    /// Shared state
    shared: Arc<SharedState>,
}

impl CaptureStream {
    /// Create a new capture stream
    ///
    /// # Arguments
    /// * `fd` - PipeWire file descriptor from the portal
    /// * `node_id` - PipeWire node ID to connect to
    pub fn new(fd: OwnedFd, node_id: u32) -> Result<Self> {
        let (frame_tx, _) = broadcast::channel(4); // Small buffer for low latency
        let (shutdown_tx, shutdown_rx) = mpsc::channel();

        let shared = Arc::new(SharedState {
            format: parking_lot::Mutex::new(None),
            frame_count: AtomicU64::new(0),
            running: AtomicBool::new(false),
        });

        // Clone what we need for the thread
        let frame_tx_clone = frame_tx.clone();
        let shared_clone = shared.clone();
        let raw_fd = fd.as_raw_fd();

        // Spawn PipeWire thread
        let pw_thread = std::thread::Builder::new()
            .name("nitrogen-pipewire".to_string())
            .spawn(move || {
                // Take ownership of the fd in this thread
                let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
                if let Err(e) =
                    run_pipewire_loop(fd, node_id, frame_tx_clone, shutdown_rx, shared_clone)
                {
                    error!("PipeWire loop error: {}", e);
                }
            })
            .map_err(|e| {
                NitrogenError::pipewire(format!("Failed to spawn PipeWire thread: {}", e))
            })?;

        // Forget the original fd since the thread now owns it
        std::mem::forget(fd);

        Ok(Self {
            frame_tx,
            pw_thread: Some(pw_thread),
            shutdown_tx: Some(shutdown_tx),
            shared,
        })
    }

    /// Subscribe to frames from this stream
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<Frame>> {
        self.frame_tx.subscribe()
    }

    /// Check if the stream is still running
    pub fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::SeqCst)
            && self
                .pw_thread
                .as_ref()
                .map(|t| !t.is_finished())
                .unwrap_or(false)
    }

    /// Get the current frame count
    pub fn frame_count(&self) -> u64 {
        self.shared.frame_count.load(Ordering::Relaxed)
    }

    /// Get the current video format if known
    pub fn format(&self) -> Option<(u32, u32, u32)> {
        self.shared
            .format
            .lock()
            .map(|f| (f.width, f.height, f.framerate_num))
    }

    /// Stop the capture stream
    pub fn stop(&mut self) {
        info!("Stopping capture stream");

        // Signal shutdown
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        // Wait for thread to finish
        if let Some(thread) = self.pw_thread.take() {
            // Give the thread a moment to clean up
            let _ = thread.join();
        }

        self.shared.running.store(false, Ordering::SeqCst);
        info!("Capture stream stopped");
    }
}

impl Drop for CaptureStream {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Run the PipeWire main loop (called from dedicated thread)
fn run_pipewire_loop(
    fd: OwnedFd,
    node_id: u32,
    frame_tx: broadcast::Sender<Arc<Frame>>,
    shutdown_rx: mpsc::Receiver<()>,
    shared: Arc<SharedState>,
) -> Result<()> {
    // Initialize PipeWire
    pw::init();

    info!("Initializing PipeWire capture for node {}", node_id);

    // Create main loop
    let mainloop = pw::main_loop::MainLoop::new(None)
        .map_err(|e| NitrogenError::pipewire(format!("Failed to create main loop: {}", e)))?;

    let loop_ = mainloop.loop_();

    // Create context
    let context = pw::context::Context::new(&mainloop)
        .map_err(|e| NitrogenError::pipewire(format!("Failed to create context: {}", e)))?;

    // Connect using the portal's file descriptor
    let core = context
        .connect_fd(fd, None)
        .map_err(|e| NitrogenError::pipewire(format!("Failed to connect to PipeWire fd: {}", e)))?;

    // User data for callbacks
    struct UserData {
        format: Option<VideoInfo>,
        frame_tx: broadcast::Sender<Arc<Frame>>,
        shared: Arc<SharedState>,
    }

    let user_data = UserData {
        format: None,
        frame_tx,
        shared: shared.clone(),
    };

    // Create stream
    let stream = Stream::new(
        &core,
        "nitrogen-capture",
        pw::properties::properties! {
            *pw::keys::MEDIA_TYPE => "Video",
            *pw::keys::MEDIA_CATEGORY => "Capture",
            *pw::keys::MEDIA_ROLE => "Screen",
        },
    )
    .map_err(|e| NitrogenError::pipewire(format!("Failed to create stream: {}", e)))?;

    // Set up stream listener
    let _listener = stream
        .add_local_listener_with_user_data(user_data)
        .state_changed(|_, user_data, old, new| {
            debug!("Stream state changed: {:?} -> {:?}", old, new);
            match new {
                StreamState::Streaming => {
                    info!("PipeWire stream is now streaming");
                    user_data.shared.running.store(true, Ordering::SeqCst);
                }
                StreamState::Error(msg) => {
                    // Check for common window/source-closed error patterns
                    let msg_lower = msg.to_lowercase();
                    if msg_lower.contains("no source")
                        || msg_lower.contains("removed")
                        || msg_lower.contains("closed")
                        || msg_lower.contains("destroyed")
                    {
                        error!("Capture source was closed or removed: {}", msg);
                    } else {
                        error!("Stream error: {}", msg);
                    }
                    user_data.shared.running.store(false, Ordering::SeqCst);
                }
                StreamState::Paused => {
                    // Source may be minimized or occluded
                    warn!("Stream paused - capture source may be minimized or hidden");
                }
                StreamState::Unconnected => {
                    info!("Stream disconnected");
                    user_data.shared.running.store(false, Ordering::SeqCst);
                }
                _ => {}
            }
        })
        .param_changed(|_, user_data, id, param| {
            // Only handle format changes
            let Some(param) = param else { return };
            if id != pw::spa::param::ParamType::Format.as_raw() {
                return;
            }

            // Parse media type
            let (media_type, media_subtype) = match format_utils::parse_format(param) {
                Ok(v) => v,
                Err(_) => return,
            };

            if media_type != MediaType::Video || media_subtype != MediaSubtype::Raw {
                return;
            }

            // Parse video format
            let mut video_info = pw::spa::param::video::VideoInfoRaw::new();
            if video_info.parse(param).is_err() {
                warn!("Failed to parse video format");
                return;
            }

            let info = VideoInfo {
                format: video_info.format(),
                width: video_info.size().width,
                height: video_info.size().height,
                framerate_num: video_info.framerate().num,
                framerate_denom: video_info.framerate().denom,
            };

            info!(
                "Video format negotiated: {:?} {}x{} @ {}/{}fps",
                info.format, info.width, info.height, info.framerate_num, info.framerate_denom
            );

            user_data.format = Some(info);
            *user_data.shared.format.lock() = Some(info);
        })
        .process(|stream, user_data| {
            // Dequeue buffer
            let Some(mut buffer) = stream.dequeue_buffer() else {
                trace!("No buffer available");
                return;
            };

            let Some(format) = user_data.format else {
                trace!("No format yet, skipping frame");
                return;
            };

            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }

            let data = &mut datas[0];

            // Get chunk info first (immutable borrow)
            let chunk_size = data.chunk().size() as usize;
            let chunk_stride = data.chunk().stride() as u32;

            if chunk_size == 0 {
                return;
            }

            // Now get frame data (mutable borrow)
            if let Some(slice) = data.data() {
                // Copy frame data (we need to copy since the buffer is returned)
                let frame_data = slice[..chunk_size.min(slice.len())].to_vec();

                let frame = Frame {
                    format: FrameFormat {
                        width: format.width,
                        height: format.height,
                        fourcc: format.to_fourcc(),
                        stride: chunk_stride,
                    },
                    data: FrameData::Memory(frame_data),
                    pts: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos() as u64)
                        .unwrap_or(0),
                };

                // Send frame
                let count = user_data.shared.frame_count.fetch_add(1, Ordering::Relaxed);
                if user_data.frame_tx.send(Arc::new(frame)).is_err() {
                    // No receivers, that's okay
                }

                if count % 60 == 0 {
                    trace!("Captured {} frames", count + 1);
                }
            }
        })
        .register()
        .map_err(|e| NitrogenError::pipewire(format!("Failed to register listener: {}", e)))?;

    // Build format parameters - accept common video formats
    let obj = pw::spa::pod::object!(
        pw::spa::utils::SpaTypes::ObjectParamFormat,
        pw::spa::param::ParamType::EnumFormat,
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::MediaType,
            Id,
            MediaType::Video
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::MediaSubtype,
            Id,
            MediaSubtype::Raw
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::VideoFormat,
            Choice,
            Enum,
            Id,
            VideoFormat::BGRx,
            VideoFormat::BGRx,
            VideoFormat::BGRA,
            VideoFormat::RGBx,
            VideoFormat::RGBA,
            VideoFormat::xRGB,
            VideoFormat::ARGB,
            VideoFormat::xBGR,
            VideoFormat::ABGR,
            VideoFormat::RGB,
            VideoFormat::BGR
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            Rectangle {
                width: 1920,
                height: 1080
            },
            Rectangle {
                width: 1,
                height: 1
            },
            Rectangle {
                width: 8192,
                height: 8192
            }
        ),
        pw::spa::pod::property!(
            pw::spa::param::format::FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            Fraction { num: 60, denom: 1 },
            Fraction { num: 0, denom: 1 },
            Fraction { num: 240, denom: 1 }
        ),
    );

    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(obj),
    )
    .map_err(|e| NitrogenError::pipewire(format!("Failed to serialize format: {:?}", e)))?
    .0
    .into_inner();

    let pod = Pod::from_bytes(&values)
        .ok_or_else(|| NitrogenError::pipewire("Failed to create Pod from serialized format"))?;
    let mut params = [pod];

    // Connect stream to the portal's node
    stream
        .connect(
            Direction::Input,
            Some(node_id),
            StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .map_err(|e| NitrogenError::pipewire(format!("Failed to connect stream: {}", e)))?;

    info!("PipeWire stream connected to node {}", node_id);

    // Add a source to check for shutdown
    let mainloop_weak = mainloop.downgrade();
    let _source = loop_.add_idle(true, move || {
        // Check for shutdown signal (non-blocking)
        if shutdown_rx.try_recv().is_ok() {
            info!("Shutdown signal received");
            if let Some(mainloop) = mainloop_weak.upgrade() {
                mainloop.quit();
            }
            return;
        }
    });

    // Run the main loop
    mainloop.run();

    info!("PipeWire main loop ended");
    shared.running.store(false, Ordering::SeqCst);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_info_fourcc() {
        let info = VideoInfo {
            format: VideoFormat::BGRx,
            width: 1920,
            height: 1080,
            framerate_num: 60,
            framerate_denom: 1,
        };
        assert_eq!(info.to_fourcc(), 0x34325258);
    }
}

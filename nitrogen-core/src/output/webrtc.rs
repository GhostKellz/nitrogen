//! WebRTC output for Nitrogen
//!
//! Provides WebRTC streaming output for browser-based viewing.
//! Uses the `webrtc` crate for peer-to-peer video streaming.
//!
//! ## Signaling Server
//!
//! Includes a built-in HTTP signaling server for easy browser-based viewing:
//! - `GET /` - Simple HTML viewer page with WebRTC client
//! - `GET /offer` - Returns SDP offer as JSON
//! - `POST /answer` - Accepts SDP answer as JSON
//! - `GET /status` - Connection status

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};

use axum::{
    extract::State,
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};

use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_H264, MIME_TYPE_OPUS};
use webrtc::api::APIBuilder;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;
use webrtc::media::Sample;

use crate::encode::EncodedPacket;
use crate::error::{NitrogenError, Result};

/// WebRTC output configuration
#[derive(Debug, Clone)]
pub struct WebRTCConfig {
    /// ICE servers for NAT traversal
    pub ice_servers: Vec<String>,
    /// Video codec (h264 recommended for compatibility)
    pub video_codec: String,
    /// Video payload type
    pub video_payload_type: u8,
    /// Audio enabled
    pub audio_enabled: bool,
}

impl Default for WebRTCConfig {
    fn default() -> Self {
        Self {
            ice_servers: vec!["stun:stun.l.google.com:19302".to_string()],
            video_codec: "h264".to_string(),
            video_payload_type: 96,
            audio_enabled: true,
        }
    }
}

/// WebRTC output sink
///
/// Streams encoded video/audio over WebRTC to connected peers.
pub struct WebRTCOutput {
    /// Configuration
    config: WebRTCConfig,
    /// Peer connection
    peer_connection: Option<Arc<RTCPeerConnection>>,
    /// Video track
    video_track: Option<Arc<TrackLocalStaticSample>>,
    /// Audio track (if enabled)
    audio_track: Option<Arc<TrackLocalStaticSample>>,
    /// Running flag
    running: AtomicBool,
}

impl WebRTCOutput {
    /// Create a new WebRTC output with the given configuration
    pub async fn new(config: WebRTCConfig) -> Result<Self> {
        info!("Creating WebRTC output");
        debug!("ICE servers: {:?}", config.ice_servers);

        Ok(Self {
            config,
            peer_connection: None,
            video_track: None,
            audio_track: None,
            running: AtomicBool::new(false),
        })
    }

    /// Initialize the peer connection
    pub async fn init(&mut self) -> Result<()> {
        // Create media engine
        let mut media_engine = MediaEngine::default();

        // Register H264 codec
        media_engine
            .register_default_codecs()
            .map_err(|e| NitrogenError::webrtc(format!("Failed to register codecs: {}", e)))?;

        // Create interceptor registry
        let mut registry = Registry::new();
        registry = register_default_interceptors(registry, &mut media_engine)
            .map_err(|e| NitrogenError::webrtc(format!("Failed to register interceptors: {}", e)))?;

        // Create API
        let api = APIBuilder::new()
            .with_media_engine(media_engine)
            .with_interceptor_registry(registry)
            .build();

        // Create peer connection configuration
        let ice_servers: Vec<RTCIceServer> = self
            .config
            .ice_servers
            .iter()
            .map(|url| RTCIceServer {
                urls: vec![url.clone()],
                ..Default::default()
            })
            .collect();

        let rtc_config = RTCConfiguration {
            ice_servers,
            ..Default::default()
        };

        // Create peer connection
        let peer_connection = api
            .new_peer_connection(rtc_config)
            .await
            .map_err(|e| NitrogenError::webrtc(format!("Failed to create peer connection: {}", e)))?;

        let peer_connection = Arc::new(peer_connection);

        // Create video track
        let video_track = Arc::new(TrackLocalStaticSample::new(
            RTCRtpCodecCapability {
                mime_type: MIME_TYPE_H264.to_string(),
                ..Default::default()
            },
            "video".to_string(),
            "nitrogen-video".to_string(),
        ));

        // Add video track to peer connection
        let rtp_sender = peer_connection
            .add_track(video_track.clone() as Arc<dyn TrackLocal + Send + Sync>)
            .await
            .map_err(|e| NitrogenError::webrtc(format!("Failed to add video track: {}", e)))?;

        // Spawn RTCP reader
        tokio::spawn(async move {
            let mut rtcp_buf = vec![0u8; 1500];
            while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
        });

        self.video_track = Some(video_track);

        // Create audio track if enabled
        if self.config.audio_enabled {
            let audio_track = Arc::new(TrackLocalStaticSample::new(
                RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_OPUS.to_string(),
                    ..Default::default()
                },
                "audio".to_string(),
                "nitrogen-audio".to_string(),
            ));

            let audio_rtp_sender = peer_connection
                .add_track(audio_track.clone() as Arc<dyn TrackLocal + Send + Sync>)
                .await
                .map_err(|e| NitrogenError::webrtc(format!("Failed to add audio track: {}", e)))?;

            // Spawn RTCP reader for audio
            tokio::spawn(async move {
                let mut rtcp_buf = vec![0u8; 1500];
                while let Ok((_, _)) = audio_rtp_sender.read(&mut rtcp_buf).await {}
            });

            self.audio_track = Some(audio_track);
        }

        // Set up connection state callback
        peer_connection.on_peer_connection_state_change(Box::new(move |state: RTCPeerConnectionState| {
            info!("WebRTC peer connection state: {:?}", state);

            if state == RTCPeerConnectionState::Failed {
                error!("WebRTC peer connection failed");
            }

            Box::pin(async {})
        }));

        self.peer_connection = Some(peer_connection);
        self.running.store(true, Ordering::SeqCst);

        info!("WebRTC output initialized");
        Ok(())
    }

    /// Create an SDP offer for signaling
    pub async fn create_offer(&self) -> Result<String> {
        let pc = self.peer_connection.as_ref()
            .ok_or_else(|| NitrogenError::webrtc("Peer connection not initialized".to_string()))?;

        let offer = pc
            .create_offer(None)
            .await
            .map_err(|e| NitrogenError::webrtc(format!("Failed to create offer: {}", e)))?;

        pc.set_local_description(offer.clone())
            .await
            .map_err(|e| NitrogenError::webrtc(format!("Failed to set local description: {}", e)))?;

        Ok(offer.sdp)
    }

    /// Set the remote SDP answer from signaling
    pub async fn set_answer(&self, sdp: &str) -> Result<()> {
        let pc = self.peer_connection.as_ref()
            .ok_or_else(|| NitrogenError::webrtc("Peer connection not initialized".to_string()))?;

        let answer = RTCSessionDescription::answer(sdp.to_string())
            .map_err(|e| NitrogenError::webrtc(format!("Invalid SDP answer: {}", e)))?;

        pc.set_remote_description(answer)
            .await
            .map_err(|e| NitrogenError::webrtc(format!("Failed to set remote description: {}", e)))?;

        Ok(())
    }

    /// Run the WebRTC output, consuming encoded packets from the channel
    pub async fn run(&self, mut video_rx: broadcast::Receiver<Arc<EncodedPacket>>) -> Result<()> {
        let video_track = self.video_track.as_ref()
            .ok_or_else(|| NitrogenError::webrtc("Video track not initialized".to_string()))?;

        info!("WebRTC output started");

        while self.running.load(Ordering::SeqCst) {
            match video_rx.recv().await {
                Ok(packet) => {
                    // Convert encoded packet to RTP and send
                    if let Err(e) = self.send_video_packet(video_track, &packet).await {
                        warn!("Failed to send video packet: {}", e);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("WebRTC output lagged by {} frames", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("Video channel closed, stopping WebRTC output");
                    break;
                }
            }
        }

        Ok(())
    }

    /// Send a video packet over RTP
    async fn send_video_packet(
        &self,
        track: &Arc<TrackLocalStaticSample>,
        packet: &EncodedPacket,
    ) -> Result<()> {
        // TrackLocalStaticSample handles RTP packetization automatically
        // We just need to send the encoded H.264 frame data

        let sample = Sample {
            data: bytes::Bytes::copy_from_slice(&packet.data),
            duration: std::time::Duration::from_millis(33), // ~30fps default
            ..Default::default()
        };

        track
            .write_sample(&sample)
            .await
            .map_err(|e| NitrogenError::webrtc(format!("Failed to write sample: {}", e)))?;

        Ok(())
    }

    /// Stop the WebRTC output
    pub async fn stop(&mut self) -> Result<()> {
        info!("Stopping WebRTC output");
        self.running.store(false, Ordering::SeqCst);

        if let Some(pc) = self.peer_connection.take() {
            pc.close()
                .await
                .map_err(|e| NitrogenError::webrtc(format!("Failed to close connection: {}", e)))?;
        }

        Ok(())
    }

    /// Check if the output is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get the configuration
    pub fn config(&self) -> &WebRTCConfig {
        &self.config
    }
}

impl Drop for WebRTCOutput {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

// ============================================================================
// WebRTC Signaling Server
// ============================================================================

/// Shared state for the signaling server
struct SignalingState {
    webrtc: Arc<RwLock<WebRTCOutput>>,
}

/// Start the WebRTC signaling server
///
/// This starts a simple HTTP server that handles WebRTC signaling:
/// - `GET /` returns an HTML viewer page
/// - `GET /offer` returns the SDP offer
/// - `POST /answer` accepts the SDP answer
/// - `GET /status` returns connection status
pub async fn start_signaling_server(
    webrtc: Arc<RwLock<WebRTCOutput>>,
    port: u16,
) -> Result<()> {
    let state = Arc::new(SignalingState { webrtc });

    let app = Router::new()
        .route("/", get(viewer_page))
        .route("/offer", get(get_offer))
        .route("/answer", post(set_answer))
        .route("/status", get(get_status))
        .with_state(state);

    let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
    info!("WebRTC signaling server starting on http://localhost:{}", port);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| NitrogenError::webrtc(format!("Failed to bind signaling server: {}", e)))?;

    axum::serve(listener, app)
        .await
        .map_err(|e| NitrogenError::webrtc(format!("Signaling server error: {}", e)))?;

    Ok(())
}

/// HTML viewer page with embedded WebRTC client
async fn viewer_page() -> Html<&'static str> {
    Html(VIEWER_HTML)
}

/// Get SDP offer endpoint
async fn get_offer(
    State(state): State<Arc<SignalingState>>,
) -> impl IntoResponse {
    let webrtc = state.webrtc.read().await;

    match webrtc.create_offer().await {
        Ok(sdp) => (StatusCode::OK, Json(serde_json::json!({ "sdp": sdp }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// Answer request body
#[derive(serde::Deserialize)]
struct AnswerRequest {
    sdp: String,
}

/// Set SDP answer endpoint
async fn set_answer(
    State(state): State<Arc<SignalingState>>,
    Json(body): Json<AnswerRequest>,
) -> impl IntoResponse {
    let webrtc = state.webrtc.read().await;

    match webrtc.set_answer(&body.sdp).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "status": "ok" }))),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

/// Get connection status endpoint
async fn get_status(
    State(state): State<Arc<SignalingState>>,
) -> impl IntoResponse {
    let webrtc = state.webrtc.read().await;
    let running = webrtc.is_running();

    Json(serde_json::json!({
        "running": running,
        "video_enabled": webrtc.video_track.is_some(),
        "audio_enabled": webrtc.audio_track.is_some(),
    }))
}

/// Embedded HTML viewer page
const VIEWER_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Nitrogen WebRTC Viewer</title>
    <style>
        body {
            margin: 0;
            padding: 20px;
            background: #1a1a1a;
            color: #fff;
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
        }
        .container {
            max-width: 1280px;
            margin: 0 auto;
        }
        h1 {
            color: #7c3aed;
            margin-bottom: 20px;
        }
        video {
            width: 100%;
            max-width: 1280px;
            background: #000;
            border-radius: 8px;
        }
        .status {
            margin: 10px 0;
            padding: 10px;
            background: #2a2a2a;
            border-radius: 4px;
        }
        .connected { color: #22c55e; }
        .disconnected { color: #ef4444; }
        .connecting { color: #f59e0b; }
        button {
            background: #7c3aed;
            color: white;
            border: none;
            padding: 10px 20px;
            border-radius: 4px;
            cursor: pointer;
            font-size: 16px;
            margin-right: 10px;
        }
        button:hover { background: #6d28d9; }
        button:disabled { background: #4a4a4a; cursor: not-allowed; }
    </style>
</head>
<body>
    <div class="container">
        <h1>Nitrogen WebRTC Viewer</h1>
        <video id="video" autoplay playsinline muted></video>
        <div class="status">
            Status: <span id="status" class="disconnected">Disconnected</span>
        </div>
        <button id="connect" onclick="connect()">Connect</button>
        <button id="disconnect" onclick="disconnect()" disabled>Disconnect</button>
    </div>
    <script>
        let pc = null;
        const video = document.getElementById('video');
        const statusEl = document.getElementById('status');
        const connectBtn = document.getElementById('connect');
        const disconnectBtn = document.getElementById('disconnect');

        function setStatus(status, className) {
            statusEl.textContent = status;
            statusEl.className = className;
        }

        async function connect() {
            try {
                setStatus('Connecting...', 'connecting');
                connectBtn.disabled = true;

                // Create peer connection
                pc = new RTCPeerConnection({
                    iceServers: [{ urls: 'stun:stun.l.google.com:19302' }]
                });

                pc.ontrack = (event) => {
                    video.srcObject = event.streams[0];
                };

                pc.oniceconnectionstatechange = () => {
                    if (pc.iceConnectionState === 'connected') {
                        setStatus('Connected', 'connected');
                        disconnectBtn.disabled = false;
                    } else if (pc.iceConnectionState === 'disconnected' || pc.iceConnectionState === 'failed') {
                        setStatus('Disconnected', 'disconnected');
                        connectBtn.disabled = false;
                        disconnectBtn.disabled = true;
                    }
                };

                // Get offer from server
                const offerRes = await fetch('/offer');
                const offerData = await offerRes.json();

                if (offerData.error) {
                    throw new Error(offerData.error);
                }

                // Set remote description (server's offer)
                await pc.setRemoteDescription({
                    type: 'offer',
                    sdp: offerData.sdp
                });

                // Create and set local answer
                const answer = await pc.createAnswer();
                await pc.setLocalDescription(answer);

                // Send answer to server
                await fetch('/answer', {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ sdp: answer.sdp })
                });

            } catch (err) {
                console.error('Connection error:', err);
                setStatus('Error: ' + err.message, 'disconnected');
                connectBtn.disabled = false;
            }
        }

        function disconnect() {
            if (pc) {
                pc.close();
                pc = null;
            }
            video.srcObject = null;
            setStatus('Disconnected', 'disconnected');
            connectBtn.disabled = false;
            disconnectBtn.disabled = true;
        }
    </script>
</body>
</html>
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_webrtc_config_default() {
        let config = WebRTCConfig::default();
        assert!(!config.ice_servers.is_empty());
        assert_eq!(config.video_codec, "h264");
        assert!(config.audio_enabled);
    }

    #[tokio::test]
    async fn test_webrtc_output_creation() {
        let config = WebRTCConfig::default();
        let output = WebRTCOutput::new(config).await;
        assert!(output.is_ok());
    }
}

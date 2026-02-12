# WebRTC Browser Viewing

Nitrogen includes a built-in WebRTC server for browser-based viewing of your capture. This is useful for:

- Previewing your stream without OBS
- Sharing your screen over a local network
- Low-latency viewing without additional software

## Quick Start

```bash
# Enable WebRTC output
nitrogen cast --webrtc

# Then open in your browser:
# http://localhost:9000
```

## Configuration

### Port Selection

```bash
# Use a custom port
nitrogen cast --webrtc --webrtc-port 8080
```

### Combined with Other Outputs

WebRTC can run alongside other outputs:

```bash
# WebRTC + Discord virtual camera
nitrogen cast --webrtc --discord

# WebRTC + Recording
nitrogen cast --webrtc --record capture.mp4

# WebRTC + RTMP streaming
nitrogen cast --webrtc --stream rtmp://...
```

## How It Works

1. Nitrogen starts a local HTTP server for signaling
2. Your browser connects and performs WebRTC handshake
3. Video is streamed directly to your browser via peer-to-peer

### Architecture

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│  Screen Capture │────▶│  NVENC Encoder  │────▶│  WebRTC Output  │
└─────────────────┘     └─────────────────┘     └────────┬────────┘
                                                          │
                                                          ▼
                                                ┌─────────────────┐
                                                │ Signaling Server│
                                                │ (localhost:9000)│
                                                └────────┬────────┘
                                                          │
                                                          ▼
                                                ┌─────────────────┐
                                                │    Browser      │
                                                │  (WebRTC Peer)  │
                                                └─────────────────┘
```

## API Endpoints

The signaling server provides these endpoints:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/` | GET | HTML viewer page |
| `/offer` | GET | Get SDP offer (JSON) |
| `/answer` | POST | Submit SDP answer (JSON) |
| `/status` | GET | Connection status |

### Manual Signaling

For custom clients, use the API directly:

```javascript
// 1. Get offer from server
const offerRes = await fetch('http://localhost:9000/offer');
const { sdp: offerSdp } = await offerRes.json();

// 2. Create peer connection
const pc = new RTCPeerConnection({
    iceServers: [{ urls: 'stun:stun.l.google.com:19302' }]
});

// 3. Set remote description (server's offer)
await pc.setRemoteDescription({ type: 'offer', sdp: offerSdp });

// 4. Create and set local answer
const answer = await pc.createAnswer();
await pc.setLocalDescription(answer);

// 5. Send answer to server
await fetch('http://localhost:9000/answer', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ sdp: answer.sdp })
});

// 6. Handle incoming tracks
pc.ontrack = (event) => {
    document.getElementById('video').srcObject = event.streams[0];
};
```

## Network Configuration

### Local Network Access

By default, the server binds to `0.0.0.0`, allowing access from other devices on your network:

```bash
# From another device on your network:
# http://YOUR_IP:9000
```

### Firewall

Ensure the WebRTC port is accessible:

```bash
# UFW
sudo ufw allow 9000/tcp

# firewalld
sudo firewall-cmd --add-port=9000/tcp --permanent
sudo firewall-cmd --reload
```

### NAT Traversal

WebRTC uses ICE (Interactive Connectivity Establishment) with STUN servers for NAT traversal. The default STUN server is `stun:stun.l.google.com:19302`.

## Codec Support

WebRTC output uses:
- **Video**: H.264 (most compatible with browsers)
- **Audio**: Opus (when audio is enabled)

## Troubleshooting

### Video Not Playing

1. Check browser console for errors
2. Ensure WebRTC is not blocked by browser extensions
3. Try a different browser (Chrome recommended)
4. Check that the port is not blocked

### High Latency

WebRTC is designed for low latency. If experiencing delays:
1. Check network conditions
2. Ensure no proxies are interfering
3. Try reducing resolution: `--preset 720p60`

### Connection Failed

1. Verify the signaling server is running (check terminal output)
2. Ensure ICE candidates can be exchanged (check firewall)
3. Try on localhost first before network access

### Multiple Viewers

Currently, the WebRTC output supports a single viewer at a time. For multiple viewers, consider:
- RTMP streaming to a media server
- Using a WebRTC SFU (Selective Forwarding Unit)

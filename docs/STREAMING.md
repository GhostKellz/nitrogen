# Streaming Guide

Nitrogen supports streaming to RTMP/SRT services like Twitch, YouTube, and custom servers.

## Supported Protocols

- **RTMP** (`rtmp://`) - Standard streaming protocol, widely supported
- **RTMPS** (`rtmps://`) - Secure RTMP over TLS
- **SRT** (`srt://`) - Secure Reliable Transport, low-latency alternative

## Quick Start

### Stream to Twitch

```bash
nitrogen cast --stream rtmp://live.twitch.tv/app/YOUR_STREAM_KEY
```

### Stream to YouTube

```bash
nitrogen cast --stream rtmp://a.rtmp.youtube.com/live2/YOUR_STREAM_KEY
```

### Stream to Custom SRT Server

```bash
nitrogen cast --stream srt://your-server.com:9000
```

## Combined Outputs

You can combine streaming with other outputs:

```bash
# Stream and record locally
nitrogen cast --stream rtmp://... --record game.mp4

# Stream with Discord virtual camera
nitrogen cast --stream rtmp://... --discord
```

## Stream Settings

### Resolution and Framerate

Most streaming services have recommended settings:

| Service | Max Resolution | Max FPS | Recommended Bitrate |
|---------|---------------|---------|---------------------|
| Twitch | 1080p | 60 | 6000 kbps |
| YouTube | 4K | 60 | 20000-51000 kbps |
| Facebook | 1080p | 30 | 4000 kbps |

Use presets to match:

```bash
# Twitch-optimized
nitrogen cast --preset 1080p60 --bitrate 6000 --stream rtmp://...

# YouTube 4K
nitrogen cast --preset 4k60 --bitrate 20000 --stream rtmp://...
```

### Codec Selection

- **H.264** (default) - Most compatible, supported everywhere
- **HEVC** - Better compression, limited platform support
- **AV1** - Best compression, emerging support (YouTube, Twitch beta)

```bash
# Use HEVC for better quality at same bitrate
nitrogen cast --codec hevc --stream rtmp://...

# Use AV1 (requires RTX 40+ GPU)
nitrogen cast --codec av1 --stream srt://...
```

### Audio Configuration

```bash
# Stream with desktop audio
nitrogen cast --audio desktop --stream rtmp://...

# Stream with microphone
nitrogen cast --audio mic --stream rtmp://...

# Stream with both (mixed)
nitrogen cast --audio both --stream rtmp://...

# Adjust audio bitrate (default: 192 kbps for AAC)
nitrogen cast --audio both --audio-bitrate 256 --stream rtmp://...
```

## Low-Latency Streaming

For low-latency applications, use SRT:

```bash
# SRT with minimum latency
nitrogen cast --stream srt://server:9000?latency=200000
```

SRT URL parameters:
- `latency` - Target latency in microseconds (default: 120000 = 120ms)
- `maxbw` - Maximum bandwidth in bytes/sec

## Troubleshooting

### Stream Won't Connect

1. Verify the stream URL and key are correct
2. Check your network allows outbound connections on the required port
3. For RTMP, ensure port 1935 is not blocked
4. Try RTMPS (port 443) if regular RTMP is blocked

### Poor Stream Quality

1. Increase bitrate: `--bitrate 8000`
2. Use a higher quality preset: `--quality slow`
3. Enable lookahead for better encoding: `--av1-lookahead` (AV1 only)

### High Latency

1. Use SRT instead of RTMP
2. Enable low-latency mode (enabled by default)
3. Reduce resolution/framerate if network is constrained

### Stream Key Security

Nitrogen automatically masks stream keys in logs and display output. However:

- Never share your stream key
- Regenerate keys if compromised
- Use environment variables for automation:

```bash
export TWITCH_KEY="your_key"
nitrogen cast --stream "rtmp://live.twitch.tv/app/$TWITCH_KEY"
```

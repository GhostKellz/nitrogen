# CLI Commands Reference

Complete reference for all Nitrogen CLI commands and options.

## Commands Overview

| Command | Description |
|---------|-------------|
| `nitrogen cast` | Start capture and stream to virtual camera |
| `nitrogen list-sources` | List available capture sources |
| `nitrogen info` | Show system info and NVENC capabilities |
| `nitrogen stop` | Stop the current capture session |
| `nitrogen status` | Show status of running capture |

---

## nitrogen cast

Start screen capture and stream to a virtual camera.

### Basic Options

| Option | Short | Default | Description |
|--------|-------|---------|-------------|
| `--preset` | `-p` | `1080p60` | Output preset (resolution/fps) |
| `--resolution` | | | Custom resolution (e.g., `2560x1600`) |
| `--fps` | | | Custom framerate |
| `--codec` | `-c` | `h264` | Video codec (h264, hevc, av1) |
| `--bitrate` | `-b` | `0` (auto) | Video bitrate in kbps |
| `--quality` | `-q` | `medium` | Encoder quality (fast, medium, slow, quality) |

### Capture Source

| Option | Short | Description |
|--------|-------|-------------|
| `--monitor` | `-m` | Monitor ID to capture (e.g., "DP-2") |
| `--window` | `-w` | Window ID to capture |

If neither is specified, a portal picker dialog will appear.

### Output Options

| Option | Default | Description |
|--------|---------|-------------|
| `--camera-name` | `Nitrogen Camera` | Virtual camera name |
| `--no-camera` | | Disable virtual camera (recording only) |
| `--record` | | Record to file (e.g., `~/Videos/stream.mp4`) |
| `--no-daemon` | | Run in foreground only (no IPC server) |

### Audio Options

| Option | Default | Description |
|--------|---------|-------------|
| `--audio` | `none` | Audio source (none, desktop, mic, both) |
| `--audio-codec` | `aac` | Audio codec (aac, opus) |
| `--audio-bitrate` | `0` (auto) | Audio bitrate in kbps |

### HDR Tonemapping

| Option | Default | Description |
|--------|---------|-------------|
| `--hdr-tonemap` | `auto` | Tonemap mode (auto, on, off) |
| `--hdr-algorithm` | `reinhard` | Algorithm (reinhard, aces, hable) |
| `--hdr-peak-luminance` | `1000` | Peak luminance in nits |

**Algorithms:**
- `reinhard` - Simple, preserves colors well
- `aces` - Filmic, cinematic look (used in film production)
- `hable` - Uncharted 2 filmic curve

### Performance Overlay

| Option | Default | Description |
|--------|---------|-------------|
| `--overlay` | | Enable latency overlay |
| `--overlay-position` | `top-left` | Position (top-left, top-right, bottom-left, bottom-right) |

The overlay shows capture latency, encode latency, FPS, and dropped frames.

### Frame Generation (Smooth Motion)

| Option | Default | Description |
|--------|---------|-------------|
| `--frame-gen` | `off` | Frame interpolation (off, 2x, 3x, 4x, adaptive) |

**Warning:** Experimental feature. May cause visual artifacts.

- `2x` - Doubles framerate (30fps → 60fps)
- `3x` - Triples framerate (30fps → 90fps)
- `4x` - Quadruples framerate (30fps → 120fps)
- `adaptive` - Automatically adjusts based on scene complexity

### AV1 Codec Options (RTX 40/50 Series)

| Option | Default | Description |
|--------|---------|-------------|
| `--av1-10bit` | | Enable 10-bit color (main10 profile) |
| `--av1-tier` | `main` | AV1 tier (main, high) |
| `--av1-tune` | `hq` | Tuning mode (hq, uhq, ll, ull) |
| `--av1-lookahead` | | Enable lookahead for better quality |
| `--av1-lookahead-depth` | `20` | Lookahead depth in frames |
| `--av1-spatial-aq` | | Enable spatial adaptive quantization |
| `--av1-temporal-aq` | | Enable temporal AQ (RTX 50+) |
| `--av1-chroma` | `420` | Chroma format (420, 422, 444) |
| `--av1-b-ref` | | Enable B-frame reference mode |
| `--av1-gop` | | GOP length override |
| `--av1-auto` | | Auto-detect RTX 50 features |

**RTX 50 Series Exclusive Features:**
- `uhq` tuning (~8% better compression)
- Temporal AQ (~4-5% efficiency gain)
- Extended lookahead (up to 250 frames)
- YUV 4:2:2 and 4:4:4 chroma
- B-frame reference mode

### Streaming (RTMP/SRT)

| Option | Description |
|--------|-------------|
| `--stream` | Stream URL (rtmp://, rtmps://, or srt://) |

**Examples:**
- Twitch: `--stream rtmp://live.twitch.tv/app/your_stream_key`
- YouTube: `--stream rtmp://a.rtmp.youtube.com/live2/your_stream_key`
- SRT server: `--stream srt://localhost:9999`

### Audio Mixing

| Option | Default | Description |
|--------|---------|-------------|
| `--desktop-volume` | `1.0` | Desktop audio volume (0.0 - 2.0) |
| `--mic-volume` | `1.0` | Microphone volume (0.0 - 2.0) |
| `--audio-ducking` | | Reduce desktop when mic is active |

### Other Options

| Option | Description |
|--------|-------------|
| `--gpu` | GPU index for encoding (default: 0) |
| `--no-low-latency` | Disable low-latency mode |

---

## Examples

### Basic Streaming

```bash
# Default 1080p60 streaming
nitrogen cast

# 1440p60 with higher bitrate
nitrogen cast --preset 1440p60 --bitrate 12000

# Custom resolution
nitrogen cast --resolution 2560x1600 --fps 60
```

### With Audio

```bash
# Desktop audio only
nitrogen cast --audio desktop

# Microphone only
nitrogen cast --audio mic

# Both desktop and microphone
nitrogen cast --audio both --audio-codec opus
```

### Recording

```bash
# Stream + record
nitrogen cast --record ~/Videos/stream.mp4

# Record only (no virtual camera)
nitrogen cast --no-camera --record ~/Videos/recording.mkv

# High quality recording with HEVC
nitrogen cast --codec hevc --quality slow --record ~/Videos/hq.mp4
```

### HDR Content

```bash
# Auto-detect and tonemap HDR
nitrogen cast --hdr-tonemap auto

# Force tonemapping with ACES curve
nitrogen cast --hdr-tonemap on --hdr-algorithm aces

# Passthrough (no tonemapping)
nitrogen cast --hdr-tonemap off
```

### Performance Monitoring

```bash
# Enable overlay in top-left corner
nitrogen cast --overlay

# Overlay in bottom-right corner
nitrogen cast --overlay --overlay-position bottom-right
```

### Frame Generation

```bash
# 30fps to 60fps interpolation
nitrogen cast --preset 1080p30 --frame-gen 2x

# Maximum smoothness
nitrogen cast --preset 720p30 --frame-gen 4x

# Adaptive interpolation
nitrogen cast --frame-gen adaptive
```

### AV1 Encoding (RTX 40/50)

```bash
# Basic AV1
nitrogen cast --codec av1

# High quality AV1 with RTX 50 features
nitrogen cast --codec av1 --av1-auto

# Manual RTX 50 configuration
nitrogen cast --codec av1 --av1-tune uhq --av1-temporal-aq --av1-b-ref

# 10-bit AV1 with lookahead
nitrogen cast --codec av1 --av1-10bit --av1-lookahead --av1-lookahead-depth 50
```

### RTMP/SRT Streaming

```bash
# Stream to Twitch
nitrogen cast --stream rtmp://live.twitch.tv/app/live_xxxxx_yyyyy

# Stream to YouTube Live
nitrogen cast --stream rtmp://a.rtmp.youtube.com/live2/xxxx-xxxx-xxxx

# Low-latency SRT stream
nitrogen cast --stream srt://localhost:9999

# High quality stream with audio
nitrogen cast \
  --stream rtmp://live.twitch.tv/app/key \
  --codec hevc \
  --bitrate 8000 \
  --audio desktop
```

### Audio Mixing

```bash
# Desktop audio only
nitrogen cast --audio desktop

# Both desktop and mic with custom volumes
nitrogen cast --audio both --desktop-volume 0.8 --mic-volume 1.2

# Enable ducking (reduce desktop when speaking)
nitrogen cast --audio both --audio-ducking

# Mute desktop, mic only
nitrogen cast --audio both --desktop-volume 0.0 --mic-volume 1.5
```

### Combined Options

```bash
# Full featured streaming
nitrogen cast \
  --preset 1080p60 \
  --codec hevc \
  --bitrate 8000 \
  --audio desktop \
  --record ~/Videos/stream.mp4 \
  --overlay

# Maximum quality recording
nitrogen cast \
  --no-camera \
  --preset 4k60 \
  --codec av1 \
  --av1-10bit \
  --av1-auto \
  --quality slow \
  --bitrate 40000 \
  --audio both \
  --record ~/Videos/recording.mkv
```

---

## nitrogen info

Display system information and NVENC capabilities.

```bash
nitrogen info
```

Shows:
- GPU model and driver version
- NVENC encoder availability
- Supported codecs
- RTX 50 features (if available)
- Gamescope/Steam Deck detection

---

## nitrogen list-sources

List available capture sources.

```bash
nitrogen list-sources
```

Shows available monitors and windows that can be captured.

---

## nitrogen status

Show status of running capture session.

```bash
nitrogen status
```

Displays:
- Current state (running, paused, etc.)
- Resolution and framerate
- Frames processed/dropped
- Encoding latency statistics

---

## nitrogen stop

Stop the current capture session.

```bash
nitrogen stop
```

Gracefully stops capture and closes virtual camera.

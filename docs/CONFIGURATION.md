# Configuration Guide

Nitrogen uses a TOML configuration file located at `~/.config/nitrogen/config.toml`.

## Quick Start

Create the config directory and file:

```bash
mkdir -p ~/.config/nitrogen
touch ~/.config/nitrogen/config.toml
```

## Full Configuration Reference

```toml
# ~/.config/nitrogen/config.toml

[defaults]
# Default resolution preset
# Options: 720p30, 720p60, 1080p30, 1080p60, 1440p30, 1440p60, 1440p120, 4k30, 4k60, 4k120
preset = "1080p60"

# Default video codec
# Options: h264, hevc, av1
codec = "h264"

# Video bitrate in kbps (0 = auto based on preset)
bitrate = 6000

# Enable low-latency encoding mode
low_latency = true

[camera]
# Name shown in applications like Discord
name = "Nitrogen Camera"

[encoder]
# Encoder quality preset
# Options: fast, medium, slow, quality
# fast = lowest latency, lower quality
# quality = highest quality, more latency
quality = "medium"

# GPU index for multi-GPU systems (0 = first GPU)
gpu = 0

[audio]
# Audio capture source
# Options: none, desktop, mic, both
source = "none"

# Audio codec
# Options: aac, opus
codec = "aac"

# Audio bitrate in kbps
bitrate = 192

[hotkeys]
# Enable global hotkeys (requires input group membership)
enabled = true

# Toggle capture on/off
toggle = "ctrl+shift+f9"

# Pause/resume capture
pause = "ctrl+shift+f10"

# Toggle file recording
record = "ctrl+shift+f11"

[recording]
# Default output directory for recordings
output_dir = "~/Videos"

# Default container format
# Options: mp4, mkv
format = "mp4"
```

## Preset Reference

| Preset | Resolution | FPS | Default Bitrate |
|--------|------------|-----|-----------------|
| `720p30` | 1280x720 | 30 | 2500 kbps |
| `720p60` | 1280x720 | 60 | 4000 kbps |
| `1080p30` | 1920x1080 | 30 | 4500 kbps |
| `1080p60` | 1920x1080 | 60 | 6000 kbps |
| `1440p30` | 2560x1440 | 30 | 8000 kbps |
| `1440p60` | 2560x1440 | 60 | 12000 kbps |
| `1440p120` | 2560x1440 | 120 | 20000 kbps |
| `4k30` | 3840x2160 | 30 | 15000 kbps |
| `4k60` | 3840x2160 | 60 | 25000 kbps |
| `4k120` | 3840x2160 | 120 | 40000 kbps |

## Codec Requirements

| Codec | GPU Requirement | Notes |
|-------|-----------------|-------|
| `h264` | GTX 600+ | Most compatible, recommended for Discord |
| `hevc` | GTX 900+ | Better quality at same bitrate |
| `av1` | RTX 4000+ | Best compression, limited compatibility |

## Encoder Quality Presets

| Quality | Use Case | Latency | Quality |
|---------|----------|---------|---------|
| `fast` | Live streaming | Lowest | Good |
| `medium` | General use | Low | Better |
| `slow` | Recording | Medium | High |
| `quality` | Production | Higher | Best |

## Audio Codec Comparison

| Codec | Best For | Bitrate Range |
|-------|----------|---------------|
| `aac` | Compatibility, MP4 files | 96-320 kbps |
| `opus` | Quality, MKV files | 64-256 kbps |

## Hotkey Format

Hotkeys use the format: `modifier+modifier+key`

**Modifiers:**
- `ctrl` or `control`
- `alt`
- `shift`
- `super`, `meta`, or `win`

**Keys:**
- Function keys: `f1` through `f12`
- Letters: `a` through `z`
- Numbers: `0` through `9`
- Special: `space`, `enter`, `escape`, `tab`, `delete`, `insert`
- Navigation: `home`, `end`, `pageup`, `pagedown`, `up`, `down`, `left`, `right`
- Numpad: `numpad0` through `numpad9` or `kp0` through `kp9`

**Examples:**
```toml
toggle = "ctrl+shift+f9"
pause = "alt+p"
record = "super+r"
```

## Command Line Overrides

CLI arguments override config file settings:

```bash
# Override preset
nitrogen cast --preset 1440p60

# Override codec and bitrate
nitrogen cast --codec hevc --bitrate 8000

# Override audio
nitrogen cast --audio desktop

# Combine overrides
nitrogen cast -p 1080p60 --codec h264 --audio both --record ~/Videos/stream.mp4
```

## Environment Variables

| Variable | Description |
|----------|-------------|
| `NITROGEN_CONFIG` | Custom config file path |
| `RUST_LOG` | Logging level (error, warn, info, debug, trace) |

## Configuration Priority

1. CLI arguments (highest priority)
2. Environment variables
3. Config file (`~/.config/nitrogen/config.toml`)
4. Built-in defaults (lowest priority)

## Example Configurations

### Discord Streaming (Low Bandwidth)
```toml
[defaults]
preset = "720p60"
codec = "h264"
bitrate = 3000
low_latency = true

[encoder]
quality = "fast"
```

### Discord Streaming (High Quality)
```toml
[defaults]
preset = "1080p60"
codec = "h264"
bitrate = 8000
low_latency = true

[audio]
source = "desktop"
codec = "aac"
bitrate = 192
```

### Local Recording
```toml
[defaults]
preset = "1440p60"
codec = "hevc"
bitrate = 15000
low_latency = false

[encoder]
quality = "slow"

[audio]
source = "both"
codec = "aac"
bitrate = 256

[recording]
output_dir = "~/Videos/Recordings"
format = "mp4"
```

### 4K Gaming Capture
```toml
[defaults]
preset = "4k60"
codec = "av1"  # RTX 40 series required
bitrate = 30000
low_latency = true

[encoder]
quality = "medium"

[audio]
source = "desktop"
codec = "opus"
bitrate = 160
```

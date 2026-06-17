# Audio Capture Guide

Nitrogen captures audio through PipeWire, supporting both desktop audio and microphone input.

## Quick Start

```bash
# Capture desktop audio
nitrogen cast --audio desktop

# Capture microphone
nitrogen cast --audio mic

# Capture both
nitrogen cast --audio both

# Record with audio
nitrogen cast --audio desktop --record ~/Videos/stream.mp4
```

## Audio Sources

### Desktop Audio
Captures all audio playing on your system (games, music, videos, etc.).

```bash
nitrogen cast --audio desktop
```

This works by capturing the audio monitor of your default audio sink.

### Microphone
Captures input from your default microphone/input device.

```bash
nitrogen cast --audio mic
```

### Both
Captures and mixes both desktop audio and microphone.

```bash
nitrogen cast --audio both
```

## Audio Codecs

### AAC
- Best compatibility with MP4 containers
- Widely supported in browsers and players
- Recommended for Discord streaming

```bash
nitrogen cast --audio desktop --audio-codec aac
```

### Opus
- Better quality at lower bitrates
- Recommended for MKV containers
- Better for voice content

```bash
nitrogen cast --audio desktop --audio-codec opus
```

## Audio Bitrate

Control audio quality with bitrate settings:

```bash
# Low quality (saves bandwidth)
nitrogen cast --audio desktop --audio-bitrate 96

# Standard quality
nitrogen cast --audio desktop --audio-bitrate 192

# High quality
nitrogen cast --audio desktop --audio-bitrate 320
```

**Recommended bitrates:**

| Use Case | AAC | Opus |
|----------|-----|------|
| Voice only | 96 kbps | 64 kbps |
| Mixed content | 192 kbps | 128 kbps |
| Music/High quality | 256-320 kbps | 160-256 kbps |

## Configuration

Add to `~/.config/nitrogen/config.toml`:

```toml
[audio]
# Audio source: none, desktop, mic, both
source = "desktop"

# Audio codec: aac, opus
codec = "aac"

# Bitrate in kbps
bitrate = 192
```

## Technical Details

### Sample Format
Nitrogen captures audio in these formats (in order of preference):
1. F32LE (32-bit float) - best quality
2. S32LE (32-bit signed integer)
3. S16LE (16-bit signed integer)

### Sample Rates
- Capture: Uses source device sample rate (usually 48000 Hz)
- Encoding: Converted to encoder's preferred rate

### Channels
- Stereo (2 channels) for desktop audio
- Mono (1 channel) for microphone
- Both: Mixed to stereo

### Buffer Size
Audio is captured in small chunks to minimize latency:
- Default: ~20ms per buffer
- Trade-off between latency and CPU usage

## PipeWire Requirements

### Check PipeWire Status
```bash
# Verify PipeWire is running
systemctl --user status pipewire

# Check if PipeWire handles audio
pactl info | grep "Server Name"
# Should show: PipeWire
```

### List Audio Devices
```bash
# List all audio nodes
pw-cli ls Node

# List sinks (speakers/outputs)
pw-cli ls Node | grep -A2 "Audio/Sink"

# List sources (microphones/inputs)
pw-cli ls Node | grep -A2 "Audio/Source"
```

### Set Default Devices
```bash
# List available sinks
pactl list short sinks

# Set default sink
pactl set-default-sink <sink-name>

# List available sources
pactl list short sources

# Set default source
pactl set-default-source <source-name>
```

## Troubleshooting

### No Audio in Recording

1. **Check audio source is specified:**
   ```bash
   nitrogen cast --audio desktop --record output.mp4
   ```

2. **Verify PipeWire is handling audio:**
   ```bash
   pactl info | grep "Server Name"
   ```

3. **Check if audio is actually playing:**
   - Desktop audio capture needs active audio output
   - Try playing something before capturing

### Audio Desync

1. **Use low-latency mode:**
   ```bash
   nitrogen cast --low-latency --audio desktop
   ```

2. **Try lower framerate:**
   ```bash
   nitrogen cast -p 1080p30 --audio desktop
   ```

3. **Check system load:**
   - High CPU/GPU usage can cause timing issues
   - Close unnecessary applications

### Wrong Device Captured

1. **Check defaults:**
   ```bash
   pactl get-default-sink
   pactl get-default-source
   ```

2. **Change defaults:**
   ```bash
   pactl set-default-sink <correct-sink>
   pactl set-default-source <correct-source>
   ```

3. **Use pavucontrol:**
   ```bash
   pavucontrol  # GUI for audio device management
   ```

### Audio Too Quiet/Loud

1. **Check system volume:**
   ```bash
   pactl get-sink-volume @DEFAULT_SINK@
   ```

2. **Adjust capture volume:**
   - Use pavucontrol to adjust application/device volumes
   - Nitrogen captures at system volume level

## Debug Logging

Enable audio debug logging:

```bash
RUST_LOG=nitrogen_core::capture::audio=debug nitrogen cast --audio desktop
```

This shows:
- Audio device detection
- Format negotiation
- Buffer processing
- Sample rate conversion

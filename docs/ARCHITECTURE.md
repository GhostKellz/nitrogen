# Nitrogen Architecture

## Overview

Nitrogen is a Wayland-native streaming application that captures your screen using xdg-desktop-portal, encodes video using NVIDIA NVENC, and outputs to a PipeWire virtual camera for use in applications like Discord.

## Pipeline Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            Nitrogen Pipeline                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌────────────────┐    ┌────────────────┐    ┌────────────────────────────┐ │
│  │  Portal        │    │   Scaler       │    │   Virtual Camera           │ │
│  │  Capture       │───▶│   (Optional)   │───▶│   (PipeWire)               │ │
│  │  (PipeWire)    │    │                │    │                            │ │
│  └────────────────┘    └────────────────┘    └────────────────────────────┘ │
│         │                     │                           │                  │
│         ▼                     ▼                           ▼                  │
│    Raw Frames           Scaled Frames              "Nitrogen Camera"         │
│    (BGRA/DMA-BUF)       (Target Resolution)        visible in apps           │
│                                                                              │
│  ┌────────────────┐    ┌────────────────┐    ┌────────────────────────────┐ │
│  │  NVENC         │    │   Muxer        │    │   File Output              │ │
│  │  Encoder       │───▶│   (FFmpeg)     │───▶│   (MP4/MKV)                │ │
│  │  (H.264/HEVC)  │    │                │    │                            │ │
│  └────────────────┘    └────────────────┘    └────────────────────────────┘ │
│         ▲                     ▲                                              │
│         │                     │                                              │
│  ┌────────────────┐    ┌────────────────┐                                   │
│  │  Audio         │    │   Audio        │                                   │
│  │  Capture       │───▶│   Encoder      │                                   │
│  │  (PipeWire)    │    │  (AAC/Opus)    │                                   │
│  └────────────────┘    └────────────────┘                                   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. Capture Layer (`nitrogen-core/src/capture/`)

#### Portal Capture (`portal.rs`)
- Uses xdg-desktop-portal for secure screen/window selection
- Communicates via D-Bus using `ashpd` crate
- Works with any Wayland compositor that supports the portal

#### Video Stream (`stream.rs`)
- Receives raw video frames from PipeWire
- Handles both memory-mapped and DMA-BUF frames
- Broadcasts frames to subscribers via tokio channels

#### Audio Capture (`audio.rs`)
- Captures desktop audio or microphone via PipeWire
- Supports multiple sample formats (F32, S16, S32)
- Broadcasts audio frames to subscribers

### 2. Encoding Layer (`nitrogen-core/src/encode/`)

#### NVENC Encoder (`nvenc.rs`)
- Hardware-accelerated video encoding using NVIDIA GPU
- Supports H.264, HEVC, and AV1 (RTX 40+)
- Configurable quality presets and bitrates

#### Audio Encoder (`audio.rs`)
- FFmpeg-based audio encoding
- Supports AAC and Opus codecs
- Sample rate and channel conversion

#### Scaler (`scaler.rs`)
- Software frame scaling when source != target resolution
- Uses FFmpeg swscale for high-quality resizing

### 3. Output Layer (`nitrogen-core/src/output/`)

#### File Recorder (`file.rs`)
- Muxes encoded video and audio into MP4 or MKV containers
- Uses FFmpeg for container format handling
- Handles A/V synchronization

### 4. Pipeline Controller (`pipeline.rs`)
- Orchestrates all components
- Manages lifecycle (start, stop, pause)
- Handles configuration and state

### 5. IPC Layer (`nitrogen-core/src/ipc/`)
- Unix domain socket communication
- Allows CLI to control running daemon
- Commands: start, stop, status

### 6. Hotkeys (`hotkeys.rs`)
- Global keyboard shortcuts via evdev
- Works across all applications
- Configurable key bindings

## Data Flow

### Video Path
1. Portal requests screen capture permission
2. PipeWire delivers raw frames (BGRA or DMA-BUF)
3. Optional: Scaler resizes if needed
4. Frames sent to virtual camera (always)
5. If recording: Frames encoded via NVENC → muxed to file

### Audio Path
1. PipeWire delivers audio samples
2. Samples converted to encoder format
3. Encoded via FFmpeg (AAC/Opus)
4. Muxed with video into file

## Crate Structure

```
nitrogen/
├── nitrogen-core/        # Core library
│   ├── capture/          # Screen and audio capture
│   ├── config/           # Configuration parsing
│   ├── encode/           # Video/audio encoding
│   ├── ipc/              # Inter-process communication
│   ├── output/           # File recording
│   ├── pipeline.rs       # Main orchestrator
│   ├── hotkeys.rs        # Global hotkeys
│   ├── types.rs          # Shared types
│   └── error.rs          # Error types
│
└── nitrogen-cli/         # CLI application
    └── commands/         # Subcommands (cast, stop, info, etc.)
```

## External Dependencies

| Component | Crate | Purpose |
|-----------|-------|---------|
| Async runtime | `tokio` | Async I/O and task scheduling |
| Portal | `ashpd` | xdg-desktop-portal D-Bus interface |
| PipeWire | `pipewire` | Video/audio streaming |
| FFmpeg | `ffmpeg-next` | Encoding and muxing |
| Hotkeys | `evdev` | Raw keyboard input |
| Config | `toml`, `serde` | Configuration parsing |
| Logging | `tracing` | Structured logging |

## Thread Model

- **Main thread**: CLI interaction, signal handling
- **PipeWire thread**: Video frame callbacks (real-time)
- **Audio thread**: Audio capture callbacks (real-time)
- **Hotkey thread**: Keyboard event monitoring
- **Tokio runtime**: Async tasks (IPC, file I/O)

## Memory Management

- Video frames use `Arc<VideoFrame>` for zero-copy sharing
- Audio frames use `Arc<AudioFrame>` for broadcast distribution
- DMA-BUF frames mapped only when needed
- Encoded packets reference-counted through muxing

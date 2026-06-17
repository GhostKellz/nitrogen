# Smooth Motion Frame Generation

Nitrogen includes experimental frame interpolation ("Smooth Motion") to increase output framerate beyond the source framerate. This can make 30fps content appear smoother at 60fps or higher.

> **Warning**: This is an experimental feature that may cause visual artifacts, especially in fast-moving scenes.

## Quick Start

```bash
# Double the framerate (30fps -> 60fps)
nitrogen cast --frame-gen 2x

# Triple the framerate (30fps -> 90fps)
nitrogen cast --frame-gen 3x

# Quadruple the framerate (30fps -> 120fps)
nitrogen cast --frame-gen 4x

# Adaptive mode (adjusts based on scene complexity)
nitrogen cast --frame-gen adaptive
```

## How It Works

Frame generation creates intermediate frames between captured frames:

```
Source:      [F1]─────────────────[F2]─────────────────[F3]

2x output:   [F1]────[I1]────[F2]────[I2]────[F3]────[I3]

Where I1, I2, I3 are interpolated frames
```

### Interpolation Methods

1. **GPU Interpolation** (NVIDIA Optical Flow)
   - Uses NVFRUC (NVIDIA Frame Rate Up Conversion)
   - Best quality, hardware-accelerated
   - Requires RTX 20 series or newer

2. **CPU Interpolation** (Fallback)
   - Linear blending between frames
   - Lower quality, may cause ghosting
   - Used when GPU interpolation unavailable

## Modes

| Mode | Description | Use Case |
|------|-------------|----------|
| `off` | No interpolation (default) | Maximum quality, no artifacts |
| `2x` | Double framerate | 30fps -> 60fps |
| `3x` | Triple framerate | 30fps -> 90fps |
| `4x` | Quadruple framerate | 30fps -> 120fps |
| `adaptive` | Adjusts based on motion | Best balance of quality/smoothness |

### Adaptive Mode

Adaptive mode analyzes each frame and adjusts interpolation:
- **Low motion**: More interpolation (smoother)
- **High motion**: Less interpolation (fewer artifacts)
- **Scene changes**: Skips interpolation entirely

## Configuration

### Scene Change Detection

The interpolator detects scene changes to avoid artifacts when the content changes dramatically. The default threshold is tuned for most content.

### DMA-BUF Frames

For DMA-BUF frames (zero-copy from GPU), interpolation falls back to frame duplication rather than blending, as the frames cannot be easily accessed on CPU.

## Performance Impact

Frame generation adds processing overhead:

| Mode | CPU Impact | GPU Impact | Latency Added |
|------|------------|------------|---------------|
| 2x | Low | Medium | ~8-16ms |
| 3x | Medium | Medium | ~8-16ms |
| 4x | High | High | ~8-16ms |
| adaptive | Variable | Variable | ~8-16ms |

## Best Practices

### When to Use

- Capturing 30fps content that you want smoother
- Streaming older games locked to 30fps
- Video playback through virtual camera

### When to Avoid

- Already running at target framerate (60fps+)
- Fast-paced competitive games (artifacts distracting)
- Content with lots of text/UI (interpolation can blur)
- Low-end systems where overhead impacts performance

### Recommended Settings

```bash
# Best quality for streaming
nitrogen cast --frame-gen adaptive --quality slow

# Performance-focused
nitrogen cast --frame-gen 2x --quality fast
```

## Troubleshooting

### Visual Artifacts

**Ghosting**: Objects leave trails
- Try `adaptive` mode instead of fixed multiplier
- Reduce multiplier (4x -> 2x)

**Judder/Stutter**: Inconsistent motion
- Ensure source framerate is stable
- Check CPU/GPU usage isn't maxed

**Blurry Interpolated Frames**:
- This is a limitation of linear blending
- GPU interpolation (NVFRUC) produces better results

### High CPU Usage

Frame generation can be CPU-intensive, especially at high resolutions:
1. Use a lower multiplier (2x instead of 4x)
2. Reduce resolution: `--preset 720p60`
3. Ensure GPU interpolation is active (check logs for "NVFRUC")

### GPU Interpolation Not Working

Check logs for NVFRUC initialization:
```bash
nitrogen cast --frame-gen 2x -vv 2>&1 | grep -i fruc
```

Requirements:
- NVIDIA RTX 20 series or newer
- Latest NVIDIA drivers
- `libnvidia-fbc` installed

## Technical Details

### Algorithm

1. **Scene Detection**: Compare histograms of consecutive frames
2. **Motion Estimation**: Use optical flow (GPU) or pixel analysis (CPU)
3. **Frame Synthesis**: Blend frames based on interpolation factor
4. **Output**: Insert interpolated frames into stream

### Frame Timing

Interpolated frames are timestamped to maintain proper playback timing:

```
Original: F1(0ms) ─────────── F2(33ms) ─────────── F3(66ms)

2x Mode:  F1(0ms) ── I1(16ms) ── F2(33ms) ── I2(50ms) ── F3(66ms)
```

### Memory Usage

Frame generation buffers frames for interpolation:
- 2-3 frames buffered at any time
- Memory = width × height × 4 bytes × buffer_count
- 1080p: ~25-40 MB additional
- 4K: ~100-150 MB additional

# Contributing to Nitrogen

Thank you for your interest in contributing to Nitrogen! This guide will help you get started.

## Code of Conduct

Be respectful and constructive. We're building software together.

## Getting Started

### Prerequisites

- Rust 1.75+ (stable)
- NVIDIA GPU with NVENC (GTX 600+ or Quadro K+)
- NVIDIA drivers 515.43.04+
- PipeWire and xdg-desktop-portal
- FFmpeg with NVENC support
- Wayland compositor (Hyprland, Sway, GNOME, KDE)

### Building

```bash
git clone https://github.com/ghostkellz/nitrogen
cd nitrogen
cargo build --release
cargo test
```

### Project Structure

```
nitrogen/
├── nitrogen-core/           # Core library
│   ├── src/
│   │   ├── capture/         # Screen and audio capture
│   │   │   ├── mod.rs       # Capture module exports
│   │   │   ├── portal.rs    # xdg-desktop-portal integration
│   │   │   ├── stream.rs    # PipeWire video stream
│   │   │   └── audio.rs     # PipeWire audio capture
│   │   ├── config/          # Configuration system
│   │   ├── encode/          # Video and audio encoding
│   │   │   ├── mod.rs       # Encoder exports
│   │   │   ├── nvenc.rs     # NVENC video encoder
│   │   │   ├── audio.rs     # AAC/Opus audio encoder
│   │   │   └── scaler.rs    # Frame scaling
│   │   ├── ipc/             # Inter-process communication
│   │   ├── output/          # Output targets
│   │   │   ├── mod.rs       # Output exports
│   │   │   └── file.rs      # File recording (MP4/MKV)
│   │   ├── hotkeys.rs       # Global hotkey support
│   │   ├── pipeline.rs      # Main capture pipeline
│   │   └── types.rs         # Core types
│   └── tests/               # Integration tests
├── nitrogen-cli/            # CLI application
│   └── src/commands/        # CLI commands
├── release/                 # Distribution packaging
│   ├── arch/                # Arch Linux PKGBUILD
│   ├── deb/                 # Debian packaging
│   ├── fedora/              # Fedora/Bazzite RPM spec
│   ├── pop-os/              # Pop!_OS notes
│   └── bazzite/             # Bazzite notes
└── docs/                    # Documentation
```

## Development Workflow

1. Fork and create a feature branch
2. Make changes with tests
3. Run `cargo test`, `cargo clippy`, `cargo fmt`
4. Submit a PR with clear description

### Commit Messages

```
feat: add audio capture support
fix: handle window close during capture
docs: update installation guide
test: add pipeline integration tests
```

## Code Style

- Use `cargo fmt` and `cargo clippy`
- Add `user_hint()` for recoverable errors
- Use appropriate log levels (error/warn/info/debug/trace)
- Document public APIs

## Areas for Contribution

**Good First Issues:**
- Improve error messages
- Add more tests
- Documentation improvements
- CLI help text refinement

**Larger Projects:**
- Region selection (capture specific area)
- TUI interface
- WebRTC output
- OBS integration
- Multi-monitor presets

## Getting Help

- Check existing issues before creating new ones
- Include `nitrogen info` output in bug reports
- Specify your compositor and GPU

## License

MIT OR Apache-2.0

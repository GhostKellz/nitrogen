# Changelog

All notable changes to Nitrogen will be tracked in this file.

## [Unreleased]

### Added

- Added GitHub issue templates for bug reports and feature requests.
- Added Dependabot configuration (`.github/dependabot.yml`) for weekly Cargo dependency updates.
- Added `SECURITY.md` with vulnerability reporting guidance, supported versions, dependency auditing notes, and user-facing security considerations.
- Added documentation index at `docs/README.md`.
- Added advisory tracking under `docs/advisories/`.

### Changed

- Reorganized documentation into topic folders:
  - `docs/features/`
  - `docs/getting-started/`
  - `docs/guides/`
  - `docs/internals/`
  - `docs/reference/`
- Updated README links to point at the reorganized docs.
- Updated `ghostwave-core` to the pinned `v0.3.1` tag.
- Updated the PipeWire Rust stack from `0.8` to `0.10` and migrated local PipeWire constructors to the current `MainLoopRc`, `ContextRc`, and `StreamRc` APIs.
- Added a workspace `Cargo.lock` suitable for committing with the binary workspace.

### Fixed

- Fixed encoder flush aborting on end-of-stream: `receive_packets()` in `encode/nvenc.rs` and `encode/audio.rs` now treat `ffmpeg::Error::Eof` after `send_eof()` as normal drain completion instead of an error.
- Fixed a SIGSEGV on shutdown caused by `av_write_trailer` being called twice: `output/file.rs` now guards finalization with a `trailer_written` flag so `finalize()` and `Drop` cannot both write the MP4 trailer. Added a `test_finalize_is_idempotent` regression test.

### Security

- Cleared prior accepted audit warnings from the old `ghostwave-core` dependency chain.
- Current `cargo audit` status: no vulnerability failures; one accepted unmaintained warning remains for `bincode 1.3.3` through `webrtc 0.11`.

### Validation

- `cargo build --workspace` and `cargo test --workspace` pass (0 failures; hardware-gated NVENC/PipeWire/portal tests verified on driver 610).
- `cargo clippy --workspace --all-targets -- -D warnings` is clean; `cargo fmt --all --check` is clean.
- `cargo audit` passes with the accepted `bincode` warning.
- End-to-end runtime validation on RTX 5090 / driver 610.43.02 (open) / KDE Wayland: portal screencast (monitor and whole-workspace 6400x2160) → H.264 NVENC → playable MP4; virtual camera (`Video/Source` PipeWire node) consumed by an external client.

## [0.2.0] - 2026-06-17

### Added

- Wayland-native capture through PipeWire and xdg-desktop-portal.
- NVIDIA NVENC-oriented capture, encode, output, and pipeline modules.
- Audio capture, mixing, virtual microphone output, and audio encoder support.
- WebRTC, RTMP/SRT-style streaming, file output, overlay, hotkey, IPC, environment, and performance modules.
- Unit and integration coverage for configuration, encoding, IPC, pipeline behavior, and core utility modules.

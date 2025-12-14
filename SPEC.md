# SPEC.md — Nitrogen

**Name:** Nitrogen  
**Tagline:** _“Wayland-native NVIDIA streaming for Discord and friends.”_  
**Author:** Christopher “CKEL / GhostKellz” Kelley  
**Status:** Draft v0.1  
**Target stack:** Rust + PipeWire + NVENC (NVIDIA) + Wayland (Plasma/Hyprland)

---

## 1. Problem Statement

Discord on Linux in 2025 still has rough edges:

- Wayland screen sharing is inconsistent (black screens, missing windows, no HDR awareness).
- NVIDIA users pay a massive performance tax when sharing 4K/high-refresh gameplay.
- Fullscreen Vulkan/Proton/gamescope titles are hard to share reliably.
- Users have to juggle Discord, OBS, and various workarounds for basic “show my game” functionality.

**Nitrogen** aims to solve this by providing a **Wayland-native, NVIDIA-accelerated streaming pipeline** that Discord and other apps can consume as a **standard camera or window source**, without violating Discord’s ToS.

---

## 2. High-Level Vision

Nitrogen is a **companion app**, not a Discord client.

- Wraps the **official Discord web client** (or native client) in a Rust-based shell.
- Provides a **virtual camera / video source** fed by:
  - Wayland screencast portal (PipeWire)
  - NVIDIA NVENC / AV1 encoder
- Optimizes for:
  - 4K / high refresh (120–240 Hz)
  - Low CPU overhead
  - Stable behavior on Wayland + NVIDIA
- Stays **ToS-safe** by:
  - Never speaking Discord’s protocol directly
  - Never handling tokens or login flows
  - Acting purely as a system-level video source

---

## 3. Goals & Non-Goals

### 3.1 Goals

- **G1:** Provide a **virtual camera** (“Nitrogen Camera”) visible in:
  - Discord
  - OBS
  - Video conferencing tools
- **G2:** Capture:
  - Full monitors (e.g. DP-2, 4K/240)
  - Individual windows (gamescope windows, apps)
- **G3:** Use **NVENC / AV1** where available for efficient encode:
  - Input: up to 4K/240
  - Output presets: 1080p60, 1440p60, 1440p120, etc.
- **G4:** Work on **Wayland** with:
  - Plasma 6 (KWin)
  - wlroots compositors (Hyprland, Sway, etc.)
- **G5:** Integrate cleanly with:
  - PipeWire + xdg-desktop-portal
  - NVIDIA 500+ driver stack
- **G6:** Provide a **CLI-first** UX with optional GUI overlay for control.

### 3.2 Non-Goals

- **NG1:** Do _not_ implement a Discord client or touch Discord’s private APIs.
- **NG2:** Do _not_ replace or intercept Discord’s voice/video backend.
- **NG3:** Do _not_ aim to be a full OBS competitor.
- **NG4:** Do _not_ support non-NVIDIA GPUs in v0.x (can be future expansion).

---

## 4. Architecture Overview

Nitrogen is split into two main components:

1. **nitrogen-core** (Rust library / backend)
2. **nitrogen-shell** (Rust app shell + optional UI)

### 4.1 nitrogen-core

Responsibilities:

- Handle **Wayland screencast** negotiation via portals:
  - D-Bus calls via `ashpd` or similar.
  - Prompt user to pick screen/window.
- Connect to **PipeWire**:
  - Set up input stream (screencast).
  - Set up output stream (virtual camera node).
- Implement **NVENC-based encoding pipeline**:
  - Use FFmpeg (`ffmpeg-next` / `ffmpeg-sys`) with `h264_nvenc`, `hevc_nvenc`, or `av1_nvenc`.
  - Support downscaling and FPS limiting.
- Provide a small API surface:
  - `start_capture(config: CaptureConfig) -> Result<Handle>`  
  - `stop_capture(handle: Handle)`
  - `list_sources() -> Vec<SourceInfo>`

### 4.2 nitrogen-shell

Two execution modes:

1. **Companion mode (with Discord webview)**  
   - Rust + Tauri or wry-based app.
   - Opens `https://discord.com/app` in an embedded webview.
   - Provides a Nitrogen control sidebar/overlay:
     - Select source (monitor/window)
     - Select resolution/FPS preset
     - Start/Stop Nitrogen Camera

2. **Headless / CLI mode**  
   - Run without GUI:
     - `nitrogen list-sources`
     - `nitrogen cast --monitor DP-2 --preset 1440p60`
     - `nitrogen stop`

---

## 5. Data Flow

1. **User starts Nitrogen** (GUI or CLI).
2. Nitrogen:
   - Uses **xdg-desktop-portal** to ask the user which screen/window to share.
   - Receives a PipeWire node ID / stream handle.
3. Nitrogen-core:
   - Subscribes to the PipeWire stream.
   - Optionally resizes and color-converts frames.
   - Feeds frames into **NVENC** via FFmpeg.
4. Encoded frames:
   - Are decoded or mapped into a format suitable for a **virtual camera** PipeWire node.
   - This node is registered as `Nitrogen Camera` (or similar).
5. Discord / OBS:
   - Sees `Nitrogen Camera` alongside other webcams.
   - User selects this as the video source.
6. Discord:
   - Re-encodes whatever it gets into its own upstream codec (H.264/AV1/etc.).
   - Nitrogen remains agnostic to Discord’s upstream wire format.

---

## 6. Key Components & Modules

### 6.1 Core Types

- `CaptureConfig`
  - `source: CaptureSource` (Monitor, Window)
  - `resolution: (u32, u32)`
  - `fps: u32`
  - `codec: Codec` (H264, HEVC, AV1)
  - `bitrate: u32`
- `CaptureSource`
  - `Monitor { id: String }`
  - `Window { id: String }`
- `SourceInfo`
  - `id: String`
  - `name: String`
  - `kind: SourceKind` (Monitor, Window)
  - `dimensions: (u32, u32)`
- `Handle`
  - Opaque capture session handle.

### 6.2 Crate Breakdown

- `nitrogen-core`
  - `capture::pipewire` — input stream (screencast)
  - `encode::nvenc` — FFmpeg/NVENC pipeline
  - `output::pipewire_camera` — virtual camera node
  - `config` — presets and configuration
  - `error` — error handling types

- `nitrogen-shell`
  - `cli` — argument parsing (clap)
  - `ui` — Tauri/wry integration
  - `bridge` — IPC/commands to nitrogen-core

---

## 7. MVP Scope (v0.1.x)

**Must-have:**

- PipeWire screencast capture via portal (monitor-only for first cut).
- NVENC H.264 encoding pipeline.
- PipeWire virtual camera output node.
- CLI:
  - `nitrogen list-sources`
  - `nitrogen cast --monitor <id> --preset 1080p60`
  - `nitrogen stop`
- Verified working with:
  - Discord (Flatpak/web)
  - OBS

**Nice-to-have for v0.1:**

- Simple TUI (ratatui or similar) to manage capture sessions.
- Basic presets:
  - `1080p60`
  - `1440p60`
  - `1440p120` (NVIDIA only, ensure headroom)

---

## 8. Future Phases

### v0.2.x

- Window-based capture (not just full monitor).
- Preset manager (`~/.config/nitrogen/config.toml`).
- HDR → SDR tonemapping for HDR games.
- Integration hints for gamescope:
  - Docs/examples: `gamescope --expose-wayland ...` + Nitrogen.

### v0.3.x

- Optional **WebRTC** output mode (view in browser / remote client).
- Multiple output profiles (local camera + remote stream).
- Basic latency/bitrate overlay for debugging (MangoHUD-like stats).

### v0.4.x+

- Optional integration into your **Ghost** ecosystem:
  - GhostNV (driver stack)
  - ghostctl (CLI launcher for Nitrogen)
  - Phantomboot live environment integration for “stream your recovery session”.

---

## 9. Platform & Environment Requirements

- **OS:** Linux (Arch-targeted, but portable)
- **GPU:** NVIDIA RTX (Turing or newer, ideally 3000+ / 4000+ / 5000+)
- **Drivers:** NVIDIA proprietary or open stack with NVENC enabled.
- **Display stack:**
  - Wayland (KDE Plasma 6, Hyprland, Sway, etc.)
  - PipeWire + xdg-desktop-portal stack running.
- **Toolchain:**
  - Rust (stable, latest)
  - FFmpeg libraries with NVENC support
  - PipeWire dev headers

---

## 10. Security & Privacy

- Nitrogen **never** handles:
  - Discord tokens
  - Login credentials
  - Direct API calls to Discord

- All capture is local-only unless:
  - Explicitly enabled WebRTC/remote streaming (future).
- Use the standard **screencast portal**, which:
  - Prompts user for source approval.
  - Can be revoked at any time.

---

## 11. Licensing & Branding

- **License:** MIT or Apache-2.0 (TBD).
- **Binary name:** `nitrogen`
- **Library crate:** `nitrogen-core`
- **Desktop name:** `Nitrogen`  
- **Camera name:** `Nitrogen Camera`

Tagline options:
- “Wayland-native NVIDIA streaming.”
- “Nitrogen: GPU-powered casting for Discord.”
- “Cast your games at 4K/240 — without melting your CPU.”

---


//! Info command - show system information and capabilities

use anyhow::Result;
use nitrogen_core::capture;
use nitrogen_core::config::Codec;
use nitrogen_core::encode;

/// Show system information and NVENC capabilities
pub async fn info() -> Result<()> {
    println!("Nitrogen - System Information\n");

    // GPU Information
    println!("GPU Information:");
    if let Some(gpu) = encode::get_gpu_info() {
        println!("  Model:   {}", gpu.name);
        println!("  VRAM:    {} MB", gpu.vram_mb);
        println!("  Driver:  {}", gpu.driver_version);
    } else {
        println!("  No NVIDIA GPU detected (nvidia-smi not found or failed)");
    }

    println!();

    // Check NVENC availability
    println!("NVIDIA Encoding Support:");
    println!("  NVENC Available: {}", encode::nvenc_available());

    let encoders = encode::available_encoders();
    if encoders.is_empty() {
        println!("  No NVENC encoders found.");
        println!();
        println!("  Make sure you have:");
        println!("  - An NVIDIA GPU with NVENC support (GeForce GTX 600+, Quadro K series+)");
        println!("  - NVIDIA drivers installed (515.43.04+ recommended)");
        println!("  - FFmpeg compiled with NVENC support");
    } else {
        println!("  Available encoders:");
        for encoder in &encoders {
            println!("    - {}", encoder);
        }
    }

    println!();

    // Codec capabilities
    println!("Codec Capabilities:");
    for codec in [Codec::H264, Codec::Hevc, Codec::Av1] {
        let codec_name = match codec {
            Codec::H264 => "H.264",
            Codec::Hevc => "HEVC",
            Codec::Av1 => "AV1",
        };

        if let Some(caps) = encode::get_encoder_capabilities(codec) {
            println!("  {}:", codec_name);
            println!("    Available:  yes");
            println!(
                "    B-frames:   {}",
                if caps.b_frames { "yes" } else { "no" }
            );
            println!("    10-bit:     {}", if caps.bit_10 { "yes" } else { "no" });
            println!(
                "    Lookahead:  {}",
                if caps.lookahead { "yes" } else { "no" }
            );
            println!("    Max res:    {}x{}", caps.max_width, caps.max_height);
        } else {
            println!("  {}: not available", codec_name);
        }
    }

    println!();

    // System services
    println!("System Services:");
    let (pw_running, pw_status) = capture::check_pipewire_status();
    let (portal_running, portal_status) = capture::check_portal_status();

    let pw_icon = if pw_running { "[OK]" } else { "[!!]" };
    let portal_icon = if portal_running { "[OK]" } else { "[!!]" };

    println!("  {} PipeWire:          {}", pw_icon, pw_status);
    println!("  {} xdg-desktop-portal: {}", portal_icon, portal_status);

    if !pw_running || !portal_running {
        println!();
        println!("  Troubleshooting:");
        if !pw_running {
            println!("    - PipeWire is required for screen capture");
            println!("      Try: systemctl --user start pipewire");
        }
        if !portal_running {
            println!("    - xdg-desktop-portal is required for screen selection");
            println!("      Try: systemctl --user start xdg-desktop-portal");
        }
    }

    println!();

    // Show environment info
    println!("Environment:");
    println!(
        "  XDG_SESSION_TYPE: {}",
        std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "not set".to_string())
    );
    println!(
        "  WAYLAND_DISPLAY:  {}",
        std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "not set".to_string())
    );
    println!(
        "  XDG_CURRENT_DESKTOP: {}",
        std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_else(|_| "not set".to_string())
    );

    // Check if running on Wayland
    let is_wayland = std::env::var("XDG_SESSION_TYPE")
        .map(|v| v == "wayland")
        .unwrap_or(false)
        || std::env::var("WAYLAND_DISPLAY").is_ok();

    println!();
    if is_wayland {
        println!("Running on Wayland - good!");
    } else {
        println!("Warning: Not running on Wayland.");
        println!("Nitrogen is designed for Wayland. X11 support is limited.");
    }

    println!();
    println!("Supported presets:");
    println!("  720p30   - 1280x720 @ 30fps");
    println!("  720p60   - 1280x720 @ 60fps");
    println!("  1080p30  - 1920x1080 @ 30fps");
    println!("  1080p60  - 1920x1080 @ 60fps (default)");
    println!("  1440p30  - 2560x1440 @ 30fps");
    println!("  1440p60  - 2560x1440 @ 60fps");
    println!("  1440p120 - 2560x1440 @ 120fps");
    println!("  4k30     - 3840x2160 @ 30fps");
    println!("  4k60     - 3840x2160 @ 60fps");
    println!("  4k120    - 3840x2160 @ 120fps");

    println!();
    println!("Supported codecs:");
    println!("  h264 - H.264/AVC (most compatible)");
    println!("  hevc - H.265/HEVC (better compression)");
    println!("  av1  - AV1 (best compression, RTX 40 series+)");

    Ok(())
}

//! Info command - show system information and capabilities

use anyhow::Result;
use nitrogen_core::encode;

/// Show system information and NVENC capabilities
pub async fn info() -> Result<()> {
    println!("Nitrogen - System Information\n");

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
        for encoder in encoders {
            println!("    - {}", encoder);
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

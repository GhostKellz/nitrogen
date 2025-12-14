//! Cast command - start capture and streaming

use anyhow::{Context, Result};
use clap::Args;
use nitrogen_core::{
    config::{CaptureConfig, Codec, EncoderPreset, Preset},
    pipeline::Pipeline,
    types::CaptureSource,
};
use tokio::signal;
use tracing::{error, info, warn};

/// Arguments for the cast command
#[derive(Args)]
pub struct CastArgs {
    /// Monitor ID to capture (e.g., "DP-2")
    /// If not specified, the portal picker will be shown
    #[arg(short, long)]
    monitor: Option<String>,

    /// Window ID to capture
    #[arg(short, long)]
    window: Option<String>,

    /// Output preset (720p30, 720p60, 1080p30, 1080p60, 1440p60, 1440p120, 4k30, 4k60)
    #[arg(short, long, default_value = "1080p60")]
    preset: String,

    /// Video codec (h264, hevc, av1)
    #[arg(short, long, default_value = "h264")]
    codec: String,

    /// Bitrate in kbps (0 = auto)
    #[arg(short, long, default_value = "0")]
    bitrate: u32,

    /// Encoder quality preset (fast, medium, slow, quality)
    #[arg(short, long, default_value = "medium")]
    quality: String,

    /// Virtual camera name
    #[arg(long, default_value = "Nitrogen Camera")]
    camera_name: String,

    /// Disable low-latency mode
    #[arg(long)]
    no_low_latency: bool,
}

/// Start a capture session
pub async fn cast(args: CastArgs) -> Result<()> {
    println!("Nitrogen - Starting Capture\n");

    // Parse preset
    let preset = Preset::from_preset_str(&args.preset).ok_or_else(|| {
        anyhow::anyhow!(
            "Invalid preset '{}'. Valid options: 720p30, 720p60, 1080p30, 1080p60, 1440p30, 1440p60, 1440p120, 4k30, 4k60, 4k120",
            args.preset
        )
    })?;

    // Parse codec
    let codec: Codec = args
        .codec
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid codec '{}'. Valid options: h264, hevc, av1", args.codec))?;

    // Parse encoder preset
    let encoder_preset = match args.quality.to_lowercase().as_str() {
        "fast" => EncoderPreset::Fast,
        "medium" => EncoderPreset::Medium,
        "slow" => EncoderPreset::Slow,
        "quality" => EncoderPreset::Quality,
        _ => {
            return Err(anyhow::anyhow!(
                "Invalid quality '{}'. Valid options: fast, medium, slow, quality",
                args.quality
            ))
        }
    };

    // Determine capture source
    let source = if let Some(ref monitor) = args.monitor {
        CaptureSource::monitor(monitor)
    } else if let Some(ref window) = args.window {
        CaptureSource::window(window)
    } else {
        // Default to portal selection (monitor)
        CaptureSource::monitor("portal")
    };

    // Build configuration
    let config = CaptureConfig {
        source,
        preset,
        codec,
        bitrate: args.bitrate,
        encoder_preset,
        camera_name: args.camera_name.clone(),
        low_latency: !args.no_low_latency,
    };

    println!("Configuration:");
    println!("  Preset:      {}", config.preset);
    println!("  Resolution:  {}x{}", config.width(), config.height());
    println!("  Framerate:   {} fps", config.fps());
    println!("  Codec:       {}", config.codec);
    println!("  Bitrate:     {} kbps", config.effective_bitrate());
    println!("  Camera:      {}", config.camera_name);
    println!("  Low Latency: {}", config.low_latency);
    println!();

    // Create pipeline
    let mut pipeline = Pipeline::new(config)
        .await
        .context("Failed to create pipeline")?;

    println!("Waiting for source selection...");
    println!("(A dialog should appear to select your screen or window)\n");

    // Start pipeline (will prompt user via portal)
    let session = pipeline.start().await.context("Failed to start pipeline")?;

    println!("Capture started!");
    println!("  Source:     {:?}", session.source_type);
    println!("  Resolution: {}x{}", session.width, session.height);
    println!("  Node ID:    {}", session.node_id);
    println!();
    println!("Virtual camera '{}' is now available.", args.camera_name);
    println!("Select it in Discord or other applications to start streaming.");
    println!();
    println!("Press Ctrl+C to stop...\n");

    // Wait for Ctrl+C
    let ctrl_c = async {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
    };

    // Process frames until interrupted
    tokio::select! {
        _ = ctrl_c => {
            println!("\nReceived interrupt signal...");
        }
        _ = async {
            while pipeline.is_running() {
                if let Err(e) = pipeline.process().await {
                    error!("Pipeline error: {}", e);
                    break;
                }
            }
        } => {
            info!("Pipeline processing ended");
        }
    }

    // Stop pipeline
    println!("Stopping capture...");
    pipeline.stop().await?;

    println!("Capture stopped.");

    Ok(())
}

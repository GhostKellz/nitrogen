//! Cast command - start capture and streaming

use anyhow::{Context, Result};
use clap::Args;
use nitrogen_core::{
    config::{
        AudioCodec, AudioSource, Av1Config, Av1Tier, Av1Tune, CaptureConfig, ChromaFormat, Codec,
        ConfigFile, EncoderPreset, MultipassMode, Preset,
    },
    daemon_running,
    gpu::detect_rtx50_features,
    ipc::IpcServer,
    pipeline::Pipeline,
    socket_path,
    types::CaptureSource,
};
use std::sync::Arc;
use tokio::signal;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

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
    /// Use --resolution and --fps for custom settings instead
    #[arg(short, long, default_value = "1080p60")]
    preset: String,

    /// Custom resolution (e.g., "2560x1600"). Overrides preset resolution.
    #[arg(long, value_name = "WxH")]
    resolution: Option<String>,

    /// Custom framerate. Overrides preset FPS.
    #[arg(long, value_name = "FPS")]
    fps: Option<u32>,

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

    /// Run in foreground only (no IPC server)
    #[arg(long)]
    no_daemon: bool,

    /// GPU index to use for NVENC encoding (0 = first GPU)
    #[arg(short = 'g', long, default_value = "0")]
    gpu: u32,

    /// Record to file (e.g., recording.mp4 or recording.mkv)
    /// Can be used alongside virtual camera output
    #[arg(short = 'r', long, value_name = "FILE")]
    record: Option<String>,

    /// Audio source (none, desktop, mic, both)
    #[arg(short = 'a', long, default_value = "none")]
    audio: String,

    /// Audio codec (aac, opus)
    #[arg(long, default_value = "aac")]
    audio_codec: String,

    /// Audio bitrate in kbps (0 = auto)
    #[arg(long, default_value = "0")]
    audio_bitrate: u32,

    // ========== AV1-specific options ==========
    /// AV1: Enable 10-bit color (main10 profile)
    #[arg(long)]
    av1_10bit: bool,

    /// AV1: Tier (main, high)
    #[arg(long, default_value = "main")]
    av1_tier: String,

    /// AV1: Tuning mode (hq, uhq, ll, ull)
    /// uhq = Ultra High Quality (RTX 50+ only, ~8% efficiency gain)
    #[arg(long, default_value = "hq")]
    av1_tune: String,

    /// AV1: Enable lookahead (improves quality)
    #[arg(long)]
    av1_lookahead: bool,

    /// AV1: Lookahead depth in frames (default 20, RTX 50: up to 250)
    #[arg(long, default_value = "20")]
    av1_lookahead_depth: u32,

    /// AV1: Enable spatial adaptive quantization
    #[arg(long)]
    av1_spatial_aq: bool,

    /// AV1: Enable temporal adaptive quantization (RTX 50+, ~4-5% efficiency)
    #[arg(long)]
    av1_temporal_aq: bool,

    /// AV1: Chroma format (420, 422, 444)
    /// 422 and 444 require RTX 50 series
    #[arg(long, default_value = "420")]
    av1_chroma: String,

    /// AV1: Enable B-frame reference mode (RTX 50+, better compression)
    #[arg(long)]
    av1_b_ref: bool,

    /// AV1: GOP length override (default: auto based on fps)
    #[arg(long)]
    av1_gop: Option<u32>,

    /// Auto-detect GPU and enable RTX 50 UHQ features when available
    #[arg(long)]
    av1_auto: bool,
}

/// Start a capture session
pub async fn cast(args: CastArgs) -> Result<()> {
    println!("Nitrogen - Starting Capture\n");

    // Check if another instance is already running
    if daemon_running().await {
        eprintln!("Error: Another nitrogen instance is already running.");
        eprintln!("Use 'nitrogen stop' to stop it first, or 'nitrogen status' to check its state.");
        return Err(anyhow::anyhow!("Another instance is running"));
    }

    // Load config file for defaults
    let file_config = ConfigFile::load_or_default();
    debug!("Loaded config, using file defaults where CLI args are default");

    // Use config file values when CLI args are at their default values
    // This allows config file to set new defaults while CLI can still override
    let preset_str = if args.preset == "1080p60" {
        &file_config.defaults.preset
    } else {
        &args.preset
    };

    let codec_str = if args.codec == "h264" {
        &file_config.defaults.codec
    } else {
        &args.codec
    };

    let quality_str = if args.quality == "medium" {
        &file_config.encoder.quality
    } else {
        &args.quality
    };

    let bitrate = if args.bitrate == 0 {
        file_config.defaults.bitrate
    } else {
        args.bitrate
    };

    let camera_name = if args.camera_name == "Nitrogen Camera" {
        &file_config.camera.name
    } else {
        &args.camera_name
    };

    let low_latency = if args.no_low_latency {
        false
    } else {
        file_config.defaults.low_latency
    };

    let gpu = if args.gpu == 0 {
        file_config.encoder.gpu
    } else {
        args.gpu
    };

    // Parse preset - check for custom resolution/fps first
    let preset = if args.resolution.is_some() || args.fps.is_some() {
        // Custom resolution/fps specified
        let base_preset = Preset::from_preset_str(preset_str).unwrap_or(Preset::P1080_60);
        let (base_width, base_height) = base_preset.resolution();
        let base_fps = base_preset.fps();

        // Parse custom resolution if provided
        let (width, height) = if let Some(ref res) = args.resolution {
            parse_resolution(res)?
        } else {
            (base_width, base_height)
        };

        // Use custom fps if provided
        let fps = args.fps.unwrap_or(base_fps);

        Preset::Custom { width, height, fps }
    } else {
        Preset::from_preset_str(preset_str).ok_or_else(|| {
            anyhow::anyhow!(
                "Invalid preset '{}'. Valid options: 720p30, 720p60, 1080p30, 1080p60, 1440p30, 1440p60, 1440p120, 4k30, 4k60, 4k120",
                preset_str
            )
        })?
    };

    // Parse codec
    let codec: Codec = codec_str.parse().map_err(|_| {
        anyhow::anyhow!(
            "Invalid codec '{}'. Valid options: h264, hevc, av1",
            codec_str
        )
    })?;

    // Parse encoder preset
    let encoder_preset = match quality_str.to_lowercase().as_str() {
        "fast" => EncoderPreset::Fast,
        "medium" => EncoderPreset::Medium,
        "slow" => EncoderPreset::Slow,
        "quality" => EncoderPreset::Quality,
        _ => {
            return Err(anyhow::anyhow!(
                "Invalid quality '{}'. Valid options: fast, medium, slow, quality",
                quality_str
            ))
        }
    };

    // Parse audio source - use config file default if CLI is default
    let audio_source_str = if args.audio == "none" {
        &file_config.audio.source
    } else {
        &args.audio
    };
    let audio_source = match audio_source_str.to_lowercase().as_str() {
        "none" => AudioSource::None,
        "desktop" => AudioSource::Desktop,
        "mic" | "microphone" => AudioSource::Microphone,
        "both" => AudioSource::Both,
        _ => {
            return Err(anyhow::anyhow!(
                "Invalid audio source '{}'. Valid options: none, desktop, mic, both",
                audio_source_str
            ))
        }
    };

    // Parse audio codec
    let audio_codec_str = if args.audio_codec == "aac" {
        &file_config.audio.codec
    } else {
        &args.audio_codec
    };
    let audio_codec = match audio_codec_str.to_lowercase().as_str() {
        "aac" => AudioCodec::Aac,
        "opus" => AudioCodec::Opus,
        _ => {
            return Err(anyhow::anyhow!(
                "Invalid audio codec '{}'. Valid options: aac, opus",
                audio_codec_str
            ))
        }
    };

    // Audio bitrate (0 = use codec default)
    let audio_bitrate = if args.audio_bitrate == 0 {
        file_config.audio.bitrate
    } else {
        args.audio_bitrate
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

    // Build AV1 configuration
    let av1_config = if codec == Codec::Av1 {
        // Parse AV1 tier
        let av1_tier = match args.av1_tier.to_lowercase().as_str() {
            "main" => Av1Tier::Main,
            "high" => Av1Tier::High,
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid AV1 tier '{}'. Valid options: main, high",
                    args.av1_tier
                ))
            }
        };

        // Parse AV1 tune
        let av1_tune = match args.av1_tune.to_lowercase().as_str() {
            "hq" => Av1Tune::Hq,
            "uhq" => Av1Tune::Uhq,
            "ll" => Av1Tune::Ll,
            "ull" => Av1Tune::Ull,
            "lossless" => Av1Tune::Lossless,
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid AV1 tune '{}'. Valid options: hq, uhq, ll, ull, lossless",
                    args.av1_tune
                ))
            }
        };

        // Parse chroma format
        let av1_chroma = match args.av1_chroma.as_str() {
            "420" => ChromaFormat::Yuv420,
            "422" => ChromaFormat::Yuv422,
            "444" => ChromaFormat::Yuv444,
            _ => {
                return Err(anyhow::anyhow!(
                    "Invalid AV1 chroma format '{}'. Valid options: 420, 422, 444",
                    args.av1_chroma
                ))
            }
        };

        // Auto-detect RTX 50 features if requested
        let (final_tune, final_temporal_aq, final_chroma, final_b_ref, final_lookahead_depth) =
            if args.av1_auto {
                match detect_rtx50_features(gpu) {
                    Ok(features) => {
                        info!("Detected RTX 50 series: {:?}", features);
                        println!("  RTX 50 detected: Enabling UHQ features...");
                        (
                            if features.uhq_supported {
                                Av1Tune::Uhq
                            } else {
                                av1_tune
                            },
                            features.temporal_aq_supported,
                            if features.yuv422_supported {
                                av1_chroma
                            } else {
                                ChromaFormat::Yuv420
                            },
                            features.b_ref_supported,
                            if features.extended_lookahead {
                                args.av1_lookahead_depth.max(100)
                            } else {
                                args.av1_lookahead_depth
                            },
                        )
                    }
                    Err(e) => {
                        debug!("RTX 50 detection failed: {}, using defaults", e);
                        (av1_tune, args.av1_temporal_aq, av1_chroma, args.av1_b_ref, args.av1_lookahead_depth)
                    }
                }
            } else {
                (av1_tune, args.av1_temporal_aq, av1_chroma, args.av1_b_ref, args.av1_lookahead_depth)
            };

        Av1Config {
            ten_bit: args.av1_10bit,
            tier: av1_tier,
            gop_length: args.av1_gop,
            lookahead: args.av1_lookahead,
            lookahead_depth: final_lookahead_depth,
            spatial_aq: args.av1_spatial_aq,
            temporal_aq: final_temporal_aq,
            tune: final_tune,
            chroma: final_chroma,
            multipass: MultipassMode::Disabled,
            b_ref_mode: final_b_ref,
        }
    } else {
        Av1Config::default()
    };

    // Build configuration
    let config = CaptureConfig {
        source,
        preset,
        codec,
        bitrate,
        encoder_preset,
        camera_name: camera_name.to_string(),
        low_latency,
        gpu,
        record_path: args.record.as_ref().map(std::path::PathBuf::from),
        cursor_mode: nitrogen_core::config::CursorMode::Embedded,
        audio_source,
        av1: av1_config,
        audio_codec,
        audio_bitrate,
    };

    // Validate configuration
    if let Err(e) = config.validate_strict() {
        return Err(anyhow::anyhow!("Invalid configuration: {}", e));
    }

    // Check for warnings
    let warnings = config.validate();
    if !warnings.is_empty() {
        println!("Warnings:");
        for warning in &warnings {
            println!("  \u{26a0}  {}", warning);
        }
        println!();
    }

    println!("Configuration:");
    println!("  Preset:      {}", config.preset);
    println!("  Resolution:  {}x{}", config.width(), config.height());
    println!("  Framerate:   {} fps", config.fps());
    println!("  Codec:       {}", config.codec);
    println!("  Bitrate:     {} kbps", config.effective_bitrate());
    println!("  Camera:      {}", config.camera_name);
    println!("  Low Latency: {}", config.low_latency);
    println!("  GPU:         {}", config.gpu);
    if config.audio_source != AudioSource::None {
        let effective_audio_bitrate = if config.audio_bitrate == 0 {
            config.audio_codec.default_bitrate()
        } else {
            config.audio_bitrate
        };
        println!(
            "  Audio:       {:?} ({}, {} kbps)",
            config.audio_source, config.audio_codec, effective_audio_bitrate
        );
    }
    if let Some(ref path) = config.record_path {
        println!("  Recording:   {:?}", path);
    }
    println!();

    // Create pipeline
    let pipeline = Pipeline::new(config)
        .await
        .context("Failed to create pipeline")?;

    // Wrap pipeline in Arc<RwLock> for sharing with IPC server
    let pipeline = Arc::new(RwLock::new(Some(pipeline)));

    // Start IPC server (unless --no-daemon)
    let ipc_server = if !args.no_daemon {
        let mut server = IpcServer::new(pipeline.clone()).context("Failed to create IPC server")?;
        server.start().await.context("Failed to start IPC server")?;
        println!("IPC server listening at {:?}", socket_path());
        Some(server)
    } else {
        println!("Running in foreground-only mode (no IPC server)");
        None
    };

    println!("Waiting for source selection...");
    println!("(A dialog should appear to select your screen or window)\n");

    // Start pipeline (will prompt user via portal)
    let session = {
        let mut guard = pipeline.write().await;
        let p = guard
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Pipeline was unexpectedly None"))?;
        p.start().await.context("Failed to start pipeline")?
    };

    println!("Capture started!");
    println!("  Source:     {:?}", session.source_type);
    println!("  Resolution: {}x{}", session.width, session.height);
    println!("  Node ID:    {}", session.node_id);
    println!();
    println!("Virtual camera '{}' is now available.", camera_name);
    println!("Select it in Discord or other applications to start streaming.");
    println!();

    if !args.no_daemon {
        println!("Use 'nitrogen status' to check stats, 'nitrogen stop' to stop.");
    }
    println!("Press Ctrl+C to stop...\n");

    // Wait for Ctrl+C or IPC shutdown
    let ctrl_c = async {
        signal::ctrl_c().await.expect("Failed to listen for Ctrl+C");
    };

    // Main processing loop
    let pipeline_clone = pipeline.clone();
    let process_loop = async move {
        loop {
            // Process frames
            let mut guard = pipeline_clone.write().await;
            if let Some(ref mut p) = *guard {
                if !p.is_running() {
                    info!("Pipeline is no longer running");
                    break;
                }
                match p.process().await {
                    Ok(true) => {} // Continue
                    Ok(false) => {
                        info!("Pipeline processing ended");
                        break;
                    }
                    Err(e) => {
                        error!("Pipeline error: {}", e);
                        break;
                    }
                }
            } else {
                break;
            }
        }
    };

    // Handle IPC connections while processing
    if let Some(ref server) = ipc_server {
        let ipc_loop = async {
            loop {
                match server.accept_one().await {
                    Ok(true) => {} // Continue
                    Ok(false) => {
                        info!("IPC server signaled shutdown");
                        break;
                    }
                    Err(e) => {
                        warn!("IPC error: {}", e);
                    }
                }
            }
        };

        tokio::select! {
            _ = ctrl_c => {
                println!("\nReceived interrupt signal...");
            }
            _ = process_loop => {
                info!("Processing loop ended");
            }
            _ = ipc_loop => {
                info!("IPC loop ended (shutdown requested)");
            }
        }
    } else {
        tokio::select! {
            _ = ctrl_c => {
                println!("\nReceived interrupt signal...");
            }
            _ = process_loop => {
                info!("Processing loop ended");
            }
        }
    }

    // Stop pipeline
    println!("Stopping capture...");
    {
        let mut guard = pipeline.write().await;
        if let Some(ref mut p) = *guard {
            p.stop().await?;
        }
        *guard = None;
    }

    // Clean up IPC server
    if let Some(server) = ipc_server {
        server.cleanup();
    }

    println!("Capture stopped.");

    Ok(())
}

/// Parse a resolution string like "1920x1080" or "2560x1440"
fn parse_resolution(s: &str) -> Result<(u32, u32)> {
    let parts: Vec<&str> = s.split('x').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!(
            "Invalid resolution '{}'. Use format WIDTHxHEIGHT (e.g., 1920x1080)",
            s
        ));
    }

    let width: u32 = parts[0]
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid width '{}' in resolution", parts[0]))?;

    let height: u32 = parts[1]
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid height '{}' in resolution", parts[1]))?;

    // Ensure dimensions are reasonable
    if width == 0 || height == 0 {
        return Err(anyhow::anyhow!(
            "Resolution dimensions must be greater than 0"
        ));
    }

    if width > 7680 || height > 4320 {
        return Err(anyhow::anyhow!(
            "Resolution {}x{} exceeds maximum (7680x4320)",
            width,
            height
        ));
    }

    // Ensure dimensions are even (required for video encoding)
    let width = width & !1;
    let height = height & !1;

    Ok((width, height))
}

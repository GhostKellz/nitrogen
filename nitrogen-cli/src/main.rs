//! Nitrogen CLI
//!
//! Wayland-native NVIDIA streaming for Discord and friends.
//!
//! # Usage
//!
//! ```bash
//! # List available sources
//! nitrogen list-sources
//!
//! # Start casting a monitor
//! nitrogen cast --preset 1080p60
//!
//! # Stop casting
//! nitrogen stop
//! ```

mod commands;

use clap::{Parser, Subcommand};
use tracing::Level;
use tracing_subscriber::EnvFilter;

/// Nitrogen - Wayland-native NVIDIA streaming for Discord
#[derive(Parser)]
#[command(name = "nitrogen")]
#[command(author = "GhostKellz")]
#[command(version)]
#[command(about = "Wayland-native NVIDIA streaming for Discord and friends", long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Enable verbose logging
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Subcommand to run
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List available capture sources
    #[command(alias = "ls")]
    ListSources,

    /// Start capturing and streaming to virtual camera
    Cast(commands::CastArgs),

    /// Stop the current capture session
    Stop(commands::StopArgs),

    /// Show status of running capture
    Status,

    /// Show system information and NVENC capabilities
    Info,

    /// Manage configuration file
    Config(commands::ConfigArgs),
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Set up logging based on verbosity
    let level = match cli.verbose {
        0 => Level::WARN,
        1 => Level::INFO,
        2 => Level::DEBUG,
        _ => Level::TRACE,
    };

    let filter = EnvFilter::from_default_env()
        .add_directive(
            format!("nitrogen={}", level)
                .parse()
                .unwrap_or_else(|_| format!("nitrogen=warn").parse().expect("default directive")),
        );

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();

    // Run the appropriate command
    let result = match cli.command {
        Commands::ListSources => commands::list_sources().await,
        Commands::Cast(args) => commands::cast(args).await,
        Commands::Stop(args) => commands::stop(args).await,
        Commands::Status => commands::status().await,
        Commands::Info => commands::info().await,
        Commands::Config(args) => commands::config(args).await,
    };

    if let Err(e) = result {
        print_error(&e);
        std::process::exit(1);
    }
}

/// Print an error with helpful hints when available
fn print_error(error: &anyhow::Error) {
    eprintln!("Error: {}", error);

    // Check if we can provide a helpful hint
    // Walk the error chain looking for NitrogenError
    for cause in error.chain() {
        if let Some(nitrogen_err) = cause.downcast_ref::<nitrogen_core::error::NitrogenError>() {
            if let Some(hint) = nitrogen_err.user_hint() {
                eprintln!();
                eprintln!("Hint:");
                for line in hint.lines() {
                    eprintln!("  {}", line);
                }
            }
            return;
        }
    }

    // Generic hints for common error patterns
    let err_str = error.to_string().to_lowercase();

    if err_str.contains("permission denied") {
        eprintln!();
        eprintln!("Hint: Check file/device permissions. You may need to add your user to the 'video' group.");
    } else if err_str.contains("connection refused")
        || err_str.contains("no such file") && err_str.contains("socket")
    {
        eprintln!();
        eprintln!("Hint: The nitrogen daemon may not be running. Start a capture session with: nitrogen cast");
    } else if err_str.contains("nvenc") || err_str.contains("nvidia") {
        eprintln!();
        eprintln!("Hint: Ensure NVIDIA drivers are installed and your GPU supports NVENC.");
        eprintln!("  Try: nvidia-smi");
    }
}

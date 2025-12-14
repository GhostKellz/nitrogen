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

use anyhow::Result;
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
    Stop,

    /// Show status of running capture
    Status,

    /// Show system information and NVENC capabilities
    Info,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set up logging based on verbosity
    let level = match cli.verbose {
        0 => Level::WARN,
        1 => Level::INFO,
        2 => Level::DEBUG,
        _ => Level::TRACE,
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::from_default_env()
                .add_directive(format!("nitrogen={}", level).parse().unwrap()),
        )
        .with_target(false)
        .init();

    // Run the appropriate command
    match cli.command {
        Commands::ListSources => commands::list_sources().await?,
        Commands::Cast(args) => commands::cast(args).await?,
        Commands::Stop => commands::stop().await?,
        Commands::Status => commands::status().await?,
        Commands::Info => commands::info().await?,
    }

    Ok(())
}

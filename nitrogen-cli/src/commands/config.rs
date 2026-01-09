//! Config command - manage configuration files

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use nitrogen_core::config::{sample_config, ConfigFile};

/// Arguments for the config command
#[derive(Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Show the path to the config file
    Path,

    /// Show the current configuration
    Show,

    /// Generate a default config file
    Init {
        /// Force overwrite if file exists
        #[arg(short, long)]
        force: bool,
    },

    /// Print a sample configuration to stdout
    Sample,
}

/// Run config subcommand
pub async fn config(args: ConfigArgs) -> Result<()> {
    match args.command {
        ConfigCommand::Path => {
            let path = ConfigFile::default_path();
            println!("{}", path.display());
            if path.exists() {
                println!("(file exists)");
            } else {
                println!("(file does not exist)");
            }
        }
        ConfigCommand::Show => {
            let path = ConfigFile::default_path();
            if !path.exists() {
                println!("No configuration file found at: {}", path.display());
                println!();
                println!("Using default settings. Create a config file with:");
                println!("  nitrogen config init");
                return Ok(());
            }

            let content = std::fs::read_to_string(&path).context("Failed to read config file")?;

            println!("Configuration file: {}\n", path.display());
            println!("{}", content);
        }
        ConfigCommand::Init { force } => {
            let path = ConfigFile::default_path();

            if path.exists() && !force {
                println!("Configuration file already exists: {}", path.display());
                println!();
                println!("Use --force to overwrite, or edit the existing file.");
                return Ok(());
            }

            // Create parent directory if needed
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent).context("Failed to create config directory")?;
                }
            }

            // Write sample config
            std::fs::write(&path, sample_config()).context("Failed to write config file")?;

            println!("Created configuration file: {}", path.display());
            println!();
            println!("Edit this file to customize Nitrogen settings.");
        }
        ConfigCommand::Sample => {
            print!("{}", sample_config());
        }
    }

    Ok(())
}

//! List sources command

use anyhow::Result;
use nitrogen_core::capture;

/// List available capture sources
pub async fn list_sources() -> Result<()> {
    println!("Nitrogen - Available Capture Sources\n");

    let sources = capture::list_sources().await?;

    if sources.is_empty() {
        println!("No sources found.");
        println!("\nNote: On Wayland, source selection happens through the desktop portal.");
        println!("Use 'nitrogen cast' to start a capture session and select a source.");
        return Ok(());
    }

    println!(
        "{:<20} {:<30} {:<10} {:<15}",
        "ID", "Name", "Type", "Resolution"
    );
    println!("{}", "-".repeat(75));

    for source in sources {
        let dims = if source.dimensions.0 > 0 {
            format!("{}x{}", source.dimensions.0, source.dimensions.1)
        } else {
            "Unknown".to_string()
        };

        let refresh = source
            .refresh_rate
            .map(|r| format!(" @ {:.0}Hz", r))
            .unwrap_or_default();

        println!(
            "{:<20} {:<30} {:<10} {}{}",
            source.id,
            truncate(&source.name, 28),
            source.kind,
            dims,
            refresh
        );
    }

    println!("\nNote: On Wayland, use 'nitrogen cast' to start capturing.");
    println!("The desktop portal will prompt you to select a screen or window.");

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}

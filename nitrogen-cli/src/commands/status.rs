//! Status command - show status of running capture

use anyhow::Result;
use nitrogen_core::{daemon_running, socket_path, IpcClient};

/// Show status of running capture
pub async fn status() -> Result<()> {
    println!("Nitrogen - Status\n");

    // Check if daemon is running
    if !daemon_running().await {
        println!("Status: Not running");
        println!();
        println!("Start a capture session with: nitrogen cast");
        return Ok(());
    }

    // Connect to daemon
    let mut client = match IpcClient::connect().await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to connect to daemon: {}", e);
            eprintln!("Socket: {:?}", socket_path());
            return Err(e.into());
        }
    };

    // Get status
    match client.status().await {
        Ok(status) => {
            println!(
                "Status: {}",
                if status.running { "Running" } else { "Idle" }
            );
            println!("State:  {}", status.state);
            println!("PID:    {}", status.pid);
            println!("Uptime: {:.1}s", status.uptime_seconds);

            if let Some((w, h)) = status.resolution {
                println!();
                println!("Resolution: {}x{}", w, h);
            }

            if let Some(fps) = status.fps {
                println!("Target FPS: {}", fps);
            }

            if let Some(ref camera) = status.camera_name {
                println!("Camera:     {}", camera);
            }

            if let Some(ref source) = status.source {
                println!("Source:     {}", source);
            }

            // Try to get stats too
            if let Ok(stats) = client.stats().await {
                println!();
                println!("Encoding:");
                println!("  Codec:   {}", stats.codec);
                println!("  Bitrate: {} kbps", stats.bitrate);
                println!("  Output:  {}x{}", stats.resolution.0, stats.resolution.1);

                println!();
                println!("Performance:");
                println!(
                    "  Actual FPS: {:.1} / {} target",
                    stats.actual_fps, stats.target_fps
                );
                println!("  Elapsed:    {:.1}s", stats.elapsed_seconds);

                println!();
                println!("Frame Statistics:");
                println!("  Processed: {}", stats.frames_processed);

                // Show dropped frames with warning if > 1%
                let total_frames =
                    stats.frames_processed + stats.frames_dropped + stats.frames_failed;
                if total_frames > 0 {
                    let drop_pct = (stats.frames_dropped as f64 / total_frames as f64) * 100.0;
                    let fail_pct = (stats.frames_failed as f64 / total_frames as f64) * 100.0;

                    if stats.frames_dropped > 0 {
                        let warning = if drop_pct > 5.0 {
                            " (HIGH)"
                        } else if drop_pct > 1.0 {
                            " (warning)"
                        } else {
                            ""
                        };
                        println!(
                            "  Dropped:   {} ({:.1}%){}",
                            stats.frames_dropped, drop_pct, warning
                        );
                    } else {
                        println!("  Dropped:   0");
                    }

                    if stats.frames_failed > 0 {
                        let warning = if fail_pct > 1.0 { " (WARNING)" } else { "" };
                        println!(
                            "  Failed:    {} ({:.1}%){}",
                            stats.frames_failed, fail_pct, warning
                        );
                    } else {
                        println!("  Failed:    0");
                    }
                } else {
                    println!("  Dropped:   0");
                    println!("  Failed:    0");
                }

                // Show health summary
                if stats.frames_dropped > 0 || stats.frames_failed > 0 {
                    println!();
                    if stats.frames_dropped > stats.frames_processed / 20 {
                        println!("Note: High frame drop rate. Consider:");
                        println!("  - Lowering resolution/framerate");
                        println!("  - Reducing bitrate");
                        println!("  - Using 'fast' quality preset");
                    }
                    if stats.frames_failed > 0 {
                        println!("Note: Frame encoding failures detected. Check GPU load.");
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to get status: {}", e);
            return Err(e.into());
        }
    }

    println!();
    println!("Socket: {:?}", socket_path());

    Ok(())
}

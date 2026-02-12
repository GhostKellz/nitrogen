//! Environment detection for Nitrogen
//!
//! Detects runtime environment including:
//! - Gamescope (Steam Deck gaming mode, nested compositors)
//! - Steam Deck hardware
//! - Wayland compositors (KDE, Hyprland, Sway, GNOME, etc.)
//! - Session type (Wayland, X11)

use std::env;
use std::fs;
use tracing::{debug, info};

/// Detected runtime environment
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeEnvironment {
    /// Running under Gamescope compositor
    Gamescope(GamescopeInfo),
    /// Native Wayland session
    NativeWayland(WaylandInfo),
    /// X11 session (not recommended)
    X11,
    /// Unknown environment
    Unknown,
}

/// Information about Gamescope environment
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamescopeInfo {
    /// Running on Steam Deck hardware
    pub steam_deck: bool,
    /// Gamescope display identifier
    pub display: Option<String>,
    /// Running as nested compositor (under another Wayland compositor)
    pub nested: bool,
}

/// Information about native Wayland environment
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaylandInfo {
    /// Desktop environment/compositor name
    pub compositor: String,
    /// Wayland display socket
    pub display: Option<String>,
}

/// Optimizations to apply based on environment
#[derive(Debug, Clone, Default)]
pub struct EnvironmentOptimizations {
    /// Use lower latency encode settings
    pub reduce_latency: bool,
    /// Prefer FSR-compatible resolutions
    pub fsr_compatible_res: bool,
    /// Use direct DRM capture if available (bypasses portal)
    pub prefer_drm_capture: bool,
    /// Automatically select optimal preset for hardware
    pub auto_preset: bool,
    /// Suggested encoder preset override
    pub encoder_preset_hint: Option<String>,
}

impl RuntimeEnvironment {
    /// Get optimizations for this environment
    pub fn optimizations(&self) -> EnvironmentOptimizations {
        match self {
            RuntimeEnvironment::Gamescope(info) => {
                let mut opts = EnvironmentOptimizations {
                    reduce_latency: true,
                    fsr_compatible_res: true,
                    // DRM capture would provide lower latency but requires implementation
                    // See nitrogen_core::capture::drm for status and plans
                    prefer_drm_capture: false,
                    auto_preset: true,
                    encoder_preset_hint: Some("fast".to_string()),
                };

                // Steam Deck specific optimizations
                if info.steam_deck {
                    opts.encoder_preset_hint = Some("fast".to_string());
                }

                opts
            }
            RuntimeEnvironment::NativeWayland(info) => {
                let mut opts = EnvironmentOptimizations::default();

                // Compositor-specific optimizations
                match info.compositor.to_lowercase().as_str() {
                    "hyprland" => {
                        // Hyprland has good DMA-BUF support
                        opts.reduce_latency = true;
                    }
                    "kde" | "kwin" | "plasma" => {
                        // KDE Plasma
                        opts.reduce_latency = true;
                    }
                    "sway" => {
                        // Sway/wlroots
                        opts.reduce_latency = true;
                    }
                    "gnome" | "mutter" => {
                        // GNOME Shell
                        opts.reduce_latency = false; // GNOME has some portal latency
                    }
                    _ => {}
                }

                opts
            }
            RuntimeEnvironment::X11 => EnvironmentOptimizations {
                reduce_latency: false,
                fsr_compatible_res: false,
                prefer_drm_capture: false,
                auto_preset: false,
                encoder_preset_hint: None,
            },
            RuntimeEnvironment::Unknown => EnvironmentOptimizations::default(),
        }
    }

    /// Check if running under Gamescope
    pub fn is_gamescope(&self) -> bool {
        matches!(self, RuntimeEnvironment::Gamescope(_))
    }

    /// Check if running on Steam Deck
    pub fn is_steam_deck(&self) -> bool {
        match self {
            RuntimeEnvironment::Gamescope(info) => info.steam_deck,
            _ => is_steam_deck_hardware(),
        }
    }

    /// Get a human-readable description
    pub fn description(&self) -> String {
        match self {
            RuntimeEnvironment::Gamescope(info) => {
                if info.steam_deck {
                    "Gamescope (Steam Deck)".to_string()
                } else if info.nested {
                    "Gamescope (nested)".to_string()
                } else {
                    "Gamescope".to_string()
                }
            }
            RuntimeEnvironment::NativeWayland(info) => {
                format!("Wayland ({})", info.compositor)
            }
            RuntimeEnvironment::X11 => "X11".to_string(),
            RuntimeEnvironment::Unknown => "Unknown".to_string(),
        }
    }
}

/// Detect the current runtime environment
pub fn detect_environment() -> RuntimeEnvironment {
    // Check for Gamescope first (highest priority)
    if let Some(gamescope) = detect_gamescope() {
        info!("Detected environment: {}", gamescope.description());
        return gamescope;
    }

    // Check for Wayland
    if let Some(wayland) = detect_wayland() {
        info!("Detected environment: {}", wayland.description());
        return wayland;
    }

    // Check for X11
    if env::var("DISPLAY").is_ok() {
        info!("Detected environment: X11");
        return RuntimeEnvironment::X11;
    }

    debug!("Could not detect runtime environment");
    RuntimeEnvironment::Unknown
}

/// Detect Gamescope environment
fn detect_gamescope() -> Option<RuntimeEnvironment> {
    // Check for Gamescope-specific environment variables
    let gamescope_display = env::var("GAMESCOPE_WAYLAND_DISPLAY").ok();
    let gamescope_xwayland = env::var("SteamGamepadUI").is_ok();
    let steam_deck_env = env::var("SteamDeck").ok();

    // Check if running under gamescope by looking at WAYLAND_DISPLAY
    let wayland_display = env::var("WAYLAND_DISPLAY").ok();
    let is_gamescope_display = wayland_display
        .as_ref()
        .map(|d| d.starts_with("gamescope"))
        .unwrap_or(false);

    // Gamescope sets specific env vars
    let has_gamescope_env = gamescope_display.is_some()
        || gamescope_xwayland
        || is_gamescope_display
        || steam_deck_env.is_some();

    if has_gamescope_env {
        let steam_deck = is_steam_deck_hardware() || steam_deck_env.is_some();
        let nested = env::var("WAYLAND_DISPLAY").is_ok() && !is_gamescope_display;

        return Some(RuntimeEnvironment::Gamescope(GamescopeInfo {
            steam_deck,
            display: gamescope_display.or(wayland_display),
            nested,
        }));
    }

    // Also check process list for gamescope (fallback)
    if is_gamescope_running() {
        return Some(RuntimeEnvironment::Gamescope(GamescopeInfo {
            steam_deck: is_steam_deck_hardware(),
            display: env::var("WAYLAND_DISPLAY").ok(),
            nested: true,
        }));
    }

    None
}

/// Detect native Wayland environment
fn detect_wayland() -> Option<RuntimeEnvironment> {
    // Check if this is a Wayland session
    let session_type = env::var("XDG_SESSION_TYPE").ok();
    let wayland_display = env::var("WAYLAND_DISPLAY").ok();

    if session_type.as_deref() != Some("wayland") && wayland_display.is_none() {
        return None;
    }

    // Detect compositor from XDG_CURRENT_DESKTOP or other env vars
    let compositor = detect_compositor();

    Some(RuntimeEnvironment::NativeWayland(WaylandInfo {
        compositor,
        display: wayland_display,
    }))
}

/// Detect the Wayland compositor
fn detect_compositor() -> String {
    // Try XDG_CURRENT_DESKTOP first
    if let Ok(desktop) = env::var("XDG_CURRENT_DESKTOP") {
        let desktop_lower = desktop.to_lowercase();

        // Handle common desktop environments
        if desktop_lower.contains("hyprland") {
            return "Hyprland".to_string();
        }
        if desktop_lower.contains("kde") || desktop_lower.contains("plasma") {
            return "KDE Plasma".to_string();
        }
        if desktop_lower.contains("gnome") {
            return "GNOME".to_string();
        }
        if desktop_lower.contains("sway") {
            return "Sway".to_string();
        }
        if desktop_lower.contains("wlroots") {
            return "wlroots".to_string();
        }
        if desktop_lower.contains("cosmic") {
            return "COSMIC".to_string();
        }

        return desktop;
    }

    // Try XDG_SESSION_DESKTOP
    if let Ok(session) = env::var("XDG_SESSION_DESKTOP") {
        return session;
    }

    // Try DESKTOP_SESSION
    if let Ok(session) = env::var("DESKTOP_SESSION") {
        return session;
    }

    // Fallback: check for compositor-specific env vars
    if env::var("HYPRLAND_INSTANCE_SIGNATURE").is_ok() {
        return "Hyprland".to_string();
    }
    if env::var("SWAYSOCK").is_ok() {
        return "Sway".to_string();
    }
    if env::var("I3SOCK").is_ok() {
        return "i3".to_string();
    }

    "Unknown".to_string()
}

/// Check if running on Steam Deck hardware
pub fn is_steam_deck_hardware() -> bool {
    // Check /etc/os-release for SteamOS
    if let Ok(content) = fs::read_to_string("/etc/os-release") {
        if content.to_lowercase().contains("steamos") {
            return true;
        }
    }

    // Check for Steam Deck specific files
    if fs::metadata("/sys/devices/virtual/dmi/id/product_name")
        .map(|m| m.is_file())
        .unwrap_or(false)
    {
        if let Ok(product) = fs::read_to_string("/sys/devices/virtual/dmi/id/product_name") {
            if product.trim().to_lowercase().contains("jupiter")
                || product.trim().to_lowercase().contains("galileo")
            {
                return true;
            }
        }
    }

    // Check for deck-specific device
    fs::metadata("/dev/deck_card").is_ok()
}

/// Check if gamescope process is running
fn is_gamescope_running() -> bool {
    // Check /proc for gamescope process
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let path = entry.path();
            // Check if this is a numeric directory (PID)
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.chars().all(|c| c.is_ascii_digit()) {
                    let comm_path = path.join("comm");
                    if let Ok(comm) = fs::read_to_string(comm_path) {
                        if comm.trim() == "gamescope" {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Get the current Wayland display socket
pub fn wayland_display() -> Option<String> {
    env::var("WAYLAND_DISPLAY").ok()
}

/// Get the current X11 display
pub fn x11_display() -> Option<String> {
    env::var("DISPLAY").ok()
}

/// Check if PipeWire is available
pub fn is_pipewire_available() -> bool {
    // Check for PipeWire socket
    if let Some(runtime_dir) = env::var("XDG_RUNTIME_DIR").ok() {
        let pipewire_socket = format!("{}/pipewire-0", runtime_dir);
        if fs::metadata(&pipewire_socket).is_ok() {
            return true;
        }
    }

    // Fallback: check if pipewire service is running
    std::process::Command::new("systemctl")
        .args(["--user", "is-active", "pipewire"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_environment_returns_value() {
        // Should return some environment (may vary by test runner)
        let env = detect_environment();
        // Just verify it doesn't panic
        let _ = env.description();
    }

    #[test]
    fn test_environment_optimizations() {
        let gamescope = RuntimeEnvironment::Gamescope(GamescopeInfo {
            steam_deck: true,
            display: None,
            nested: false,
        });
        let opts = gamescope.optimizations();
        assert!(opts.reduce_latency);
        assert!(opts.fsr_compatible_res);
    }

    #[test]
    fn test_is_gamescope() {
        let gamescope = RuntimeEnvironment::Gamescope(GamescopeInfo {
            steam_deck: false,
            display: None,
            nested: true,
        });
        assert!(gamescope.is_gamescope());

        let wayland = RuntimeEnvironment::NativeWayland(WaylandInfo {
            compositor: "KDE".to_string(),
            display: None,
        });
        assert!(!wayland.is_gamescope());
    }

    #[test]
    fn test_environment_description() {
        let deck = RuntimeEnvironment::Gamescope(GamescopeInfo {
            steam_deck: true,
            display: None,
            nested: false,
        });
        assert!(deck.description().contains("Steam Deck"));

        let kde = RuntimeEnvironment::NativeWayland(WaylandInfo {
            compositor: "KDE Plasma".to_string(),
            display: Some("wayland-0".to_string()),
        });
        assert!(kde.description().contains("KDE"));
    }
}

//! Global hotkey support via evdev
//!
//! Provides system-wide hotkey detection for controlling nitrogen.
//! Works by reading keyboard events directly from /dev/input/event* devices.

use evdev::{Device, InputEventKind, Key};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace, warn};

use crate::error::{NitrogenError, Result};

/// A hotkey action
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HotkeyAction {
    /// Toggle capture on/off
    Toggle,
    /// Start capture
    Start,
    /// Stop capture
    Stop,
    /// Pause/resume capture
    Pause,
    /// Toggle recording
    ToggleRecording,
}

/// A hotkey binding (modifier keys + trigger key)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotkey {
    /// Modifier keys that must be held
    pub modifiers: HashSet<Key>,
    /// The trigger key
    pub key: Key,
    /// The action to perform
    pub action: HotkeyAction,
}

impl Hotkey {
    /// Create a new hotkey binding
    pub fn new(modifiers: impl IntoIterator<Item = Key>, key: Key, action: HotkeyAction) -> Self {
        Self {
            modifiers: modifiers.into_iter().collect(),
            key,
            action,
        }
    }

    /// Parse a hotkey string like "ctrl+shift+f9"
    pub fn parse(s: &str, action: HotkeyAction) -> Result<Self> {
        let lowercase = s.to_lowercase();
        let parts: Vec<&str> = lowercase.split('+').collect();
        if parts.is_empty() {
            return Err(NitrogenError::config("Empty hotkey string"));
        }

        let mut modifiers = HashSet::new();
        let mut key = None;

        for part in parts {
            let part = part.trim();
            match part {
                "ctrl" | "control" => {
                    modifiers.insert(Key::KEY_LEFTCTRL);
                }
                "alt" => {
                    modifiers.insert(Key::KEY_LEFTALT);
                }
                "shift" => {
                    modifiers.insert(Key::KEY_LEFTSHIFT);
                }
                "super" | "meta" | "win" => {
                    modifiers.insert(Key::KEY_LEFTMETA);
                }
                _ => {
                    // Try to parse as a key
                    key = Some(parse_key(part)?);
                }
            }
        }

        let key = key.ok_or_else(|| NitrogenError::config("No key specified in hotkey"))?;

        Ok(Self {
            modifiers,
            key,
            action,
        })
    }
}

/// Parse a key name to evdev Key
fn parse_key(name: &str) -> Result<Key> {
    let key = match name.to_lowercase().as_str() {
        // Function keys
        "f1" => Key::KEY_F1,
        "f2" => Key::KEY_F2,
        "f3" => Key::KEY_F3,
        "f4" => Key::KEY_F4,
        "f5" => Key::KEY_F5,
        "f6" => Key::KEY_F6,
        "f7" => Key::KEY_F7,
        "f8" => Key::KEY_F8,
        "f9" => Key::KEY_F9,
        "f10" => Key::KEY_F10,
        "f11" => Key::KEY_F11,
        "f12" => Key::KEY_F12,

        // Number keys
        "1" => Key::KEY_1,
        "2" => Key::KEY_2,
        "3" => Key::KEY_3,
        "4" => Key::KEY_4,
        "5" => Key::KEY_5,
        "6" => Key::KEY_6,
        "7" => Key::KEY_7,
        "8" => Key::KEY_8,
        "9" => Key::KEY_9,
        "0" => Key::KEY_0,

        // Letters
        "a" => Key::KEY_A,
        "b" => Key::KEY_B,
        "c" => Key::KEY_C,
        "d" => Key::KEY_D,
        "e" => Key::KEY_E,
        "f" => Key::KEY_F,
        "g" => Key::KEY_G,
        "h" => Key::KEY_H,
        "i" => Key::KEY_I,
        "j" => Key::KEY_J,
        "k" => Key::KEY_K,
        "l" => Key::KEY_L,
        "m" => Key::KEY_M,
        "n" => Key::KEY_N,
        "o" => Key::KEY_O,
        "p" => Key::KEY_P,
        "q" => Key::KEY_Q,
        "r" => Key::KEY_R,
        "s" => Key::KEY_S,
        "t" => Key::KEY_T,
        "u" => Key::KEY_U,
        "v" => Key::KEY_V,
        "w" => Key::KEY_W,
        "x" => Key::KEY_X,
        "y" => Key::KEY_Y,
        "z" => Key::KEY_Z,

        // Special keys
        "space" => Key::KEY_SPACE,
        "enter" | "return" => Key::KEY_ENTER,
        "escape" | "esc" => Key::KEY_ESC,
        "tab" => Key::KEY_TAB,
        "backspace" => Key::KEY_BACKSPACE,
        "delete" | "del" => Key::KEY_DELETE,
        "insert" | "ins" => Key::KEY_INSERT,
        "home" => Key::KEY_HOME,
        "end" => Key::KEY_END,
        "pageup" | "pgup" => Key::KEY_PAGEUP,
        "pagedown" | "pgdn" => Key::KEY_PAGEDOWN,
        "up" => Key::KEY_UP,
        "down" => Key::KEY_DOWN,
        "left" => Key::KEY_LEFT,
        "right" => Key::KEY_RIGHT,
        "printscreen" | "print" | "prtsc" => Key::KEY_SYSRQ,
        "pause" => Key::KEY_PAUSE,
        "scrolllock" => Key::KEY_SCROLLLOCK,

        // Numpad
        "numpad0" | "kp0" => Key::KEY_KP0,
        "numpad1" | "kp1" => Key::KEY_KP1,
        "numpad2" | "kp2" => Key::KEY_KP2,
        "numpad3" | "kp3" => Key::KEY_KP3,
        "numpad4" | "kp4" => Key::KEY_KP4,
        "numpad5" | "kp5" => Key::KEY_KP5,
        "numpad6" | "kp6" => Key::KEY_KP6,
        "numpad7" | "kp7" => Key::KEY_KP7,
        "numpad8" | "kp8" => Key::KEY_KP8,
        "numpad9" | "kp9" => Key::KEY_KP9,

        _ => return Err(NitrogenError::config(format!("Unknown key: {}", name))),
    };

    Ok(key)
}

/// Global hotkey listener
pub struct HotkeyListener {
    /// Registered hotkeys
    hotkeys: Vec<Hotkey>,
    /// Action sender
    action_tx: mpsc::Sender<HotkeyAction>,
    /// Running flag
    running: Arc<AtomicBool>,
    /// Listener thread handle
    thread_handle: Option<std::thread::JoinHandle<()>>,
}

impl HotkeyListener {
    /// Create a new hotkey listener
    ///
    /// Returns the listener and a receiver for hotkey actions.
    pub fn new(hotkeys: Vec<Hotkey>) -> Result<(Self, mpsc::Receiver<HotkeyAction>)> {
        let (action_tx, action_rx) = mpsc::channel(16);

        Ok((
            Self {
                hotkeys,
                action_tx,
                running: Arc::new(AtomicBool::new(false)),
                thread_handle: None,
            },
            action_rx,
        ))
    }

    /// Create with default hotkeys
    pub fn with_defaults() -> Result<(Self, mpsc::Receiver<HotkeyAction>)> {
        let hotkeys = vec![
            // Ctrl+Shift+F9 to toggle
            Hotkey::new(
                [Key::KEY_LEFTCTRL, Key::KEY_LEFTSHIFT],
                Key::KEY_F9,
                HotkeyAction::Toggle,
            ),
            // Ctrl+Shift+F10 to pause
            Hotkey::new(
                [Key::KEY_LEFTCTRL, Key::KEY_LEFTSHIFT],
                Key::KEY_F10,
                HotkeyAction::Pause,
            ),
            // Ctrl+Shift+F11 to toggle recording
            Hotkey::new(
                [Key::KEY_LEFTCTRL, Key::KEY_LEFTSHIFT],
                Key::KEY_F11,
                HotkeyAction::ToggleRecording,
            ),
        ];

        Self::new(hotkeys)
    }

    /// Start listening for hotkeys
    pub fn start(&mut self) -> Result<()> {
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }

        let hotkeys = self.hotkeys.clone();
        let action_tx = self.action_tx.clone();
        let running = self.running.clone();

        running.store(true, Ordering::SeqCst);

        let thread = std::thread::Builder::new()
            .name("nitrogen-hotkeys".to_string())
            .spawn(move || {
                if let Err(e) = run_hotkey_loop(hotkeys, action_tx, running.clone()) {
                    error!("Hotkey listener error: {}", e);
                }
                running.store(false, Ordering::SeqCst);
            })
            .map_err(|e| NitrogenError::config(format!("Failed to spawn hotkey thread: {}", e)))?;

        self.thread_handle = Some(thread);
        info!("Hotkey listener started");

        Ok(())
    }

    /// Stop listening for hotkeys
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        info!("Hotkey listener stopped");
    }

    /// Check if the listener is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Drop for HotkeyListener {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Find keyboard devices
fn find_keyboard_devices() -> Vec<Device> {
    let mut devices = Vec::new();

    // Scan /dev/input/event* for keyboard devices
    for entry in std::fs::read_dir("/dev/input").into_iter().flatten() {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.to_string_lossy().contains("event") {
                match Device::open(&path) {
                    Ok(device) => {
                        // Check if this device has keyboard capabilities
                        if device.supported_keys().map_or(false, |keys| {
                            keys.contains(Key::KEY_A) && keys.contains(Key::KEY_ENTER)
                        }) {
                            debug!(
                                "Found keyboard device: {:?} - {}",
                                path,
                                device.name().unwrap_or("unknown")
                            );
                            devices.push(device);
                        }
                    }
                    Err(e) => {
                        trace!("Could not open {:?}: {}", path, e);
                    }
                }
            }
        }
    }

    devices
}

/// Run the hotkey listening loop
fn run_hotkey_loop(
    hotkeys: Vec<Hotkey>,
    action_tx: mpsc::Sender<HotkeyAction>,
    running: Arc<AtomicBool>,
) -> Result<()> {
    let mut devices = find_keyboard_devices();

    if devices.is_empty() {
        warn!("No keyboard devices found. Hotkeys will not work.");
        warn!("Make sure you have permission to read /dev/input/event* devices.");
        warn!("You may need to add your user to the 'input' group.");
        return Err(NitrogenError::config("No keyboard devices found"));
    }

    info!(
        "Monitoring {} keyboard device(s) for hotkeys",
        devices.len()
    );

    // Track pressed keys across all devices
    let mut pressed_keys: HashSet<Key> = HashSet::new();

    // Use polling to check devices
    while running.load(Ordering::SeqCst) {
        for device in &mut devices {
            // Fetch events (non-blocking would be better but this works)
            if let Ok(events) = device.fetch_events() {
                for event in events {
                    if let InputEventKind::Key(key) = event.kind() {
                        match event.value() {
                            1 => {
                                // Key pressed
                                pressed_keys.insert(key);
                                trace!("Key pressed: {:?}", key);

                                // Check if any hotkey matches
                                for hotkey in &hotkeys {
                                    if check_hotkey(&pressed_keys, hotkey) {
                                        info!("Hotkey triggered: {:?}", hotkey.action);
                                        if action_tx.blocking_send(hotkey.action).is_err() {
                                            debug!("Action receiver dropped");
                                        }
                                    }
                                }
                            }
                            0 => {
                                // Key released
                                pressed_keys.remove(&key);
                                trace!("Key released: {:?}", key);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Small sleep to prevent busy-waiting
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    Ok(())
}

/// Check if a hotkey matches the currently pressed keys
fn check_hotkey(pressed: &HashSet<Key>, hotkey: &Hotkey) -> bool {
    // The trigger key must be pressed
    if !pressed.contains(&hotkey.key) {
        return false;
    }

    // All modifiers must be pressed (check both left and right variants)
    for modifier in &hotkey.modifiers {
        let modifier_pressed = match *modifier {
            Key::KEY_LEFTCTRL => {
                pressed.contains(&Key::KEY_LEFTCTRL) || pressed.contains(&Key::KEY_RIGHTCTRL)
            }
            Key::KEY_LEFTALT => {
                pressed.contains(&Key::KEY_LEFTALT) || pressed.contains(&Key::KEY_RIGHTALT)
            }
            Key::KEY_LEFTSHIFT => {
                pressed.contains(&Key::KEY_LEFTSHIFT) || pressed.contains(&Key::KEY_RIGHTSHIFT)
            }
            Key::KEY_LEFTMETA => {
                pressed.contains(&Key::KEY_LEFTMETA) || pressed.contains(&Key::KEY_RIGHTMETA)
            }
            _ => pressed.contains(modifier),
        };

        if !modifier_pressed {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hotkey() {
        let hotkey = Hotkey::parse("ctrl+shift+f9", HotkeyAction::Toggle).unwrap();
        assert_eq!(hotkey.key, Key::KEY_F9);
        assert!(hotkey.modifiers.contains(&Key::KEY_LEFTCTRL));
        assert!(hotkey.modifiers.contains(&Key::KEY_LEFTSHIFT));
        assert_eq!(hotkey.action, HotkeyAction::Toggle);
    }

    #[test]
    fn test_parse_simple_key() {
        let hotkey = Hotkey::parse("f5", HotkeyAction::Start).unwrap();
        assert_eq!(hotkey.key, Key::KEY_F5);
        assert!(hotkey.modifiers.is_empty());
    }

    #[test]
    fn test_parse_key() {
        assert_eq!(parse_key("f9").unwrap(), Key::KEY_F9);
        assert_eq!(parse_key("a").unwrap(), Key::KEY_A);
        assert_eq!(parse_key("space").unwrap(), Key::KEY_SPACE);
        assert!(parse_key("invalid").is_err());
    }

    #[test]
    fn test_check_hotkey() {
        let hotkey = Hotkey::new(
            [Key::KEY_LEFTCTRL, Key::KEY_LEFTSHIFT],
            Key::KEY_F9,
            HotkeyAction::Toggle,
        );

        // Missing modifier
        let pressed: HashSet<Key> = [Key::KEY_LEFTCTRL, Key::KEY_F9].into_iter().collect();
        assert!(!check_hotkey(&pressed, &hotkey));

        // All modifiers + key
        let pressed: HashSet<Key> = [Key::KEY_LEFTCTRL, Key::KEY_LEFTSHIFT, Key::KEY_F9]
            .into_iter()
            .collect();
        assert!(check_hotkey(&pressed, &hotkey));

        // Right ctrl works too
        let pressed: HashSet<Key> = [Key::KEY_RIGHTCTRL, Key::KEY_LEFTSHIFT, Key::KEY_F9]
            .into_iter()
            .collect();
        assert!(check_hotkey(&pressed, &hotkey));
    }
}

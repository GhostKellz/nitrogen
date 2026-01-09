# Global Hotkeys Guide

Nitrogen supports system-wide keyboard shortcuts that work across all applications.

## Default Hotkeys

| Hotkey | Action |
|--------|--------|
| `Ctrl+Shift+F9` | Toggle capture on/off |
| `Ctrl+Shift+F10` | Pause/resume capture |
| `Ctrl+Shift+F11` | Toggle file recording |

## Requirements

### Input Group Membership

Global hotkeys require read access to `/dev/input/event*` devices. Add your user to the `input` group:

```bash
# Add to input group
sudo usermod -aG input $USER

# Log out and back in for change to take effect
```

Verify membership:
```bash
groups | grep input
```

### Verify Device Access

```bash
# List input devices
ls -la /dev/input/event*

# Test keyboard detection (requires sudo if not in input group)
sudo evtest
```

## Configuration

Customize hotkeys in `~/.config/nitrogen/config.toml`:

```toml
[hotkeys]
# Enable/disable hotkeys
enabled = true

# Toggle capture on/off
toggle = "ctrl+shift+f9"

# Pause/resume capture
pause = "ctrl+shift+f10"

# Toggle file recording
record = "ctrl+shift+f11"
```

## Hotkey Format

Format: `modifier+modifier+key`

### Modifiers

| Modifier | Aliases |
|----------|---------|
| `ctrl` | `control` |
| `alt` | - |
| `shift` | - |
| `super` | `meta`, `win` |

### Keys

**Function Keys:**
- `f1` through `f12`

**Letters:**
- `a` through `z` (case insensitive)

**Numbers:**
- `0` through `9`

**Special Keys:**
| Key | Aliases |
|-----|---------|
| `space` | - |
| `enter` | `return` |
| `escape` | `esc` |
| `tab` | - |
| `backspace` | - |
| `delete` | `del` |
| `insert` | `ins` |
| `home` | - |
| `end` | - |
| `pageup` | `pgup` |
| `pagedown` | `pgdn` |
| `up` | - |
| `down` | - |
| `left` | - |
| `right` | - |
| `printscreen` | `print`, `prtsc` |
| `pause` | - |
| `scrolllock` | - |

**Numpad:**
- `numpad0` through `numpad9`
- `kp0` through `kp9`

## Example Configurations

### Alternative Keys (Avoid Compositor Conflicts)
```toml
[hotkeys]
toggle = "ctrl+alt+f9"
pause = "ctrl+alt+f10"
record = "ctrl+alt+f11"
```

### Simple Keys (No Modifiers)
```toml
[hotkeys]
toggle = "pause"
pause = "scrolllock"
record = "insert"
```

### Vim-Style
```toml
[hotkeys]
toggle = "ctrl+shift+c"
pause = "ctrl+shift+p"
record = "ctrl+shift+r"
```

### Numpad
```toml
[hotkeys]
toggle = "numpad7"
pause = "numpad8"
record = "numpad9"
```

## Disable Hotkeys

To disable global hotkeys entirely:

```toml
[hotkeys]
enabled = false
```

## Troubleshooting

### Hotkeys Not Working

1. **Check input group membership:**
   ```bash
   groups
   # Should include 'input'
   ```

2. **Add to input group if missing:**
   ```bash
   sudo usermod -aG input $USER
   # Log out and back in
   ```

3. **Verify device access:**
   ```bash
   ls -la /dev/input/event*
   # Should show read permissions for input group
   ```

4. **Check nitrogen logs:**
   ```bash
   RUST_LOG=nitrogen_core::hotkeys=debug nitrogen cast
   ```

### Compositor Intercepts Hotkey

Some compositors capture certain key combinations before nitrogen sees them.

**Solutions:**

1. **Use different modifiers:**
   ```toml
   toggle = "ctrl+alt+f9"  # Instead of ctrl+shift+f9
   ```

2. **Use function keys alone:**
   ```toml
   toggle = "f9"
   ```

3. **Check compositor keybindings:**
   - Hyprland: `~/.config/hypr/hyprland.conf`
   - Sway: `~/.config/sway/config`
   - KDE: System Settings > Shortcuts

### Left vs Right Modifier Keys

Nitrogen recognizes both left and right variants of modifier keys:
- `Left Ctrl` or `Right Ctrl` work for `ctrl`
- `Left Shift` or `Right Shift` work for `shift`
- `Left Alt` or `Right Alt` work for `alt`
- `Left Super` or `Right Super` work for `super`

### Multiple Keyboards

Nitrogen monitors all detected keyboard devices. If you have multiple keyboards:
- Hotkeys work from any keyboard
- Debug log shows which devices are detected

```bash
RUST_LOG=nitrogen_core::hotkeys=debug nitrogen cast
# Look for "Found keyboard device" messages
```

## How It Works

1. Nitrogen uses `evdev` to read raw keyboard events from `/dev/input/event*`
2. It maintains a set of currently pressed keys
3. When a key is pressed, it checks if any registered hotkey combination matches
4. If matched, the corresponding action is triggered via internal message channel

This approach:
- Works system-wide (not window-focused)
- Works with any Wayland compositor
- Has minimal latency
- Requires input group permissions

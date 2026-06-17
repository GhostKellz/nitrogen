# Troubleshooting Guide

## Quick Diagnostics

Run `nitrogen info` to check your system:

```bash
nitrogen info
```

This shows:
- GPU model and driver version
- Available NVENC encoders
- PipeWire/portal service status
- Wayland session info

---

## Common Issues

### "No NVENC encoders found"

**Symptoms:** `nitrogen info` shows no available encoders.

**Causes & Solutions:**

1. **Missing NVIDIA drivers**
   ```bash
   # Check if drivers are installed
   nvidia-smi

   # Arch Linux
   sudo pacman -S nvidia nvidia-utils

   # Fedora
   sudo dnf install nvidia-driver nvidia-driver-cuda

   # Ubuntu/Pop!_OS
   sudo apt install nvidia-driver-535  # or latest
   ```

2. **FFmpeg without NVENC support**
   ```bash
   # Check FFmpeg encoders
   ffmpeg -encoders | grep nvenc

   # Should show: h264_nvenc, hevc_nvenc, av1_nvenc

   # Arch: Install ffmpeg with NVENC
   sudo pacman -S ffmpeg

   # Ubuntu: May need non-free repo or compile
   sudo apt install ffmpeg
   ```

3. **GPU too old**
   - NVENC requires GTX 600 series or newer
   - AV1 encoding requires RTX 40 series

---

### "PipeWire not running"

**Symptoms:** `nitrogen info` shows PipeWire inactive.

**Solution:**
```bash
# Start PipeWire
systemctl --user start pipewire pipewire-pulse wireplumber

# Enable at login
systemctl --user enable pipewire pipewire-pulse wireplumber

# Check status
systemctl --user status pipewire
```

---

### "Portal error" / Screen picker doesn't appear

**Symptoms:** Running `nitrogen cast` shows portal error or nothing happens.

**Causes & Solutions:**

1. **xdg-desktop-portal not running**
   ```bash
   systemctl --user start xdg-desktop-portal
   systemctl --user enable xdg-desktop-portal
   ```

2. **Missing compositor-specific portal**
   ```bash
   # Hyprland
   sudo pacman -S xdg-desktop-portal-hyprland

   # Sway/wlroots
   sudo pacman -S xdg-desktop-portal-wlr

   # GNOME
   sudo pacman -S xdg-desktop-portal-gnome

   # KDE
   sudo pacman -S xdg-desktop-portal-kde
   ```

3. **Portal configuration conflict**
   ```bash
   # Check which portals are active
   cat ~/.config/xdg-desktop-portal/portals.conf

   # Example for Hyprland:
   # [preferred]
   # default=hyprland;gtk
   # org.freedesktop.impl.portal.Screencast=hyprland
   ```

---

### "No camera device" / Discord doesn't see camera

**Symptoms:** Virtual camera not appearing in Discord.

**Solutions:**

1. **v4l2loopback not loaded**
   ```bash
   # Load the module
   sudo modprobe v4l2loopback devices=1 video_nr=10 \
     card_label='Nitrogen Camera' exclusive_caps=1

   # Verify
   ls /dev/video*
   v4l2-ctl --list-devices
   ```

2. **Make persistent across reboots**
   ```bash
   # /etc/modules-load.d/v4l2loopback.conf
   v4l2loopback

   # /etc/modprobe.d/v4l2loopback.conf
   options v4l2loopback devices=1 video_nr=10 card_label='Nitrogen Camera' exclusive_caps=1
   ```

3. **Discord needs restart**
   - Close Discord completely (check system tray)
   - Start nitrogen first: `nitrogen cast`
   - Then open Discord and select "Nitrogen Camera"

---

### Low FPS / Dropped frames

**Symptoms:** `nitrogen status` shows frame drops or low actual FPS.

**Solutions:**

1. **Lower the preset**
   ```bash
   # Try 1080p instead of 1440p
   nitrogen cast -p 1080p60

   # Or lower framerate
   nitrogen cast -p 1440p30
   ```

2. **Use faster encoder preset**
   ```bash
   nitrogen cast --encoder-preset fast
   ```

3. **Check GPU load**
   ```bash
   nvidia-smi dmon -s u
   ```

4. **Close other GPU-intensive apps**
   - Games running in background
   - Other video encoding

---

### "Session already running"

**Symptoms:** Can't start new capture.

**Solution:**
```bash
# Stop existing session
nitrogen stop

# Force stop if needed
nitrogen stop --force

# Check status
nitrogen status
```

---

### Recording not working

**Symptoms:** `--record` flag doesn't create file.

**Solutions:**

1. **Check path permissions**
   ```bash
   # Ensure directory exists and is writable
   mkdir -p ~/Videos
   nitrogen cast --record ~/Videos/test.mp4
   ```

2. **Check encoder availability**
   ```bash
   nitrogen info | grep -A5 "Codec Capabilities"
   ```

3. **Try different codec**
   ```bash
   nitrogen cast --codec hevc --record output.mp4
   ```

---

### No audio in recording

**Symptoms:** Recording file has video but no audio.

**Causes & Solutions:**

1. **Audio source not specified**
   ```bash
   # Capture desktop audio
   nitrogen cast --audio desktop --record output.mp4

   # Capture microphone
   nitrogen cast --audio mic --record output.mp4

   # Capture both
   nitrogen cast --audio both --record output.mp4
   ```

2. **PipeWire audio not working**
   ```bash
   # Check PipeWire is handling audio
   pactl info | grep "Server Name"
   # Should show: PipeWire

   # List audio sinks
   pw-cli ls Node | grep -A2 "Audio/Sink"
   ```

3. **No audio playing during capture**
   - Desktop audio capture requires something to be playing
   - Try playing audio before starting capture

---

### Audio desync in recording

**Symptoms:** Audio and video drift apart over time.

**Solutions:**

1. **Use low-latency mode**
   ```bash
   nitrogen cast --low-latency --audio desktop --record output.mp4
   ```

2. **Lower framerate for stability**
   ```bash
   nitrogen cast -p 1080p30 --audio desktop --record output.mp4
   ```

3. **Check system load**
   ```bash
   # High CPU/GPU can cause timing issues
   top -bn1 | head -20
   nvidia-smi
   ```

---

### Wrong audio device captured

**Symptoms:** Recording captures wrong speaker or microphone.

**Solutions:**

1. **Check default devices**
   ```bash
   # List audio devices
   pw-cli ls Node | grep -E "(Audio/Sink|Audio/Source)"

   # Check defaults
   pactl get-default-sink
   pactl get-default-source
   ```

2. **Set the correct default**
   ```bash
   # Set default sink (speakers)
   pactl set-default-sink <sink-name>

   # Set default source (microphone)
   pactl set-default-source <source-name>
   ```

3. **Use pavucontrol for GUI**
   ```bash
   sudo pacman -S pavucontrol  # or apt install
   pavucontrol
   ```

---

### Hotkeys not working

**Symptoms:** Pressing Ctrl+Shift+F9 (or other hotkeys) does nothing.

**Causes & Solutions:**

1. **User not in input group**
   ```bash
   # Check group membership
   groups

   # Add user to input group
   sudo usermod -aG input $USER

   # Log out and back in for change to take effect
   ```

2. **No keyboard devices found**
   ```bash
   # Check if evdev devices are readable
   ls -la /dev/input/event*

   # Test keyboard events
   sudo evtest
   ```

3. **Conflicting compositor hotkeys**
   - Check if your compositor (Hyprland, Sway, etc.) intercepts the key combo
   - Try different hotkey in config:
   ```toml
   # ~/.config/nitrogen/config.toml
   [hotkeys]
   toggle = "ctrl+alt+f9"
   pause = "ctrl+alt+f10"
   record = "ctrl+alt+f11"
   ```

4. **Hotkeys disabled in config**
   ```toml
   # Check config for disabled hotkeys
   [hotkeys]
   enabled = true  # Must be true
   ```

---

### Hotkeys trigger wrong action

**Symptoms:** Pressing a hotkey does unexpected action.

**Solutions:**

1. **Check current bindings**
   ```bash
   # View active config
   cat ~/.config/nitrogen/config.toml | grep -A5 "\[hotkeys\]"
   ```

2. **Reset to defaults**
   ```toml
   # ~/.config/nitrogen/config.toml
   [hotkeys]
   toggle = "ctrl+shift+f9"
   pause = "ctrl+shift+f10"
   record = "ctrl+shift+f11"
   ```

3. **Check for held modifier keys**
   - Sticky keys or stuck modifiers can cause issues
   - Press and release all modifier keys (Ctrl, Shift, Alt)

---

## Debug Logging

Enable verbose logging for troubleshooting:

```bash
# Basic debug
RUST_LOG=debug nitrogen cast

# Very verbose
RUST_LOG=trace nitrogen cast

# Specific modules
RUST_LOG=nitrogen_core::capture=debug nitrogen cast

# Audio capture debugging
RUST_LOG=nitrogen_core::capture::audio=debug nitrogen cast

# Hotkey debugging
RUST_LOG=nitrogen_core::hotkeys=debug nitrogen cast

# Encoder debugging
RUST_LOG=nitrogen_core::encode=debug nitrogen cast

# Multiple modules
RUST_LOG=nitrogen_core::capture=debug,nitrogen_core::encode=debug nitrogen cast
```

---

## Getting Help

If issues persist:

1. Run `nitrogen info` and save output
2. Note your:
   - Linux distribution and version
   - Compositor (Hyprland, Sway, GNOME, etc.)
   - NVIDIA GPU model
   - Driver version (`nvidia-smi`)
3. Open an issue at https://github.com/ghostkellz/nitrogen/issues

Include the debug log output:
```bash
RUST_LOG=debug nitrogen cast 2>&1 | tee nitrogen-debug.log
```

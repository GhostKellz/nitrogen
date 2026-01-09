# Nitrogen for Pop!_OS

Pop!_OS is based on Ubuntu/Debian, so use the Debian packaging.

## Building the .deb

```bash
cd release/deb

# Install build dependencies
sudo apt install debhelper cargo rustc libpipewire-0.3-dev libclang-dev pkg-config

# Build the package
dpkg-buildpackage -us -uc -b
```

## Installation

```bash
# Install the built package
sudo dpkg -i ../nitrogen_0.1.0-1_amd64.deb

# Install dependencies
sudo apt install -f
```

## Manual Installation

```bash
# Build from source
cargo build --release

# Install binary
sudo install -Dm755 target/release/nitrogen /usr/bin/nitrogen

# Install systemd service
install -Dm644 examples/nitrogen.service ~/.config/systemd/user/nitrogen.service

# Load v4l2loopback
sudo modprobe v4l2loopback devices=1 video_nr=10 card_label='Nitrogen Camera' exclusive_caps=1
```

## Pop!_OS Specific Notes

Pop!_OS ships with the NVIDIA drivers pre-installed on the NVIDIA ISO, so you should have
NVENC support out of the box. Make sure you have:

```bash
# Verify NVIDIA driver
nvidia-smi

# Install PipeWire if not present
sudo apt install pipewire pipewire-audio-client-libraries

# Install v4l2loopback
sudo apt install v4l2loopback-dkms
```

## PPA (planned)

Once published to a PPA:

```bash
sudo add-apt-repository ppa:ghostkellz/nitrogen
sudo apt update
sudo apt install nitrogen
```

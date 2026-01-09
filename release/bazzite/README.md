# Nitrogen for Bazzite

Bazzite is based on Fedora, so use the Fedora RPM spec.

## Building

```bash
# Install build dependencies
sudo dnf install rust cargo clang pkg-config pipewire-devel

# Build the RPM
rpmbuild -ba ../fedora/nitrogen.spec
```

## Installation via COPR (planned)

Once published to COPR:

```bash
sudo dnf copr enable ghostkellz/nitrogen
sudo dnf install nitrogen
```

## Manual Installation

```bash
# Build from source
cargo build --release

# Install
sudo install -Dm755 target/release/nitrogen /usr/bin/nitrogen

# Load v4l2loopback
sudo modprobe v4l2loopback devices=1 video_nr=10 card_label='Nitrogen Camera' exclusive_caps=1
```

## Bazzite-Specific Notes

Bazzite uses rpm-ostree, so you may need to layer the package:

```bash
rpm-ostree install nitrogen
```

Or use the binary directly from cargo:

```bash
cargo install --git https://github.com/ghostkellz/nitrogen
```

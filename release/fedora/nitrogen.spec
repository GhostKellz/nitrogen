Name:           nitrogen
Version:        0.1.0
Release:        1%{?dist}
Summary:        Wayland-native NVIDIA streaming for Discord

License:        MIT OR Apache-2.0
URL:            https://github.com/ghostkellz/nitrogen
Source0:        %{url}/archive/v%{version}/%{name}-%{version}.tar.gz

BuildRequires:  rust >= 1.75
BuildRequires:  cargo
BuildRequires:  clang
BuildRequires:  pkg-config
BuildRequires:  pipewire-devel
BuildRequires:  systemd-rpm-macros

Requires:       pipewire
Requires:       xdg-desktop-portal
Requires:       ffmpeg
Requires:       v4l2loopback
Requires:       nvidia-driver

Recommends:     xdg-desktop-portal-gnome
Recommends:     xdg-desktop-portal-kde

%description
Nitrogen is a Wayland-native screen capture and streaming tool optimized for
NVIDIA GPUs. It uses hardware-accelerated NVENC encoding and presents as a
virtual camera for Discord and other applications.

Features:
- Hardware-accelerated H.264/HEVC/AV1 encoding via NVENC
- Low-latency screen capture via PipeWire and xdg-desktop-portal
- Virtual camera output (v4l2loopback)
- File recording support
- Configurable presets for 720p to 4K

%prep
%autosetup -n %{name}-%{version}

%build
export RUSTUP_TOOLCHAIN=stable
cargo build --release --locked

%install
install -Dm755 target/release/nitrogen %{buildroot}%{_bindir}/nitrogen
install -Dm644 LICENSE %{buildroot}%{_datadir}/licenses/%{name}/LICENSE
install -Dm644 examples/nitrogen.service %{buildroot}%{_userunitdir}/nitrogen.service

%post
%systemd_user_post nitrogen.service

%preun
%systemd_user_preun nitrogen.service

%postun
%systemd_user_postun_with_restart nitrogen.service

%files
%license LICENSE
%{_bindir}/nitrogen
%{_userunitdir}/nitrogen.service

%changelog
* Fri Dec 20 2024 Christopher Kelley <ckelley@ghostkellz.sh> - 0.1.0-1
- Initial package

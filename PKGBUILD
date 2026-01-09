# Maintainer: GhostKellz <ghost@ghostkellz.sh>
pkgname=nitrogen-screenshare
pkgver=1.0.0
pkgrel=1
pkgdesc="Discord Wayland Screenshare - NVENC-accelerated virtual camera"
arch=('x86_64')
url="https://github.com/ghostkellz/nitrogen"
license=('MIT')
depends=('glibc' 'pipewire' 'xdg-desktop-portal')
makedepends=('rust' 'cargo')
optdepends=(
    'nvidia-utils: NVENC hardware encoding'
    'discord: Discord integration'
    'xdg-desktop-portal-kde: KDE portal backend'
    'xdg-desktop-portal-wlr: wlroots portal backend'
    'xdg-desktop-portal-gnome: GNOME portal backend'
)
provides=('nitrogen')
conflicts=('nitrogen')
source=("$pkgname-$pkgver.tar.gz::$url/archive/v$pkgver.tar.gz")
sha256sums=('SKIP')

build() {
    cd "nitrogen-$pkgver"
    cargo build --release
}

package() {
    cd "nitrogen-$pkgver"

    # CLI binary
    install -Dm755 target/release/nitrogen "$pkgdir/usr/bin/nitrogen"

    # Systemd user service
    install -Dm644 /dev/stdin "$pkgdir/usr/lib/systemd/user/nitrogen.service" <<EOF
[Unit]
Description=Nitrogen Discord Screenshare Daemon
After=pipewire.service

[Service]
Type=simple
ExecStart=/usr/bin/nitrogen daemon
Restart=on-failure

[Install]
WantedBy=default.target
EOF

    # Documentation
    install -Dm644 README.md "$pkgdir/usr/share/doc/nitrogen/README.md"
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/nitrogen/LICENSE"
}

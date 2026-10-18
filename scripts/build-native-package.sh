#!/bin/bash
set -euo pipefail
if command -v dpkg >/dev/null; then
    rm -f dist/*.deb
    exec sh scripts/build-fat-deb.sh
fi
python3 scripts/verify-runtime.py packaging/runtime-root --all-php
cargo build --workspace --release --locked
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
mkdir -p "$stage/usr/bin" "$stage/usr/lib/localweb" "$stage/usr/lib/systemd/user" \
    "$stage/usr/share/applications" "$stage/usr/share/icons/hicolor/scalable/apps" \
    "$stage/usr/share/polkit-1/actions" "$stage/usr/share/doc/localweb" dist
for binary in localweb localwebd localweb-gateway localweb-helper localweb-gui localweb-mcp; do
    install -m 0755 "target/release/$binary" "$stage/usr/lib/localweb/$binary"
    ln -s "../lib/localweb/$binary" "$stage/usr/bin/$binary"
done
cp -a packaging/runtime-root "$stage/usr/lib/localweb/runtime"
install -m 0644 packaging/systemd/localwebd.service "$stage/usr/lib/systemd/user/"
install -m 0644 packaging/desktop/io.localweb.LocalWeb.desktop "$stage/usr/share/applications/"
install -m 0644 packaging/icons/io.localweb.LocalWeb.svg "$stage/usr/share/icons/hicolor/scalable/apps/"
install -m 0644 packaging/polkit/io.localweb.helper.policy "$stage/usr/share/polkit-1/actions/"
install -m 0644 README.md ARCHITECTURE.md SECURITY.md "$stage/usr/share/doc/localweb/"
rm -f dist/*.pkg.tar.zst
size=$(du -sk "$stage/usr" | cut -f1)
cat > "$stage/.PKGINFO" <<EOF
pkgname = localweb
pkgbase = localweb
pkgver = 0.1.0-1
pkgdesc = Native local web development environment
url = https://github.com/Ogalesha/LocalWEB
builddate = ${SOURCE_DATE_EPOCH:-0}
packager = Ogalesha <maintainers@localweb.invalid>
size = $((size * 1024))
arch = x86_64
license = MIT
depend = glibc
depend = gcc-libs
depend = systemd
depend = polkit
depend = ca-certificates
depend = libcap
depend = libx11
depend = libxcb
depend = libxkbcommon
depend = wayland
depend = mesa
depend = xdg-utils
EOF
cp packaging/arch/localweb.install "$stage/.INSTALL"
tar -C "$stage" --zstd -cf dist/localweb-0.1.0-1-x86_64.pkg.tar.zst .PKGINFO .INSTALL usr

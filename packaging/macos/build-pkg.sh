#!/bin/sh
# Builds the LocalWeb macOS installer (.pkg).
#
# Must run on macOS with Xcode command line tools installed:
#   sh packaging/macos/build-pkg.sh
#
# By default the script builds the workspace for the host architecture. To
# package prebuilt binaries (for example universal lipo output), point
# LOCALWEB_BIN_DIR at the directory that holds them:
#   LOCALWEB_BIN_DIR=target/universal/release sh packaging/macos/build-pkg.sh
#
# The resulting package installs the CLI, daemon, gateway, helper and MCP
# server into /usr/local/libexec/localweb plus a `localweb` symlink, and the
# GUI as /Applications/LocalWeb.app. Per-user setup (LaunchAgents, CA, hosts)
# still happens through `localweb setup`.

set -eu

[ "$(uname -s)" = Darwin ] || {
    echo "build-pkg.sh must run on macOS: launchd and pkgbuild are required" >&2
    exit 1
}

workspace=$(cd "$(dirname "$0")/../.." && pwd)
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$workspace/Cargo.toml" | head -1)
target_dir=${CARGO_TARGET_DIR:-$workspace/target}
bin_dir=${LOCALWEB_BIN_DIR:-$target_dir/release}

if [ ! -x "$bin_dir/localweb" ] || [ ! -x "$bin_dir/localweb-gui" ]; then
    cargo build --release --manifest-path "$workspace/Cargo.toml" --workspace
    bin_dir=$target_dir/release
fi

stage="$target_dir/macos-pkg-root"
rm -rf "$stage"
install -d \
    "$stage/usr/local/libexec/localweb" \
    "$stage/usr/local/bin" \
    "$stage/usr/local/share/doc/localweb" \
    "$stage/Applications/LocalWeb.app/Contents/MacOS" \
    "$stage/Applications/LocalWeb.app/Contents/Resources"

for binary in localweb localwebd localweb-gateway localweb-helper localweb-mcp; do
    install -m 0755 "$bin_dir/$binary" "$stage/usr/local/libexec/localweb/$binary"
done
ln -s ../libexec/localweb/localweb "$stage/usr/local/bin/localweb"
install -m 0755 "$workspace/packaging/macos/uninstall.sh" "$stage/usr/local/bin/localweb-uninstall"

app="$stage/Applications/LocalWeb.app"
install -m 0755 "$bin_dir/localweb-gui" "$app/Contents/MacOS/localweb-gui"
cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key><string>en</string>
    <key>CFBundleDisplayName</key><string>LocalWeb</string>
    <key>CFBundleExecutable</key><string>localweb-gui</string>
    <key>CFBundleIconFile</key><string>LocalWeb</string>
    <key>CFBundleIdentifier</key><string>io.localweb.LocalWeb</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>CFBundleName</key><string>LocalWeb</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF

if command -v rsvg-convert >/dev/null && command -v iconutil >/dev/null && command -v sips >/dev/null; then
    iconset=$(mktemp -d)/LocalWeb.iconset
    mkdir -p "$iconset"
    rsvg-convert -w 1024 -h 1024 \
        "$workspace/packaging/icons/io.localweb.LocalWeb.svg" \
        -o "$iconset/base.png"
    for size in 16 32 128 256 512; do
        double=$((size * 2))
        sips -z "$size" "$size" "$iconset/base.png" \
            --out "$iconset/icon_${size}x${size}.png" >/dev/null
        sips -z "$double" "$double" "$iconset/base.png" \
            --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
    done
    rm -f "$iconset/base.png"
    iconutil -c icns "$iconset" -o "$app/Contents/Resources/LocalWeb.icns"
    rm -rf "${iconset%/*}"
fi

for document in README.md ARCHITECTURE.md SECURITY.md; do
    if [ -f "$workspace/$document" ]; then
        install -m 0644 "$workspace/$document" "$stage/usr/local/share/doc/localweb/$document"
    fi
done
install -m 0644 "$workspace/docs/macos-porting.md" "$stage/usr/local/share/doc/localweb/macos-porting.md"

mkdir -p "$workspace/dist"
pkgbuild \
    --root "$stage" \
    --identifier dev.localweb.pkg \
    --version "$version" \
    --install-location / \
    "$workspace/dist/LocalWeb-$version.pkg"

pkgutil --payload-files "$workspace/dist/LocalWeb-$version.pkg" |
    grep -q "Applications/LocalWeb.app/Contents/MacOS/localweb-gui" || {
    echo "the package payload is missing the LocalWeb.app GUI" >&2
    exit 1
}

echo "Built $workspace/dist/LocalWeb-$version.pkg"
echo "Next: sudo installer -pkg \"$workspace/dist/LocalWeb-$version.pkg\" -target /"
echo "Then, as the desktop user: localweb setup && localweb ca install && localweb start"

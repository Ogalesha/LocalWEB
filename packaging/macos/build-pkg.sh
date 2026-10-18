#!/bin/sh
# Builds the LocalWeb macOS installer (.pkg).
#
# Must run on macOS with Xcode command line tools installed:
#   sh packaging/macos/build-pkg.sh
#
# The resulting package installs the CLI, daemon, gateway, helper and MCP
# server into /usr/local/libexec/localweb plus a `localweb` symlink. Per-user
# setup (LaunchAgents, CA, hosts) still happens through `localweb setup`.

set -eu

[ "$(uname -s)" = Darwin ] || {
    echo "build-pkg.sh must run on macOS: launchd and pkgbuild are required" >&2
    exit 1
}

workspace=$(cd "$(dirname "$0")/../.." && pwd)
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$workspace/Cargo.toml" | head -1)
target_dir=${CARGO_TARGET_DIR:-$workspace/target}

if [ ! -x "$target_dir/release/localweb" ]; then
    cargo build --release --manifest-path "$workspace/Cargo.toml" --workspace
fi

stage="$target_dir/macos-pkg-root"
rm -rf "$stage"
install -d \
    "$stage/usr/local/libexec/localweb" \
    "$stage/usr/local/bin" \
    "$stage/usr/local/share/doc/localweb"

for binary in localweb localwebd localweb-gateway localweb-helper localweb-mcp; do
    install -m 0755 "$target_dir/release/$binary" "$stage/usr/local/libexec/localweb/$binary"
done
ln -s ../libexec/localweb/localweb "$stage/usr/local/bin/localweb"
install -m 0755 "$workspace/packaging/macos/uninstall.sh" "$stage/usr/local/bin/localweb-uninstall"

for document in README.md ARCHITECTURE.md SECURITY.md; do
    if [ -f "$workspace/$document" ]; then
        install -m 0644 "$workspace/$document" "$stage/usr/local/share/doc/localweb/$document"
    fi
done
install -m 0644 "$workspace/docs/macos-porting.md" "$stage/usr/local/share/doc/localweb/macos-porting.md"

install -d "$target_dir"
pkgbuild \
    --root "$stage" \
    --identifier dev.localweb.pkg \
    --version "$version" \
    --install-location / \
    "$target_dir/LocalWeb-$version.pkg"

echo "Built $target_dir/LocalWeb-$version.pkg"
echo "Next: sudo installer -pkg \"$target_dir/LocalWeb-$version.pkg\" -target /"
echo "Then, as the desktop user: localweb setup && localweb ca install && localweb start"

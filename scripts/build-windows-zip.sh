#!/bin/bash
# Packages the cross-built Windows binaries into a portable ZIP:
#   dist/localweb-<version>-windows-x86_64.zip
set -euo pipefail

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[ -n "$version" ] || {
    echo "Could not determine the LocalWeb version from Cargo.toml" >&2
    exit 2
}

target=x86_64-pc-windows-msvc
bin_dir="${CARGO_TARGET_DIR:-target}/$target/release"
for binary in localweb localwebd localweb-gateway localweb-helper localweb-gui localweb-mcp; do
    [ -x "$bin_dir/$binary.exe" ] || {
        echo "missing binary: $bin_dir/$binary.exe" >&2
        exit 2
    }
done

mkdir -p dist
out="dist/localweb-$version-windows-x86_64.zip"
rm -f "$out"

python3 - "$bin_dir" "$out" "$version" <<'PY'
import sys
import zipfile
from pathlib import Path

bin_dir, out, version = sys.argv[1], sys.argv[2], sys.argv[3]
prefix = f"localweb-{version}"
binaries = ["localweb", "localwebd", "localweb-gateway", "localweb-helper", "localweb-gui", "localweb-mcp"]
extras = [
    (Path("README.md"), "README.md"),
    (Path("LICENSE"), "LICENSE"),
    (Path("assets/brand/localweb-icon.ico"), "localweb.ico"),
]

with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as archive:
    for binary in binaries:
        archive.write(Path(bin_dir) / f"{binary}.exe", f"{prefix}/{binary}.exe")
    for source, name in extras:
        if source.is_file():
            archive.write(source, f"{prefix}/{name}")
PY

test -s "$out"
echo "Packaged $out"

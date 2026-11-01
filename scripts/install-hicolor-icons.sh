#!/bin/sh
# Install the LocalWeb application icon at every standard hicolor size.
# Desktop shells resolve `io.localweb.LocalWeb` through the theme index, and
# 1024x1024 is not one of the indexed hicolor directories.
set -eu

if [ "$#" -ne 2 ]; then
    echo "usage: install-hicolor-icons.sh <brand-dir> <hicolor-dir>" >&2
    exit 2
fi

brand_dir=$1
hicolor_dir=$2
source_root="$brand_dir/hicolor"
[ -d "$source_root" ] || {
    echo "Missing hicolor icon sources: $source_root" >&2
    exit 2
}

installed=0
for icon in "$source_root"/*/apps/io.localweb.LocalWeb.png; do
    [ -f "$icon" ] || continue
    relative=${icon#"$source_root"/}
    install -D -m 0644 "$icon" "$hicolor_dir/$relative"
    installed=$((installed + 1))
done

[ "$installed" -gt 0 ] || {
    echo "No hicolor LocalWeb icons found under $source_root" >&2
    exit 2
}

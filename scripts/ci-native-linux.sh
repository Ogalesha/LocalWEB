#!/bin/bash
set -euo pipefail
export PATH="/usr/lib/postgresql/18/bin:/usr/sbin:/opt/cargo/bin:$PATH"
. scripts/ci-inputs.sh
sh scripts/build-php-runtimes.sh
sh scripts/stage-runtime.sh
export LD_LIBRARY_PATH="$PWD/packaging/runtime-root/lib"
python3 scripts/verify-runtime.py packaging/runtime-root --all-php
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
sh scripts/build-native-package.sh
if command -v dpkg >/dev/null; then
    if dpkg-query -W -f='${Status}' localweb 2>/dev/null | grep -q 'install ok installed'; then
        sudo dpkg -r localweb
    fi
    sudo dpkg -i dist/*.deb
else
    if pacman -Q localweb >/dev/null 2>&1; then
        sudo pacman -R --noconfirm localweb
    fi
    sudo pacman -U --noconfirm dist/*.pkg.tar.zst
fi
python3 scripts/acceptance.py /usr/lib/localweb
if command -v dpkg >/dev/null; then
    sudo dpkg -r localweb
else
    sudo pacman -R --noconfirm localweb
fi
test ! -e /usr/lib/localweb
test ! -e /var/lib/localweb
python3 - <<'PY'
from pathlib import Path
assert '# BEGIN LOCALWEB' not in Path('/etc/hosts').read_text()
PY
rm -f dist/SHA256SUMS
sha256sum dist/* > dist/SHA256SUMS

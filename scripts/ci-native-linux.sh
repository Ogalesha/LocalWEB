#!/bin/bash
set -euo pipefail
export PATH="/usr/lib/postgresql/18/bin:/usr/sbin:/opt/cargo/bin:$PATH"

run_step() {
    local label="$1"
    shift
    printf '\n===== BEGIN %s %s =====\n' "$label" "$(date -u +%FT%TZ)"
    if "$@"; then
        printf '===== PASS %s =====\n' "$label"
    else
        local rc=$?
        printf '===== FAIL %s exit=%d =====\n' "$label" "$rc"
        return "$rc"
    fi
}

install_package() {
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
}

remove_package() {
    if command -v dpkg >/dev/null; then
        sudo dpkg -r localweb
    else
        sudo pacman -R --noconfirm localweb
    fi
}

setup_preservation_sentinels() {
    marker="$HOME/.local/share/localweb/uninstall-preservation"
    mkdir -p "$HOME/.config/localweb" "$HOME/.local/state/localweb" "$(dirname "$marker")"
    printf 'keep\n' > "$marker"
    if ! id localweb_ci_second >/dev/null 2>&1; then
        sudo useradd --create-home localweb_ci_second
    fi
    second_home=$(getent passwd localweb_ci_second | cut -d: -f6)
    sudo mkdir -p \
        "$second_home/.config/localweb" \
        "$second_home/.local/share/localweb" \
        "$second_home/.local/state/localweb"
    printf 'keep\n' |
        sudo tee "$second_home/.local/share/localweb/uninstall-preservation" >/dev/null
}

assert_preserved_data() {
    test "$(cat "$HOME/.local/share/localweb/uninstall-preservation")" = keep
    test -d "$HOME/.config/localweb"
    second_home=$(getent passwd localweb_ci_second | cut -d: -f6)
    test "$(sudo cat "$second_home/.local/share/localweb/uninstall-preservation")" = keep
    sudo test -d "$second_home/.config/localweb"
    sudo test -d "$second_home/.local/state/localweb"
}

gui_smoke() {
    screenshot=/tmp/localweb-gui-smoke.png
    rm -f "$screenshot"
    cargo build -p localweb-gui --features screenshot
    timeout 90 xvfb-run -a env LIBGL_ALWAYS_SOFTWARE=1 \
        EFRAME_SCREENSHOT_TO="$screenshot" target/debug/localweb-gui
    test -s "$screenshot"
}

run_step ci-inputs source scripts/ci-inputs.sh
run_step build-php-runtimes sh scripts/build-php-runtimes.sh
run_step stage-runtime sh scripts/stage-runtime.sh
export LD_LIBRARY_PATH="$PWD/packaging/runtime-root/lib"
run_step verify-runtime python3 scripts/verify-runtime.py packaging/runtime-root --all-php
run_step cargo-fmt cargo fmt --all --check
run_step cargo-clippy cargo clippy --workspace --all-targets -- -D warnings
run_step cargo-test cargo test --workspace
run_step gui-smoke gui_smoke
run_step package sh scripts/build-native-package.sh
run_step install install_package
run_step acceptance python3 scripts/acceptance.py /usr/lib/localweb
run_step purge-dry-run /usr/lib/localweb/localweb data purge --dry-run
run_step prepare-preservation setup_preservation_sentinels
run_step remove remove_package
run_step assert-preservation assert_preserved_data

assert_cleanup() {
    test ! -e /usr/lib/localweb
    test ! -e /var/lib/localweb
    python3 - <<'PY'
from pathlib import Path
assert '# BEGIN LOCALWEB' not in Path('/etc/hosts').read_text()
PY
}

write_checksums() {
    rm -f dist/SHA256SUMS
    sha256sum dist/* > dist/SHA256SUMS
}

run_step cleanup assert_cleanup
run_step checksums write_checksums

#!/bin/bash
# Cross-builds the workspace for x86_64-pc-windows-msvc with cargo-xwin and
# exercises the resulting binaries under Wine: CLI help/version, the daemon's
# named pipe, the MCP status call over that pipe, and the GUI screenshot test.
set -euo pipefail

target=x86_64-pc-windows-msvc
exe_dir="target/$target/release"

for tool in cargo-xwin xvfb-run llvm-lib python3; do
    command -v "$tool" >/dev/null || {
        echo "missing required tool: $tool" >&2
        exit 2
    }
done

wine_bin="${WINE_BIN:-}"
if [ -z "$wine_bin" ]; then
    wine_bin=$(command -v wine64 || command -v wine || true)
fi
if [ -z "$wine_bin" ] && [ -x /usr/lib/wine/wine64 ]; then
    wine_bin=/usr/lib/wine/wine64
fi
if [ -z "$wine_bin" ]; then
    echo "missing required tool: wine64 (install wine64 or set WINE_BIN)" >&2
    exit 2
fi

cargo xwin build --release --target "$target" --workspace
cargo xwin build --release --target "$target" -p localweb-gui --features localweb-gui/screenshot

wineprefix=$(mktemp -d /tmp/localweb-wineprefix.XXXXXX)
runtime_dir=$(mktemp -d /tmp/localweb-wine-runtime.XXXXXX)
chmod 700 "$runtime_dir"
export WINEPREFIX="$wineprefix"
export WINEDEBUG=-all
export WINEDLLOVERRIDES="mscoree,mshtml="
export XDG_RUNTIME_DIR="$runtime_dir"
daemon_pid=""
cleanup() {
    [ -n "$daemon_pid" ] && kill "$daemon_pid" 2>/dev/null || true
    pkill -x wineserver 2>/dev/null || true
    pkill -x wineserver64 2>/dev/null || true
    for _ in $(seq 1 60); do
        pgrep -x wineserver >/dev/null 2>&1 || pgrep -x wineserver64 >/dev/null 2>&1 || break
        sleep 0.5
    done
    rm -rf "$wineprefix" "$runtime_dir"
}
trap cleanup EXIT

version=$(timeout 180 "$wine_bin" "$exe_dir/localweb.exe" --version)
printf '%s' "$version" | grep -Eq '^localweb [0-9]+\.[0-9]+\.[0-9]+' || {
    echo "unexpected --version output: $version" >&2
    exit 1
}

timeout 180 "$wine_bin" "$exe_dir/localweb.exe" --help | grep -q "Commands:" || {
    echo "localweb.exe --help did not list commands" >&2
    exit 1
}

daemon_log=/tmp/localweb-wine-daemon.log
rm -f "$daemon_log"
"$wine_bin" "$exe_dir/localwebd.exe" >"$daemon_log" 2>&1 &
daemon_pid=$!
for _ in $(seq 1 240); do
    grep -q "daemon listening" "$daemon_log" && break
    sleep 0.5
done
grep -q "daemon listening" "$daemon_log" || {
    cat "$daemon_log" >&2
    echo "localwebd.exe did not start listening" >&2
    exit 1
}

{
    printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"windows-smoke","version":"0"}}}'
    sleep 2
    printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}'
    sleep 1
    printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"localweb_status","arguments":{}}}'
    sleep 3
} | timeout 120 "$wine_bin" "$exe_dir/localweb-mcp.exe" 2>/dev/null >/tmp/localweb-wine-mcp.json

python3 - <<'PY'
import json
from pathlib import Path
replies = [json.loads(line) for line in Path("/tmp/localweb-wine-mcp.json").read_text().splitlines() if line.strip()]
init = next(reply for reply in replies if reply.get("id") == 1)
assert init["result"]["serverInfo"]["name"] == "localweb", init
status = next(reply for reply in replies if reply.get("id") == 2)
assert not status["result"]["isError"], status
PY

screenshot=/tmp/localweb-gui-windows-smoke.png
rm -f "$screenshot"
timeout 180 xvfb-run -a env LIBGL_ALWAYS_SOFTWARE=1 \
    EFRAME_SCREENSHOT_TO="Z:$screenshot" "$wine_bin" "$exe_dir/localweb-gui.exe"
test -s "$screenshot"

echo "Windows Wine smoke passed: CLI, daemon pipe, MCP status, GUI screenshot"

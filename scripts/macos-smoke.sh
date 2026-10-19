#!/bin/bash
# Exercises the macOS release binaries: CLI version/help, the daemon's unix
# socket and the MCP status call over it. Call with the directory that holds
# the built binaries, e.g.:
#   bash scripts/macos-smoke.sh target/universal/release
set -euo pipefail

bin_dir=${1:-target/release}
for binary in localweb localwebd localweb-mcp; do
    [ -x "$bin_dir/$binary" ] || {
        echo "missing binary: $bin_dir/$binary" >&2
        exit 2
    }
done
command -v python3 >/dev/null || {
    echo "missing required tool: python3" >&2
    exit 2
}

version=$("$bin_dir/localweb" --version)
printf '%s' "$version" | grep -Eq '^localweb [0-9]+\.[0-9]+\.[0-9]+' || {
    echo "unexpected --version output: $version" >&2
    exit 1
}
"$bin_dir/localweb" --help | grep -q "Commands:" || {
    echo "localweb --help did not list commands" >&2
    exit 1
}

runtime_dir=$(mktemp -d)
chmod 700 "$runtime_dir"
export XDG_RUNTIME_DIR="$runtime_dir"

daemon_log=$(mktemp)
mcp_out=$(mktemp)
daemon_pid=""
cleanup() {
    [ -n "$daemon_pid" ] && kill "$daemon_pid" 2>/dev/null || true
    rm -rf "$runtime_dir" "$daemon_log" "$mcp_out"
}
trap cleanup EXIT

"$bin_dir/localwebd" >"$daemon_log" 2>&1 &
daemon_pid=$!
for _ in $(seq 1 240); do
    grep -q "daemon listening" "$daemon_log" && break
    kill -0 "$daemon_pid" 2>/dev/null || break
    sleep 0.5
done
grep -q "daemon listening" "$daemon_log" || {
    cat "$daemon_log" >&2
    echo "localwebd did not start listening" >&2
    exit 1
}

{
    printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"macos-smoke","version":"0"}}}'
    sleep 2
    printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}'
    sleep 1
    printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"localweb_status","arguments":{}}}'
    sleep 3
} | "$bin_dir/localweb-mcp" 2>/dev/null >"$mcp_out"

python3 - "$mcp_out" <<'PY'
import json
import sys
from pathlib import Path
replies = [json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines() if line.strip()]
init = next(reply for reply in replies if reply.get("id") == 1)
assert init["result"]["serverInfo"]["name"] == "localweb", init
status = next(reply for reply in replies if reply.get("id") == 2)
assert not status["result"]["isError"], status
PY

echo "macOS smoke passed: CLI, daemon socket, MCP status"

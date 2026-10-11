#!/bin/sh
# Compatibility entry point; staging now detects its native build host.
set -eu
workspace=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
exec python3 "$workspace/scripts/stage-runtime.py"

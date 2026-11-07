#!/usr/bin/env python3
"""Expose bounded Rust diagnostics in checks when job logs need a sign-in."""

import json
from pathlib import Path
import sys

path = Path(sys.argv[1])
if not path.is_file():
    raise SystemExit(0)

def escape(text):
    return str(text).replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")

count = 0
for line in path.read_text(errors="replace").splitlines():
    try:
        item = json.loads(line)
    except json.JSONDecodeError:
        continue
    message = item.get("message", {})
    if item.get("reason") != "compiler-message" or message.get("level") != "error":
        continue
    text = message.get("rendered") or message.get("message", "Rust compilation failed")
    print(f"::error title=Native Rust diagnostic::{escape(text[:6000])}")
    count += 1
    if count == 25:
        break

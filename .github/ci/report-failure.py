#!/usr/bin/env python3
"""Expose bounded CI diagnostics in a check annotation, without publishing inputs."""
from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
text = path.read_text(errors="replace") if path.is_file() else "No build log was created; check builder startup."
lines = text.splitlines()

begin_markers = []
fail_markers = []
for index, line in enumerate(lines):
    match = re.match(r"^===== BEGIN (\S+) (.+) =====$", line)
    if match:
        begin_markers.append((index, match.group(1)))
    match = re.match(r"^===== FAIL (\S+) exit=(\d+) =====$", line)
    if match:
        fail_markers.append((index, match.group(1), match.group(2)))

selected = []
if fail_markers:
    fail_index, phase, _code = fail_markers[-1]
    begin_index = 0
    for index, label in begin_markers:
        if index < fail_index and label == phase:
            begin_index = index
    phase_lines = lines[begin_index : fail_index + 1]
    error_lines = [
        line
        for line in phase_lines
        if re.search(r"(^|[^A-Za-z])error:", line, re.I) and len(line) < 2000
    ]
    selected.extend(error_lines[:20])
    selected.extend(phase_lines[-30:])
else:
    errors = [
        line
        for line in lines
        if len(line) < 2000
        and re.search(
            r"error:|failed|traceback|not found|permission denied|undefined reference",
            line,
            re.I,
        )
    ]
    selected = errors[-15:] or lines[-25:]

summary = "\n".join(selected)[-12000:]
if fail_markers:
    failing = ", ".join(f"{phase} (exit {code})" for _, phase, code in fail_markers)
    summary = f"Failing steps: {failing}\n{summary}"
summary = summary.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
print(f"::error title=Native build diagnostics::{summary}")

#!/usr/bin/env python3
"""Expose bounded CI diagnostics in a check annotation, without publishing inputs."""
from pathlib import Path
import sys
import re

path = Path(sys.argv[1])
text = path.read_text(errors="replace") if path.is_file() else "No build log was created; check builder startup."
lines = text.splitlines()
errors = [line for line in lines if len(line) < 2000 and re.search(r"error:|failed|traceback|not found|exception|permission denied|undefined reference", line, re.I)]
summary = "\n".join(errors[-15:] or lines[-25:])[-8000:]
summary = summary.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
print(f"::error title=Native build diagnostics::{summary}")

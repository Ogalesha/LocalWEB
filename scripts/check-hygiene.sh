#!/bin/bash
# Fails when tracked files contain personal email addresses or credentials.
# Runs in CI before release publishing; also useful locally.
set -euo pipefail

patterns=(
    '[A-Za-z0-9._%+-]+@(gmail|googlemail|outlook|hotmail|live|yahoo|icloud|me|proton|protonmail|gmx|web)\.[A-Za-z]{2,}'
    '-----BEGIN [A-Z ]*PRIVATE KEY-----'
    'gh[pousr]_[A-Za-z0-9]{20,}'
    'AKIA[0-9A-Z]{16}'
    'sk-[A-Za-z0-9]{20,}'
)

status=0
for pattern in "${patterns[@]}"; do
    if matches=$(git grep -n -I -E "$pattern" -- . 2>/dev/null); then
        echo "suspicious content matching '$pattern':" >&2
        printf '%s\n' "$matches" >&2
        status=1
    fi
done

if [ "$status" -eq 0 ]; then
    echo "Repository hygiene check passed"
fi
exit "$status"

#!/bin/bash
# Verify secret detection in an isolated repository, including leading '-'.
set -euo pipefail
workspace=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
fixture=$(mktemp -d /tmp/localweb-hygiene.XXXXXX)
trap 'rm -rf -- "$fixture"' EXIT
cd "$fixture"
git -c init.defaultBranch=main init -q
printf '%s%s\n' '-----BEGIN ' 'PRIVATE KEY-----' > sample
git add sample
if bash "$workspace/scripts/check-hygiene.sh" > result 2>&1; then
    echo "The hygiene check missed a private-key header" >&2
    exit 1
fi
printf 'ordinary text\n' > sample
git add sample
bash "$workspace/scripts/check-hygiene.sh"
echo "Hygiene regression test passed"

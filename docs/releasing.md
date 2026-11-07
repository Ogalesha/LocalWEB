# Release checks

Before publishing, inspect every validation run for the exact source commit.
The native build must pass Ubuntu and Arch acceptance, macOS quality/tests,
Windows native quality/tests, Wine smoke and dependency/hygiene checks.
The separate macOS smoke workflow must also pass for that commit.

`scripts/publish-downloads.py` enforces these checks for rolling and tagged
releases. A run from a fork, another branch or another commit cannot satisfy
them. A pending check delays publication; a failed, cancelled or superseded
run blocks it. Do not override failed checks or publish an unverified artifact.

After publication, verify the four public download URLs, their checksums,
the release's source commit and the rolling tag. Confirm that the homepage
uses the permanent OS-specific asset names. Versioned stable releases remain
historical snapshots; homepage links use the latest verified rolling build.

# Development

Use stable Rust (minimum version is declared in the workspace).

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release --locked
```

The desktop is native egui/eframe. Tests cover form opt-outs, detection,
worker presets, connection failure and minimum-window actions in all 14
languages. Visual and keyboard review complements automated tests.

## Native build pipeline

`.github/workflows/native-builds.yml` assembles Ubuntu 26.04 and Arch packages
in disposable distro-specific CI environments. Arch covers Omarchy's base;
an actual Omarchy desktop acceptance run remains a separate check.

Docker is confined to `.github/ci/` for GitHub's temporary Linux builders.
It is not shipped, required by the installed app, or used for development
projects.

CI compiles six pinned PHP CLI/FPM pairs, verifies downloads and the
payload, creates and installs a package, then tests 48 PHP/web/database
combinations plus HTTPS, Redis, Mailpit, CLI and removal. Artifacts
upload only after that platform passes. Run privileged acceptance only
on disposable hosts because it modifies test system integration.

## Runtime staging

Use Python 3 on the builder with complete native inputs. `stage-runtime.sh`
detects the OS; `stage-ubuntu-runtime.sh` is a compatibility wrapper.
`build-php-runtimes.sh` compiles the pinned CLI/FPM pairs. PHP 8.6 is
excluded until it has a stable release.

```bash
export COMPOSER_PHAR=/path/to/composer.phar
export COMPOSER_SHA256=expected-full-sha256
export MAILPIT_BIN=/path/to/mailpit
export MAILPIT_SHA256=expected-full-sha256
export LOCALWEB_PHP_ROOT=/path/to/private-php-builds
export LOCALWEB_PHP_VERSIONS="8.0 8.1 8.2 8.3 8.4 8.5"
sh scripts/stage-runtime.sh
python3 scripts/verify-runtime.py packaging/runtime-root --all-php
sh scripts/build-native-package.sh
```

Smaller development builds may stage only PHP 8.5; the GUI labels actual
availability. Binary/share/module overrides and native examples are in
the staging script and workflow. MySQL and MariaDB inputs must be distinct.

Staging rejects unresolved dependencies. Payload checks verify hashes,
PHP capabilities and npm/npx. The package-layout test uses fixtures and
does not exercise real runtimes. Generated inputs, payloads and packages
are ignored by Git; packaging copies only public documentation.

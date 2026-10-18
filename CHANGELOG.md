# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- PHP 8.0–8.5 selection, actual bundled-availability labels and editable app presets; 8.6 is excluded until stable.
- PostgreSQL cluster, per-project provisioning and connection settings.
- Native trust stores and systemd integration for Ubuntu, Arch and Omarchy.
- Native build/install/runtime/removal gates before artifact upload.
- GUI project details, connection-settings reveal/copy and domain-sync retry.
- `localweb-mcp`: Model Context Protocol server (stdio) with 19 tools covering status, daemon start/stop, project add/remove/start/stop/restart, environment, `@php`/`@composer`/`@node`/`@npm`/`@npx` execution, runtime list, CA status/install, hosts sync, login autostart and logs, so an AI assistant can set up a site and run its tests end to end.
- macOS port (implemented, pending hardware validation): `TrustStore::Macos` installs, verifies and removes the CA through `security`; one privilege launcher runs the helper via `pkexec` or `osascript` with a 0600 temp-file payload; the daemon service layer now has a platform backend (systemd units on Linux, supervised child processes on macOS); launch agents written by `localweb setup`; Homebrew runtime detection for PHP, Node, nginx, Apache, MariaDB, MySQL, PostgreSQL, Redis, Mailpit and Composer; `Paths::discover` falls back to `$TMPDIR`; `.github/workflows/macos-smoke.yml` compiles the workspace and runs the portable tests on `macos-latest`.
- macOS gateway runs as a per-user LaunchAgent with launchd socket activation for 127.0.0.1:443 (`launch_activate_socket`), so no root process or file capability is needed; the daemon bootstraps, kickstarts and boots it out with `launchctl`.
- Packaging for macOS: `packaging/macos/build-pkg.sh` builds `LocalWeb-<version>.pkg` with `pkgbuild`, and `packaging/macos/uninstall.sh` removes launch agents, the CA trust copy, `/usr/local/bin` entries and the `/private/var/folders` runtime while preserving per-user data and project sources.
- Desktop UI in 14 languages (English, Italiano, Shqip, Chinese, French, Greek, Hindi, Polish, Portuguese, Russian, Spanish, Swedish, Turkish, Ukrainian) with bundled Noto CJK SC and Devanagari fonts and native punctuation.
- Settings view with the resolved MCP server path, copyable client configuration, read-only environment summary and relocated maintenance actions, translated into all 14 languages.
- Per-project fixed direct HTTP ports (`18000-18999`, `--web-port`) and database proxy ports (`19000-19999`, `--db-port`) for MariaDB, MySQL and PostgreSQL; existing projects are migrated on the next daemon start and the environment exposes the proxy as `DB_PORT`.

### Changed

- Uninstall is now non-destructive: the Debian, Arch and macOS hooks stop services and remove system integration but preserve per-user projects, databases, configurations and private CA keys. `localweb data purge` lists sizes, supports `--dry-run` and requires explicit confirmation.
- `localweb ca remove` revokes this account's CA from the system trust store; installing a second, different CA is refused instead of silently overwriting another account's trust.
- Project health checks now require a bounded, CRLF-terminated HTTP/1.0 or HTTP/1.1 status line and reject 404/5xx responses; 401/403 remain accepted for authenticated roots.
- PHP source downloads retry with backoff for up to eight attempts, so transient runner DNS failures no longer abort the native build.
- Native CI pins Rust 1.99.1 and the MCP tests no longer trigger Clippy's `assert_is_empty`, which caused the exit-101 failure in run 39146985057 after the PHP builds.
- MariaDB and MySQL render their runtime socket into `my.cnf` and the daemon waits for the socket before provisioning projects, so acceptance no longer fails with `ERROR 2002` when the service runs in a private temporary namespace.
- The daemon logs under its real binary name (`localwebd=info`, overridable with `RUST_LOG`) and keeps serving when a transient accept error occurs, instead of exiting and dropping client connections.
- `localweb data purge` refuses directories that are not owned by the current user and removes user directories without following symlinks.
- Daemon state changes are transactional: web/database proxy restarts keep the previous proxy until the new configuration passes health checks, `persist_running` failures trigger rollback, stop persists only after all project-owned services stopped, and shutdown preserves the desired state when it cannot stop everything.
- CI diagnostics now emit timestamped phase markers, per-version PHP configure/make/install logs, and upload a sanitized diagnostics artifact when a native job fails.
- Release publishing runs only for `v*` tags whose commit matches the built SHA; asset uploads are no longer clobbered.

- GUI visual system: bundled Inter (SIL OFL) with medium/semibold families, design tokens in `theme.rs`, reusable widgets in `widgets.rs` and an authored icon set replacing Unicode marks; the flat, square language uses 0 px radii, no shadows, hairline borders and square status marks.
- Every view reworked around the new system: environment banner with width-aware metrics, project ledger with aligned columns that stacks below 940 px, pill-based actions, and forms with consistent labels, radii and focus states.
- Feedback moved from an inline strip to a floating toast (success auto-dismisses, errors stay until dismissed); view switches fade in over 160 ms.
- The rail now shows the application icon itself (deep green tile, sage L, light dot); one geometry draws it in the interface, the window icon and the packaged SVG.
- Every primary action remains reachable at the 840×560 minimum window in all 14 languages.
- The HTTPS gateway is now optional: when another stack owns port 443, LocalWeb logs the conflict and keeps serving every project on its direct HTTP port instead of failing to start; hosts-file gaps are a warning rather than a startup error.
- Interactive targets now meet the 36 px minimum: form checkboxes became full-width rows with hover, pressed and keyboard-focus states; picker navigation, search, table actions and toast dismissal were enlarged.
- Language switcher replaced by a rail select showing the two-letter English initials (AL, EN, ES, FR, GR, …) of all 14 languages, ordered alphabetically by initial; each language displays its own content in its own language, and technical terms (Daemon, Gateway, Worker, Shell) stay in English across translations.
- Palette rebuilt around a sage accent (`#9DBA81`) on a deep forest rail; nearly-black green ink, darker metadata, stronger hairlines and dedicated hover planes keep every label readable (body text ≥13:1, small metadata ≥7:1), while success, warning and danger keep their own meaning.
- Usability pass on the dashboard: one environment banner carries the state and its single contextual action, including certificate installation when needed; the headline never says "ready" while trust is pending, and duplicated counters, service chips and the empty workers tile are gone.
- "Stop LocalWeb" asks for confirmation only when projects are running; maintenance actions (refresh, sync local domains, repair setup) moved from the dashboard to the Settings view.
- Add-project validation anchors the folder and worker-executable messages to their fields; the submit label matches "Add project" everywhere.
- Trimmed copy and chrome: page subtitles, the rail tagline, the rail footer dividers and static runtime notes are gone; project search and its summary appear only when the list needs them. Small metadata moved from 11.5 to 12 px.
- Folder picker rebuilt as a classic browser: places sidebar (Home, Desktop, Documents, Downloads, root), back/forward/up navigation with history, breadcrumb path that collapses to Home, and a compact folder list with single-click selection and double-click navigation.
- Hardened internals: canonical error codes, the full recommended Clippy lint gate, documented service constants, and blocking service work moved off the async reactor.
- CLI and MCP now share `localweb_core::service` (daemon lifecycle and login autostart) and `localweb_core::privilege` (helper escalation) instead of calling `systemctl`, `pkexec` or `journalctl` directly, so the same commands work on both platforms.

### Fixed

- Removal sweeps any `localweb*.service` unit and wants symlink registered in a user's systemd directory, covering source-build unit files in addition to the managed names; verified end to end with a real package removal (`dpkg -r`): services, hosts block, CA, user state and runtime directories are gone and the project source folder is untouched.
- Cron opt-out omits hidden tasks; Symfony's worker preset matches the CLI.
- Workers and `@php` schedules use private PHP configuration.
- Background refresh no longer disables actions; unavailable/stopped states differ.
- Projects no longer show an indefinite loading label while the daemon is offline; a failed load is surfaced once the daemon answers.
- Selected rail destinations keep a visible focus ring, popup-menu hover uses a distinct plane, and the open language selector keeps dark, readable text.
- Compact-window actions, search no-results and modal keyboard handling.
- Starting a project no longer fails with `ServiceStartFailed` when another local development stack already listens on port 443.
- The GUI browser action uses `open` on macOS instead of `xdg-open`, which does not exist there.

## [0.1.0] - 2026-10-11

### Added

- Initial Ubuntu 26.04 amd64 reference `.deb`: nginx (stable),
  Apache 2.4, PHP 8.5 with the extensions common apps need pre-activated (every
  packaged extension still shipped and hand-enableable), Composer 2,
  MariaDB 11.8 LTS, MySQL 8.4 LTS, Redis 8, SQLite via `pdo_sqlite`,
  Node.js 24 LTS with npm and npx; Node.js 22 and 20 remain selectable from
  system installations.
- Per-project HTTPS domains (`https://<name>.dev` by default) with a local CA and
  exact `/etc/hosts` management (privileged steps always ask for
  authentication via polkit).
- Project workers (Supervisor) with Laravel and Symfony presets, plus a
  per-project cron scheduler with framework presets (Laravel, WordPress,
  Drupal) and overlap protection.
- Per-project email testing via Mailpit (SMTP + web UI).
- Native egui GUI in English, Italiano and Shqip and a CLI: projects, `php`, `composer`,
  `node`, `npm`, `exec`, `shell`, `env`, `doctor`, `runtime list`.
- Versioned daemon protocol, atomic configuration rendering, systemd user
  units with hardening and a provenance manifest. `runtime list` checks paths.
- Complete uninstall cleanup: services, certificates, hostnames, owned
  data, daemon reload, and desktop/icon cache refresh. Project source
  directories are never touched.
- Real maintainer identity in the Debian control file.
- Mailpit web UI opens straight from the project ledger.

[Unreleased]: https://github.com/Ogalesha/LocalWEB/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Ogalesha/LocalWEB/releases/tag/v0.1.0

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

### Fixed

- Cron opt-out omits hidden tasks; Symfony's worker preset matches the CLI.
- Workers and `@php` schedules use private PHP configuration.
- Background refresh no longer disables actions; unavailable/stopped states differ.
- Compact-window actions, search no-results and modal keyboard handling.

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

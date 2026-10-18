# LocalWeb architecture

LocalWeb uses native Rust processes on Ubuntu, Arch Linux, Omarchy and macOS. On Linux, managed services use transient systemd user units with a private bundled runtime; on macOS they are daemon-supervised child processes that use Homebrew-installed runtimes, and the gateway takes its port from launchd socket activation. See `docs/macos-porting.md` for the port status.

```text
 CLI ───────┐
            ├── Unix socket (0600) ── localwebd ── systemd --user
 egui GUI ──┘                              │
 Browser ── HTTP 127.0.0.1:18000-18999 ────┤
                                           ├── localweb-gateway :443 (HTTPS when free)
 Browser ── HTTPS/SNI ─────────────────────┤        │
                                           │        ├── nginx 127.0.0.1:<project port>
                                           │        └── Apache 127.0.0.1:<project port>
                                           │                    │
                                           ├── PHP-FPM 8.x ── project Unix sockets
                                           ├── MariaDB/MySQL/PostgreSQL loopback
                                           ├── per-project DB proxies 127.0.0.1:19000-19999
                                           ├── per-project SQLite files
                                           ├── per-project Redis/Mailpit loopback
                                           └── per-project supervised workers
```

The daemon owns configuration and the desired active-project set, not continuous health monitoring of every child service. Gateway state is checked through the native service manager (systemd on Linux, `launchctl` on macOS). Clients share a user-only Unix-socket protocol. Closing the GUI disconnects only that client; **Stop LocalWeb** sends the same request as `localweb shutdown`. Setup, CA installation and host synchronization invoke the CLI/helper. On Linux only the gateway receives `CAP_NET_BIND_SERVICE`; on macOS launchd creates the 127.0.0.1:443 socket for the per-user gateway LaunchAgent, so no root process and no capability are needed.

## Process ownership

nginx, Apache, PHP-FPM and database servers are shared. Redis, Mailpit and workers are per-project processes. They run as the desktop user through the native service backend (`crates/localweb-daemon/src/backend.rs`): transient systemd units on Linux, tracked child processes with pidfiles and log files on macOS. Workers restart after exit and stop with the project. PHP, Composer, FPM, workers and `@php` schedules use the selected private PHP configuration when present.

```text
/usr/lib/localweb/
  localwebd, localweb, localweb-gateway, localweb-helper, localweb-gui, localweb-mcp
  runtime/
    nginx/
    apache/
    php/8.5/
    composer/2/
    node/
    mariadb/
    mysql/
    postgres/
    redis/
    mailpit/

~/.config/localweb/       human-readable configuration
~/.local/share/localweb/  CA, certificates, owned persistent data
~/.local/state/localweb/  logs and operational state
$XDG_RUNTIME_DIR/localweb sockets, PIDs, generated runtime state
```

`localweb-mcp` is a Model Context Protocol server on stdio. It translates tool calls into the same daemon requests, privileged helper runs (`ca install`, `hosts sync`) and project command executions the GUI and CLI use, so an AI assistant drives the identical stack.

Inside `runtime/mariadb`, the directory layout mirrors a `/usr` basedir (`bin/`, `sbin/mariadbd`, `share/mariadb/`, `lib/plugin`) so the stock MariaDB tools work unmodified against `--basedir`.

## Startup transaction

Startup validates the selected runtime, generates the leaf certificate, checks hosts, provisions MariaDB/MySQL/PostgreSQL/SQLite, starts requested Redis/Mailpit, renders PHP-FPM and the selected web server, updates routes, starts the worker and checks HTTP startup. Failure attempts to restore the previous active configuration and stop new services.

Stop applies the remaining active configuration, then stops project services and unused shared databases. Global shutdown attempts every owned service. Cron runs at most once per matching minute with overlap protection; an already-running cron command currently finishes independently of project stop.

## Packaging

Native staging records the target OS and architecture and bundles private shared libraries. CI verifies, installs, exercises and removes each native package before uploading. See [runtime management](docs/runtime-management.md). macOS support is implemented but still unvalidated on hardware; the implementation map, remaining risks and removal-parity checklist live in [macOS porting](docs/macos-porting.md), with the package builder in `packaging/macos/build-pkg.sh` and the uninstaller in `packaging/macos/uninstall.sh`.

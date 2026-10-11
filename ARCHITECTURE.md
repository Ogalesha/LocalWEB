# LocalWeb architecture

LocalWeb uses native Rust processes and private native runtimes on Ubuntu, Arch Linux and Omarchy. Managed services use systemd user units.

```text
 CLI ───────┐
            ├── Unix socket (0600) ── localwebd ── systemd --user
 egui GUI ──┘                              │
                                           ├── localweb-gateway :443 (HTTPS only)
 Browser ── HTTPS/SNI ─────────────────────┤        │
                                           │        ├── nginx 127.0.0.1:18080
                                           │        └── Apache 127.0.0.1:18081
                                           │                    │
                                           ├── PHP-FPM 8.x ── project Unix sockets
                                           ├── MariaDB/MySQL/PostgreSQL loopback
                                           ├── per-project SQLite files
                                           ├── per-project Redis/Mailpit loopback
                                           └── per-project supervised workers
```

The daemon owns configuration and the desired active-project set, not continuous health monitoring of every child service. Gateway state is checked through the native service manager. Clients share a user-only Unix-socket protocol. Closing the GUI disconnects only that client; **Stop LocalWeb** sends the same request as `localweb shutdown`. Setup, CA installation and host synchronization invoke the CLI/helper. On Linux only the gateway receives `CAP_NET_BIND_SERVICE`.

## Process ownership

nginx, Apache, PHP-FPM and database servers are shared. Redis, Mailpit and workers are per-project processes. They run as the desktop user through the native service manager. Workers restart after exit and stop with the project. PHP, Composer, FPM, workers and `@php` schedules use the selected private PHP configuration when present.

```text
/usr/lib/localweb/
  localwebd, localweb, localweb-gateway, localweb-helper, localweb-gui
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

Inside `runtime/mariadb`, the directory layout mirrors a `/usr` basedir (`bin/`, `sbin/mariadbd`, `share/mariadb/`, `lib/plugin`) so the stock MariaDB tools work unmodified against `--basedir`.

## Startup transaction

Startup validates the selected runtime, generates the leaf certificate, checks hosts, provisions MariaDB/MySQL/PostgreSQL/SQLite, starts requested Redis/Mailpit, renders PHP-FPM and the selected web server, updates routes, starts the worker and checks HTTP startup. Failure attempts to restore the previous active configuration and stop new services.

Stop applies the remaining active configuration, then stops project services and unused shared databases. Global shutdown attempts every owned service. Cron runs at most once per matching minute with overlap protection; an already-running cron command currently finishes independently of project stop.

## Packaging

Native staging records the target OS and architecture and bundles private shared libraries. CI verifies, installs, exercises and removes each native package before uploading. See [runtime management](docs/runtime-management.md).

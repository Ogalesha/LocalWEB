# LocalWeb

A native local development environment for web developers. Choose a source
folder, a PHP version, a web server and the services your app needs. Run
projects side by side on fixed local HTTP ports, with optional HTTPS
domains, without Docker or virtual machines.

Use WordPress, Laravel, Symfony, Drupal, another PHP framework or CMS, or
your own application. Presets are shortcuts, not a list of supported apps.
The **Generic PHP** preset lets you choose the document root, database,
cache, worker command and scheduled tasks yourself. Your application's
requirements still determine which PHP and database versions it can use.

## Tools and features

| Tool | What LocalWeb provides |
| --- | --- |
| **PHP 8.0–8.5** | Per-project CLI and PHP-FPM selection. PHP 8.0 and 8.1 are end-of-life compatibility versions and receive no upstream security fixes; 8.2 is in security-only support until 31 Dec 2026. Use a supported branch for current work. The desktop indicates which versions are included. PHP 8.6 will be added after its stable release. |
| **nginx** | Per-project web configuration for a LEMP stack. |
| **Apache 2.4** | Per-project web configuration for a LAMP stack; can run alongside nginx. |
| **MariaDB** | Shared local server with a database and credentials for each project; the reference payload uses MariaDB 11.8 LTS. |
| **MySQL** | Shared local server with a database and credentials for each project; the reference payload uses MySQL 8.4 LTS. |
| **PostgreSQL** | Private local cluster with a database and login for each project. |
| **SQLite** | A LocalWeb-owned database file for each project, accessed through PHP. |
| **Redis** | A separate local cache process for each project. |
| **Composer 2** | Runs with the project's selected PHP. |
| **Node.js 24, npm and npx** | Bundled LTS build tools. Node.js 22 and 20 can also be selected when matching system installations are available; Node.js 20 is EOL. |
| **Mailpit** | Per-project SMTP server and browser inbox for development email. |
| **Background workers** | Supervisor-style process management using the native service manager; Laravel queue and Symfony Messenger presets, or a custom executable and arguments. |
| **Scheduled tasks** | Five-field cron schedules, framework presets, logs and overlap protection while the project is active. |
| **Local domains** | Exact project entries in `/etc/hosts`, with `.dev` as the default suffix. |
| **Direct ports and HTTPS domains** | Every project gets a fixed loopback HTTP port (18000–18999) and, for network databases, a fixed loopback database port (19000–19999) forwarded to the shared engine. On Linux the TLS gateway owns a dedicated loopback identity such as `127.0.0.2:443`, so other local stacks can keep `127.0.0.1:443`. |
| **Desktop app** | Project search, runtime/service selection, start/stop/restart, browser and Mailpit opening, details and connection settings. |
| **CLI** | Project management, PHP/Composer/Node commands, shell access, environment values and diagnostics. |
| **AI assistants (MCP)** | A Model Context Protocol server (`localweb-mcp`) that lets AI assistants create, start, inspect and test projects through the same daemon as the desktop app. |
| **Autostart** | Optional login startup and per-project startup preferences. |

The native build pipeline assembles all six PHP CLI/FPM pairs from the
pinned sources in `packaging/php-sources.txt`. A smaller development package
may include only PHP 8.5. Selecting another version does not download it:
its matching CLI and FPM must already be in the runtime or on the system.
PostgreSQL likewise needs its complete runtime payload and PHP's
`pdo_pgsql` extension. **Memcached is not implemented.**

See [runtime management](docs/runtime-management.md) for extension,
version and runtime-location details.

## Platforms and distribution

LocalWeb's source, changes and downloadable packages are published **only
on [GitHub](https://github.com/Ogalesha/LocalWEB)**. There is no LocalWeb
package in a distro repository, AUR or COPR. OS packages and upstream
downloads are build inputs, not alternative LocalWeb distribution channels.

| Platform | Package | Native integration |
| --- | --- | --- |
| Ubuntu 26.04 amd64 | `.deb` | systemd, system certificate store and system authorization |
| Arch Linux amd64 | `.pkg.tar.zst` | systemd, Arch certificate store and system authorization |
| Omarchy amd64 | Arch package | Uses the same Arch base and integration |
| macOS 12+ arm64/x86_64 | `.pkg` (universal) | launchd agents and system keychain trust for the development CA |
| Windows 10/11 x86_64 | `.zip` (portable) | Named pipes, scheduled task and user certificate store |

The package carries LocalWeb's private web stack. It does not replace the
machine's PHP, web servers, databases or caches. A normal desktop session
with the platform service manager (systemd, launchd or Task Scheduler) is
required.

### Validation status

The earlier Ubuntu/PHP 8.5 package has passed live stack and removal tests.
The expanded stack and platform ports are checked by
[Native packages and acceptance](https://github.com/Ogalesha/LocalWEB/actions/workflows/native-builds.yml).
For the source revision you are installing, check that its platform job
passed. Arch CI covers the base used by Omarchy; an Omarchy desktop session
still needs its own final acceptance check. macOS jobs build and smoke the
universal binaries on a macOS runner, and the Windows binaries are
cross-built and exercised under Wine; a final acceptance run on real macOS
and Windows hosts is still pending.

Each job builds and installs the package, tests PHP 8.0–8.5 with both web
servers and all four database choices, checks Redis,
Mailpit and the PHP CLI, then exercises stop, removal and source-folder
preservation. Packages are uploaded as workflow artifacts only after the
job passes. Tagged versions and the rolling pre-release publish the same
files as public release assets.

## Install and get started

Download the package for your OS from [GitHub Releases](https://github.com/Ogalesha/LocalWEB/releases).
Version tags are stable releases; the [LocalWeb rolling builds](https://github.com/Ogalesha/LocalWEB/releases/tag/rolling)
pre-release is rebuilt from every successful push to `main`. Release assets
download without an account, while the artifacts attached to individual
**Actions** runs require a GitHub sign-in. Check the package's SHA-256 against the accompanying
`SHA256SUMS` file.

```bash
sudo apt install ./localweb_0.1.3_amd64.deb          # Ubuntu
sudo pacman -U ./localweb-0.1.3-1-x86_64.pkg.tar.zst # Arch / Omarchy
```

Open the desktop app and choose **Set up LocalWeb**, then **Install CA**.
Or use the same workflow from a terminal:

```bash
localweb setup
localweb ca install
localweb start
```

System authorization is used for CA installation, cleanup registration and
local-domain changes. Project services run as your desktop user.

## Add your first project

In the desktop app, open **Add project**, select an existing source folder,
check its preset and document root, choose the runtime and services, then
confirm with **Add project**. The source files are kept where they are.
Select **Start** to run the project and **Open** to visit it.

For a terminal workflow:

```bash
localweb project add ~/Code/acme --php 8.5 --web nginx \
  --db mariadb --cache redis --document-root public --domain acme.dev
localweb project start acme
localweb php acme -- -v
localweb composer acme -- install
```

Choose a different stack for another project:

```bash
localweb project add ~/Code/blog --php 8.3 --web apache \
  --db postgres --document-root public --domain blog.dev
localweb project start blog
```

PHP 8.3 and PostgreSQL must be present in the installed runtime for that
example. **Details → Show connection settings** gives you the database,
Redis and email values. **Copy as .env** copies them; LocalWeb does not edit
your app's configuration file. From the CLI:

```bash
localweb env acme --format dotenv
```

Start prints the direct URL (`http://127.0.0.1:<web port>`); pin the ports
with `--web-port` and `--db-port`, or let LocalWeb allocate them from the
fixed ranges above. `localweb project info <name>` shows both endpoints, and
the environment values expose the database proxy port as `DB_PORT`.

On Linux the HTTPS gateway selects a dedicated address from `127.0.0.2/24`
and normally owns port 443 there. Exact `/etc/hosts` entries point only
LocalWeb domains to that address, so another stack may continue using
`127.0.0.1:443` while `https://indeks.dev` remains portless. A high port in
`8443-8499` is a last-resort fallback and is always shown explicitly. `.dev`
domains use browsers' built-in HSTS policy; `.test` remains available.
`localweb ca install` registers the public development CA with both Ubuntu and
the current user's Brave/Chromium NSS database; restart browsers that were
already running during the first installation.

## Background work

| Preset | Worker | Scheduled task |
| --- | --- | --- |
| Laravel | `artisan queue:work` | `artisan schedule:run` every minute |
| Symfony | `bin/console messenger:consume async` | Supply a schedule if your app needs one |
| WordPress | Custom command if needed | `wp-cron.php` every five minutes; disable WordPress's page-triggered cron in your app configuration |
| Drupal | Custom command if needed | `vendor/bin/drush cron` every fifteen minutes; install Drush in the project |
| Other apps | Custom executable and arguments | Custom schedule and command |

Enable a preset with `--supervisor` or `--cron-preset`. A custom worker uses
`--supervisor-command` and one `--supervisor-arg` per literal argument.
`@php` uses the project's selected PHP and private PHP configuration.
The desktop exposes the same presets and custom-command fields.

## Common commands

```bash
localweb status
localweb doctor
localweb runtime list
localweb project list
localweb project info acme
localweb project restart acme
localweb project stop acme
localweb node acme -- --version
localweb npm acme -- run build
localweb npx acme -- --version
localweb exec acme -- php artisan about
localweb shell acme
localweb autostart enable
localweb shutdown
```

Closing the desktop window leaves projects running. **Stop LocalWeb** stops
the managed environment; **Stop** on a project affects that project.
The displayed project state is the daemon's active-project state, not a
continuous health check of every service. When a project starts or restarts,
the daemon reads a bounded HTTP/1.x status line from its direct loopback port:
2xx/3xx and authentication challenges (401/403) count as reachable, while
404, 5xx, malformed responses and timeouts fail the start. Gateway state is
checked through the native service manager. Worker counts describe configured
workers.

## AI assistants (MCP)

The package installs `localweb-mcp`, a Model Context Protocol server that
exposes the LocalWeb daemon over stdio. Point any MCP-capable client at the
binary to let an assistant create a project, start it, run `composer`,
framework commands or tests inside the project runtime, read logs and open
the environment values. The tool set covers status and diagnostics, daemon
start/stop, project add/remove/start/stop/restart, `@php`/`@composer`/`@node`/
`@npm`/`@npx` command execution, environment, runtime list, certificate
status/installation, hosts synchronization and login autostart.

The desktop **Settings** view shows the resolved `localweb-mcp` path, copies
the configuration below, and lists the active environment values.

```json
{
  "mcpServers": {
    "localweb": {
      "command": "localweb-mcp"
    }
  }
}
```

The assistant connects to the same user daemon as the desktop app; start
LocalWeb once (`localweb start`) or use the `localweb_daemon_start` tool.
Privileged steps (CA installation, `/etc/hosts` changes) still require the
normal polkit authentication.

## Remove LocalWeb

```bash
sudo apt remove localweb     # Ubuntu
sudo pacman -R localweb      # Arch / Omarchy
```

Package removal stops LocalWeb's services and removes its system integration,
binaries, logs and host mappings. **Per-user data is preserved:** projects,
databases, settings, generated certificates and the private CA keys are never
deleted by `apt remove` or `pacman -R`, and upgrades keep them as well.
To erase this account's LocalWeb data explicitly:

```bash
localweb ca remove              # remove this account's CA from system trust
localweb data purge --dry-run   # list directories and sizes
localweb data purge             # requires typing "purge" to confirm
```

`localweb data purge` only removes directories owned by the calling account
and refuses symlinked paths. Source project folders outside LocalWeb data
directories are never touched.

## Build and contribute

Use stable Rust and the native build inputs described in
[DEVELOPMENT.md](DEVELOPMENT.md). The runtime staging tools need Python 3
on the **build host**; the installed application does not use Python,
Electron or a browser shell.

See [installation](docs/installation.md), [troubleshooting](docs/troubleshooting.md),
[architecture](ARCHITECTURE.md), [security](SECURITY.md) and
[packaging notes](packaging/distros.md).

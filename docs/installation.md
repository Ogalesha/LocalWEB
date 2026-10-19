# Installation and first steps

Download LocalWeb only from its [GitHub repository](https://github.com/Ogalesha/LocalWEB):
release assets, or artifacts from a successful **Native packages and acceptance**
workflow. Check the package's SHA-256 against its accompanying `SHA256SUMS`.
The runtime contents of the package determine which PHP versions are included.

## Linux

Use the package for your native distro:

```bash
sudo apt install ./localweb_0.1.2_amd64.deb         # Ubuntu 26.04
sudo pacman -U ./localweb-0.1.2-1-x86_64.pkg.tar.zst # Arch / Omarchy
```

You need a desktop-user session with systemd and system authorization.
The private web stack is included; a separate system-wide LAMP installation
is not required. Arch packages also target Omarchy's Arch base, but CI on
Arch is not a substitute for testing an actual Omarchy desktop session.

## macOS

Download `LocalWeb-0.1.2.pkg` (universal: Apple silicon and Intel) and
install it:

```bash
sudo installer -pkg ./LocalWeb-0.1.2.pkg -target /
```

The package installs the CLI and daemon tools under `/usr/local` and the
desktop app as `/Applications/LocalWeb.app`. It is unsigned, so Gatekeeper
warns until notarization exists. Runtimes come from Homebrew; install the
formulae for the services you use (`php@8.5`, `nginx`, ...). The port is
implemented but has not yet been exercised on Apple hardware: see
[docs/macos-porting.md](macos-porting.md) for the checklist.

## Windows

Download `localweb-0.1.2-windows-x86_64.zip` and extract it. Run
`localweb-gui.exe` for the desktop app or `localweb.exe` from a terminal.
The build is cross-compiled and exercised under Wine in CI, but has not been
run on real Windows yet: see [docs/windows-porting.md](windows-porting.md).
Per-user state lives under `%LOCALAPPDATA%`, the daemon listens on a per-user
named pipe, and the runtime root defaults to `%ProgramFiles%\LocalWeb\runtime`
(override with `LOCALWEB_RUNTIME_ROOT`).

## Setup

In the desktop app, choose **Set up LocalWeb**, then **Install CA**.
The equivalent terminal commands are:

```bash
localweb setup
localweb ca install
localweb start
```

Setup creates private configuration and data directories, a development
CA and background-service integration. CA installation trusts local HTTPS
domains. Setup, CA installation and domain changes use system authorization.
Closing the window or terminal keeps projects running.

## Add an application

Select an existing source folder. Check the **Application preset** and
**Document root**, then choose PHP, web server, database and optional services.
Framework detection is a convenience: choose **Generic PHP** for another
framework, a custom PHP app or static files, and supply any worker or cron
commands it needs. No framework code is generated or installed by LocalWeb.

```bash
localweb project add ~/Code/acme --php 8.5 --web nginx \
  --db mariadb --cache redis --document-root public
localweb project start acme
```

That folder answers on its fixed direct HTTP port (`localweb project info`
prints it) and, when the gateway can bind port 443, on `https://acme.dev`.
Pin the ports with `--web-port` and `--db-port` when another local stack
needs a different assignment. `--domain acme.test` avoids browsers'
built-in HSTS policy for `.dev`.

The project **Details** panel shows configuration and optional connection
settings. Use those values in your app; LocalWeb does not modify `.env`.
The terminal equivalent is `localweb env acme --format dotenv`.

Adding a project does not immediately start it. Project autostart means
it starts on the next LocalWeb launch. Login autostart is separately opt-in:

```bash
localweb autostart enable
```

## Daily use and removal

Use **Start**, **Stop**, **Restart**, **Open** and **Mailpit** on each project.
**Sync local domains** retries a failed hosts update; **Refresh status**
rechecks connectivity; **Repair setup** repeats setup after an incomplete run.
**Stop LocalWeb** stops the managed environment without deleting project data.

`localweb runtime list` checks the manifest and required files; it does not
display all recorded versions or verify hashes at runtime. The complete
provenance is in `runtime-manifest.json` inside the installed runtime.

Removing the package deletes LocalWeb-owned database data and settings,
so export databases first. Source folders are retained; use your package
manager to remove LocalWeb.

## Building

Use the native pipeline or [DEVELOPMENT.md](../DEVELOPMENT.md). Cross-platform
Rust compilation alone does not produce a working private runtime package.
Only a successful build **and** native acceptance run qualifies an artifact
for upload to a GitHub Release.

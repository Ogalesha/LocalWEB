# macOS porting

Status: **implemented, not yet validated on Apple hardware.** The macOS code
paths below compile in the `macos-smoke` workflow and are kept cfg-gated so the
Linux product is unchanged, but no step has been exercised on a real Mac yet.
Use the checklist at the end before declaring the port supported.

## Contract to reproduce

macOS must keep the same guarantees the Linux packages already provide:

- one privileged pass installs HTTPS trust and exact hostnames, and the same
  pass can be fully undone;
- uninstall removes every artifact LocalWeb created — launch jobs, trust
  anchor, `/etc/hosts` block, per-user state, runtime directory — and never
  touches project source directories;
- the gateway owns port 443, the daemon runs per user, and project services
  (web server, PHP-FPM, databases, workers) stop with the product.

## Implementation map

| Area | macOS implementation |
| --- | --- |
| Trust store | `crates/localweb-core/src/platform.rs`: `TrustStore::Macos` installs with `security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain`, verifies by SHA-1 fingerprint (`openssl x509` + `security find-certificate -Z`), and removes with `security delete-certificate -c "LocalWeb Development CA"`. The trust copy lives at `/Library/Application Support/LocalWeb/`. |
| Privileges | `crates/localweb-core/src/privilege.rs`: one helper, two launchers. Linux keeps `pkexec`; macOS runs `osascript -e 'do shell script "…" with administrator privileges'`. Stdin becomes a mode-0600 temp file; the invoking uid and the user's runtime directory travel as `LOCALWEB_INVOKING_UID` and `LOCALWEB_RUNTIME_DIR`. CLI and MCP both call this module. |
| Helper | `crates/localweb-helper`: accepts `LOCALWEB_INVOKING_UID` instead of `PKEXEC_UID` on macOS, resolves the home with `dscl`, guards the runtime directory under `$TMPDIR` instead of `/run/user`, writes the registry to `/usr/local/var/lib/localweb/users`, and routes CA work through `TrustStore`. `set-gateway-cap` is a no-op because capabilities do not exist on macOS. |
| Service manager | `crates/localweb-daemon/src/backend.rs`: Linux keeps transient `systemd-run --user` units; macOS spawns daemon-owned children with pidfiles under `$TMPDIR/localweb/services`, logs under `~/.local/state/localweb/logs`, and a `/bin/sh` restart loop for workers. Arguments never pass through the shell for systemd-run; the macOS loop receives them as positional parameters (`"$@"`). |
| Gateway | Runs as a per-user LaunchAgent (`dev.localweb.gateway`), not a root LaunchDaemon. The plist declares `Sockets → LocalWebHTTPS` (127.0.0.1:443) so launchd owns the privileged port; the gateway takes the socket with `launch_activate_socket("LocalWebHTTPS")` and falls back to a plain bind when started manually. The daemon bootstraps and kickstarts it, and boots it out on shutdown. |
| Daemon lifecycle | `crates/localweb-core/src/service.rs`: `restart_daemon`, `set_autostart` and `write_user_units` abstract systemd and launchd. `localweb setup` writes `~/Library/LaunchAgents/dev.localweb.daemon.plist` and `dev.localweb.gateway.plist`; the daemon agent uses `KeepAlive → SuccessfulExit=false` so `localweb shutdown` is not undone, and the gateway agent restarts on crash. |
| Runtime paths | `Paths::discover` uses `$TMPDIR/localweb` when `XDG_RUNTIME_DIR` is absent. The registry and uninstaller treat `/private/var/folders/...` as valid. |
| Runtimes | No darwin bundle: `RuntimeLayout` probes Homebrew (`/opt/homebrew`, then `/usr/local`) for `php@X`/`php`, `node@X`/`node`, `nginx`, `httpd`, `mariadb`, `mysql`, `postgresql@14…17`, `redis`, `mailpit` and Composer's PHAR. nginx/Apache/MariaDB/MySQL/PostgreSQL configuration helpers expose the Homebrew `etc`/`share`/`lib` directories instead of the Linux runtime root. |
| hosts and DNS | Same managed block in `crates/localweb-core/src/hosts.rs`; the uninstaller flushes `dscacheutil` and `mDNSResponder` after removal. |
| Packaging | `packaging/macos/build-pkg.sh` builds `dist/LocalWeb-<version>.pkg` on macOS with `pkgbuild`: CLI, `localwebd`, gateway, helper and MCP server under `/usr/local/libexec/localweb`, `localweb` symlink, and `localweb-uninstall`. Per-user setup stays explicit. |
| Uninstall | `packaging/macos/uninstall.sh` boots out both LaunchAgents/socket jobs, kills prefix processes, sweeps only registered user paths (home or `/private/var/folders/*/*/T/localweb`), removes the hosts block, the CA and its trust copy, app paths, receipts and the DNS cache. Project sources are never touched. |
| CI | `.github/workflows/macos-smoke.yml` runs `cargo check --workspace --all-targets` and the portable core/gui tests on `macos-latest`. |

## First run on a MacBook

```sh
# 1. Toolchain and runtimes (Homebrew, Apple silicon paths shown).
brew install php@8.5 nginx httpd mariadb mysql postgresql@17 redis mailpit node composer

# 2. Build and install the package.
cargo build --release --workspace
sh packaging/macos/build-pkg.sh
sudo installer -pkg dist/LocalWeb-0.1.0.pkg -target /

# 3. Per-user setup (each privileged step asks for an administrator password).
localweb setup          # LaunchAgents, CA in ~/.local/share/localweb, path registry
localweb ca install     # System keychain trust
localweb project add ~/Sites/acme --name acme --php 8.5 --web-server nginx
localweb start
open https://acme.dev
```

The daemon and gateway logs live in `~/.local/state/localweb/logs/`
(`daemon.log`, `gateway.log`, `localweb-<service>.log`).

## Known gaps and risks

- **No hardware run yet.** The gateway socket-activation path, launchd
  enable/disable semantics, `security` fingerprint checks and the `osascript`
  quoting must be exercised on the target Mac.
- **GUI bundle.** Only the CLI, daemon, gateway, helper and MCP server are
  packaged; an `.app` wrapper (and its icon/notarization) is still to do.
  `cargo run -p localweb-gui` works for local testing where eframe builds.
- **Homebrew is a prerequisite.** Runtimes are detected, never installed
  automatically; missing formulae surface as `runtime is not installed`
  errors naming the component.
- **Firefox.** It keeps its own NSS store; Safari/Chrome use the system
  keychain. Add NSS handling if the acceptance run needs Firefox.
- **`$TMPDIR` lifetime.** The OS may purge it between sessions; the daemon
  recreates its directories on start. If it proves unreliable under sleep,
  move the runtime under `~/Library/Caches/localweb/run`.
- **Formula layout drift.** nginx MIME paths, Apache module directories and
  PostgreSQL `share` names are probed but not guaranteed across Homebrew
  versions; adjust the helpers in `runtime.rs` if a probe misses.
- **Notarization.** Gatekeeper will warn on an unsigned `.pkg`; signing and
  notarization are outside this change.

## Verify on this repository (no Mac needed)

- `cargo check --target aarch64-apple-darwin` does not link here because
  `ring` needs a Darwin C toolchain, so compilation is proven by the
  `macos-smoke` workflow instead.
- `cargo fmt --check` parses every macOS module, including cfg-gated code.
- `sh -n packaging/macos/uninstall.sh` and `sh -n packaging/macos/build-pkg.sh`
  validate script syntax.

## Uninstall parity checklist (run on hardware)

1. `launchctl print gui/$UID/dev.localweb.daemon` and
   `launchctl print gui/$UID/dev.localweb.gateway` fail; both plists are gone
   from `~/Library/LaunchAgents`.
2. No process runs from `/usr/local/libexec/localweb`.
3. `/etc/hosts` has no `# BEGIN LOCALWEB` block; DNS cache flushed.
4. `security find-certificate -c "LocalWeb Development CA"` finds nothing in
   `/Library/Keychains/System.keychain`, and
   `/Library/Application Support/LocalWeb` is gone.
5. `/usr/local/libexec/localweb`, `/usr/local/var/lib/localweb`,
   `/usr/local/var/log/localweb` and `/Applications/LocalWeb.app` are gone;
   `pkgutil --pkg-info dev.localweb.pkg` reports "No receipt".
6. Per user: `~/.config/localweb`, `~/.local/share/localweb`,
   `~/.local/state/localweb` and `~/Library/LaunchAgents/dev.localweb.*.plist`
   are gone; `$TMPDIR/localweb` removed when still present.
7. The registry sweep removes only paths listed in
   `/usr/local/var/lib/localweb/users/<uid>.paths` that sit under the user's
   home or the per-user temp directory — project source folders survive.
8. Reinstalling the `.pkg` reproduces a working environment (trust + hosts +
   gateway) from a clean state.

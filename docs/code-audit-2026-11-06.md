# Code audit — 6 November 2026

Primary target: Ubuntu 26.04.1 amd64, Rust 1.98.1. Reviewed the seven Rust
crates, native service integration, runtime/configuration generation,
privileged operations, packaging scripts and release checks. Fixes include
regression tests; native tests use temporary directories, ports and audit
units without changing project services or system trust.

## Fixed findings

| Priority | Finding | Correction and evidence |
| --- | --- | --- |
| High | Web roots exposed `.env` and `.git` over HTTP. | nginx and Apache deny hidden paths; real HTTP tests verify denial and preserve `.well-known`. |
| High | Failed shutdown still terminated the daemon; a disconnected client could prevent successful shutdown from finishing. | Notify only after successful shutdown, including failed response writes; both paths have regression tests. |
| High | Linux workers awaiting an automatic restart escaped stop. | Stop loaded units even when `is-active` is false; tested against an actual systemd restart delay. |
| High | Concurrent credential requests could replace passwords or allocate conflicting ports. | Serialize allocation and persistence; eight concurrent requests return identical credentials and ports. |
| Medium | Enabling Redis/Mailpit after credentials already existed left service ports unset. | Fill missing ports while preserving credentials and existing reservations. |
| Medium | Transient unit restart kept the previous executable, arguments and environment; systemd expanded dollar variables in arguments. | Recreate units and disable environment expansion; a native test verifies changed arguments and literal `$HOME`. |
| Medium | nginx/Apache/Redis configuration broke on paths containing spaces. | Quote paths and use Apache's filesystem-aware FastCGI handler. Native tests serve PHP scripts and framework routes and ping Redis, including paths with spaces. |
| Medium | Database proxy stop left established forwarding tasks alive. | Own connection tasks with a JoinSet and abort on teardown; tests verify both endpoints close and the port can be rebound. |
| Medium | Protocol replies and MCP command output were fully buffered before checking limits. | Bound protocol reads and drain child pipes while retaining only capped output; tests cover oversized replies without EOF and verbose commands. |
| Medium | Cron treated `7` as an alias for zero in every field, missed Sunday ranges and used incorrect day/month step origins and day restrictions. | Scope Sunday aliases to weekdays, anchor steps to field minima, and implement cron's day matching rules. |
| Medium | Port migration ignored database reservations in the web range; automatic database allocation could collide with an explicit web port. | Use combined reservations and reserve explicit ports before allocation; regression tests cover both cases. |
| Medium | Saved project settings bypassed validation; malformed secret ports could overflow. | Validate names, hostnames, schemas, versions, schedules, file identity and service port ranges on load. Missing source folders can still be listed or removed. |
| Medium | Concurrent daemon startup could race on socket cleanup and mutate state before detecting another daemon. | Lock the Unix endpoint before initialization, preserve non-socket files, and only recover confirmed stale sockets. |
| Medium | The hygiene check interpreted the private-key pattern's leading dashes as an option. | Pass patterns with `-e`; an isolated Git fixture proves detection. |
| Medium | MCP built Windows PATH with a Unix colon. | Use the standard platform-aware path join operation. |

## Dependencies

RustSec database commit `309392b18f6b17022f82c701f300433126901068`, updated
6 November 2026. Initial scan: six vulnerability advisories and one Tokio
I/O-safety advisory. Updated:

- h2 0.4.14 → 0.4.20
- rustls 0.23.45 → 0.23.46
- rustls-webpki 0.103.15 → 0.103.16
- Tokio 1.53.1 → 1.54.1

The resulting lockfile has **zero vulnerability advisories** and no
I/O-safety advisory. Several TLS advisories concern optional client features
that the gateway does not enable; the scan identifies affected versions,
not proof that every advisory was exploitable in LocalWeb.

Four maintenance notices remain: `rustls-pemfile`, `zbus_names`, `zvariant`
and `zvariant_derive`. No advisories are suppressed. Replacing the PEM reader
and moving the desktop D-Bus dependency chain need separate compatibility
work. CI now runs RustSec checks as part of the release gate.

## Verification

- Full workspace suite: 101 tests passed; four opt-in native tests run separately.
- Four native Ubuntu regressions passed: both web servers/PHP/Redis, paths
  with spaces, systemd argument replacement, and pending restart cancellation.
- Formatting, Clippy with warnings denied, shell/Python syntax, hygiene and
  the hygiene regression passed.
- Release build and Debian package layout passed.

Native regressions now run in the Linux package pipeline. Reproduce locally:

```sh
cargo test --workspace --locked
cargo test -p localweb-daemon -- --ignored
cargo clippy --workspace --all-targets --locked -- -D warnings
bash scripts/test-hygiene.sh
```

The opt-in tests require a systemd user session and a complete Linux runtime.
Set `LOCALWEB_RUNTIME_ROOT` if the runtime is outside its installed location.

## Coverage limits and follow-up

The complete privileged 48-combination install/removal acceptance was not run
on the user's workstation; it belongs on the disposable CI host. Existing
databases, project sources, hosts and certificate trust were not changed.

Windows received the PATH fix and shared logic fixes. Cross-checking stopped
at the missing MSVC C/C++ toolchain (`lib.exe`); macOS and Windows native
acceptance remain unverified here. Their PID-file process ownership and
crash recovery deserve a native follow-up before claiming Ubuntu parity.

## Product copy

After the audit, English desktop, CLI, MCP and gateway copy was shortened
and aligned around one terminology guide. Repeated outcomes share constants
in `localweb-core::copy`; project removal and permanent data deletion remain
explicitly distinct. First launch reports missing setup rather than an
unavailable service, consistently in the sidebar, overview and project view.
The final workspace suite, Clippy and release build passed; an isolated
English desktop screenshot was also inspected. See [English product copy](copy-style.md).

Cron behavior follows the [Linux crontab rules](https://www.man7.org/linux/man-pages/man5/crontab.5.html).
Apache uses the documented [FastCGI handler mapping](https://httpd.apache.org/docs/2.4/mod/mod_proxy_fcgi.html#examples).

# Windows porting

Status: **covered by native Windows regressions and Wine smoke checks;
full web-stack and interactive desktop validation remain.** The Windows code paths below compile in the
`windows-wine` job, which cross-builds the workspace with
`x86_64-pc-windows-msvc` and runs the CLI, the daemon, the MCP server and the
GUI (screenshot) under Wine before any artifact is packaged.

## Contract to reproduce

Windows must keep the same guarantees the Linux and macOS builds provide:

- HTTPS trust is installed for the current user; exact hostnames use the
  elevated helper. Both operations can be undone;
- uninstall removes every system artifact LocalWeb created while preserving
  per-user data until `localweb data purge` is run explicitly, and never
  touches project source directories;
- the gateway owns port 443, the daemon runs per user, and project services
  stop with the product.

## Implementation map

| Area | Windows implementation |
| --- | --- |
| Transport | `crates/localweb-core/src/transport.rs`: a per-account named pipe keyed by the native SID, with an explicit DACL and server-account verification. |
| Paths | `crates/localweb-core/src/lib.rs` and `platform.rs`: per-user directories under `%LOCALAPPDATA%`. |
| Trust store | `crates/localweb-core/src/platform.rs`: CLI, desktop and MCP use `certutil -user -addstore -f Root` without elevation. Query and removal select the exact DER certificate thumbprint, independently of display language or PEM formatting. |
| Privileges | `crates/localweb-core/src/privilege.rs`: the same helper binary runs with a Windows launcher instead of `pkexec`/`osascript`. |
| Service manager | `crates/localweb-core/src/service.rs`: the daemon is registered as the scheduled task `LocalWeb Daemon`; `crates/localweb-daemon/src/backend.rs` spawns daemon-owned children with pidfiles and restart loops instead of `systemd-run` units. |
| Runtimes | `crates/localweb-core/src/runtime.rs`: the runtime root defaults to `%ProgramFiles%\LocalWeb\runtime` (override with `LOCALWEB_RUNTIME_ROOT`), PHP is probed as `php.exe`/`php-cgi.exe` per version, and Composer is discovered through Scoop and Chocolatey installs. |
| GUI | `crates/localweb-gui`: eframe build exercised under Wine with `EFRAME_SCREENSHOT_TO`; the ZIP carries all six executables. |
| CI | `.github/workflows/native-builds.yml`: native Windows runs Clippy, workspace regressions and an explicit current-user CA integration check. The separate `windows-wine` job cross-builds and exercises the portable ZIP. Both gate publication. |

## Test under Wine on Linux

```sh
# Inside a container with wine64, clang/lld/llvm, xvfb, python3 and cargo-xwin:
bash scripts/windows-wine-smoke.sh
```

## Known gaps and risks

- **Desktop and stack acceptance.** Native CI checks certificate trust and
  pipe ownership. Scheduled-task integration, external web runtimes and the
  interactive GUI still need a complete Windows desktop acceptance run.
- **No installer.** The release ships a portable ZIP; a signed installer
  (MSI or similar) is still to do.
- **Runtime layout drift.** PHP probing relies on the documented layout under
  the runtime root; adjust `windows_php_directories` if installers change it.
- **Unsigned binaries.** SmartScreen will warn until the executables are
  code-signed.

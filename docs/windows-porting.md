# Windows porting

Status: **implemented and exercised under Wine in CI, not yet validated on
real Windows hardware.** The Windows code paths below compile in the
`windows-wine` job, which cross-builds the workspace with
`x86_64-pc-windows-msvc` and runs the CLI, the daemon, the MCP server and the
GUI (screenshot) under Wine before any artifact is packaged.

## Contract to reproduce

Windows must keep the same guarantees the Linux and macOS builds provide:

- one privileged pass installs HTTPS trust and the exact hostnames, and the
  same pass can be fully undone;
- uninstall removes every system artifact LocalWeb created while preserving
  per-user data until `localweb data purge` is run explicitly, and never
  touches project source directories;
- the gateway owns port 443, the daemon runs per user, and project services
  stop with the product.

## Implementation map

| Area | Windows implementation |
| --- | --- |
| Transport | `crates/localweb-core/src/transport.rs`: a per-user named pipe (`\\.\pipe\localweb-<user>`); the daemon serves it and CLI/MCP connect to it. |
| Paths | `crates/localweb-core/src/lib.rs` and `platform.rs`: per-user directories under `%LOCALAPPDATA%`. |
| Trust store | `crates/localweb-core/src/platform.rs`: `TrustStore::Windows` adds the CA to the current user's Root store with `certutil -user -store Root`, verifies by SHA-1 fingerprint and removes it on request. |
| Privileges | `crates/localweb-core/src/privilege.rs`: the same helper binary runs with a Windows launcher instead of `pkexec`/`osascript`. |
| Service manager | `crates/localweb-core/src/service.rs`: the daemon is registered as the scheduled task `LocalWeb Daemon`; `crates/localweb-daemon/src/backend.rs` spawns daemon-owned children with pidfiles and restart loops instead of `systemd-run` units. |
| Runtimes | `crates/localweb-core/src/runtime.rs`: the runtime root defaults to `%ProgramFiles%\LocalWeb\runtime` (override with `LOCALWEB_RUNTIME_ROOT`), PHP is probed as `php.exe`/`php-cgi.exe` per version, and Composer is discovered through Scoop and Chocolatey installs. |
| GUI | `crates/localweb-gui`: eframe build exercised under Wine with `EFRAME_SCREENSHOT_TO`; the ZIP carries all six executables. |
| CI | `.github/workflows/native-builds.yml`: the `windows-wine` job runs in an Ubuntu 26.04 container (clang 21 links the MSVC STL headers), then `scripts/build-windows-zip.sh` packages `localweb-<version>-windows-x86_64.zip`. |

## Test under Wine on Linux

```sh
# Inside a container with wine64, clang/lld/llvm, xvfb, python3 and cargo-xwin:
bash scripts/windows-wine-smoke.sh
```

## Known gaps and risks

- **No hardware run yet.** The `certutil` trust path, scheduled-task
  semantics, named-pipe ACLs and the GUI on a real Win32 desktop must be
  exercised on a Windows machine before declaring the port supported.
- **No installer.** The release ships a portable ZIP; a signed installer
  (MSI or similar) is still to do.
- **Runtime layout drift.** PHP probing relies on the documented layout under
  the runtime root; adjust `windows_php_directories` if installers change it.
- **Unsigned binaries.** SmartScreen will warn until the executables are
  code-signed.

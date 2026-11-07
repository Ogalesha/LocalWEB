# Runtime and release audit — 7 November 2026

Follow-up to the Ubuntu audit on 6 November. Reviewed project startup,
HTTP/FastCGI configuration, CLI/MCP execution, workers, scheduled tasks,
macOS/Windows process ownership and public download publication. Ubuntu
26.04 is the primary local verification host.

## Confirmed fixes

| Priority | Finding | Fix and regression coverage |
| --- | --- | --- |
| High | Scheduled commands survived project stop; a tick could start work using an obsolete running-project snapshot. | Track job handles, serialize registration with project operations and cancel only the selected project's jobs before stopping its services. Regression checks project isolation. |
| High | Cron buffered arbitrary command output; MCP timeouts only killed the immediate child. | Share a command runner that drains both pipes with bounded retention and cancels the owned process tree. Tests exercise verbose output and a real child/grandchild tree, including Windows in Wine CI. |
| High | macOS/Windows PID files could target a reused PID. macOS also accepted zero/negative process-group IDs. | Store PID plus native process creation time, reject unverifiable records and signal only a matching process. Windows retains a verified process handle during termination; macOS creates and checks a dedicated process group. Tests reject malformed/legacy records and mismatched creation times. |
| High | Direct web ports served the default project for unrelated Host headers. | Add explicit rejecting default hosts for Nginx and Apache. Native requests reject foreign and suffix-spoofed hostnames while preserving project and loopback access. |
| Medium | Windows could spawn a worker after stop had cancelled it; failed identity persistence left a child running. | Serialize worker ownership checks, spawn and identity persistence with cancellation. Reap children when recording ownership fails. |
| Medium | Nginx omitted client address/port and server address/port from FastCGI. Apache reported port 80 on a high-port listener. | Supply standard CGI parameters and enable Apache's physical-port fallback. Real PHP requests verify metadata with both servers, including paths containing spaces. |
| Medium | HTTP 500 produced a generic health-check error pointing only at server logs. | Preserve the observed status and direct application errors to app logs and connection settings. Root 404 and missing HTTP responses get distinct guidance. A socket-level regression verifies the failed-start message. |
| Medium | Saved project environment variables were ignored by CLI/MCP and background commands. | Validate names/values, export them to CLI, MCP, workers and cron, and share CLI/MCP command setup. Runtime shims and managed PHP settings keep precedence. A real child process verifies literal values, PATH and working directory. |
| Medium | Linux workers stayed stopped after a clean exit, including framework worker restarts. | Use restart-on-exit supervision. A real systemd worker exits successfully and is started again with its literal environment intact. |
| Medium | macOS runtime discovery could report another PHP version's generic Homebrew binary as available; Windows checked CLI but not CGI version. | Require both selected executables to match; prefer private runtimes before system candidates. Regression rejects mixed PHP pairs. |
| Medium | Windows Doctor required a bundled manifest absent from the portable ZIP and described CGI as FPM. | Report the actual native runtime provider and CGI interface. Cross-platform regression expectations now reflect each backend. |
| Medium | Rolling releases retained old versioned installers, homepage links were indirect and source archives pointed at an old commit. | Publish four permanent OS-specific asset names, current checksums and the tested commit. Remove obsolete rolling installers only after replacement succeeds. Align the rolling source tag. Six publisher tests cover trusted run selection, complete matching versions, hashes and cleanup ordering. |

## Local application recovery

The reported startup failure was an application HTTP 500: its configuration
still referenced another stack's internal database hostname. Its 31-table
database was copied into an empty LocalWeb database, with private backups
of the original configuration and SQL export. Only connection settings and
the app's base URL changed. The original stack's database was preserved.

With the other stack stopped, Laravel's database inspection succeeded through
LocalWeb's project proxy and the site returned HTTPS 200 using the local CA.
No framework-specific bootstrap code was added to LocalWeb. Applications
still own their configuration and data-import steps.

## Verification and limits

- Local Ubuntu workspace regression suite, formatting and Clippy with
  warnings denied: 112 tests passed; five opt-in native service tests passed separately.
- Native checks cover Nginx/Apache/PHP/Redis, request metadata, paths with
  spaces, literal systemd arguments, cancellation of pending restarts and
  worker restart after exit 0.
- macOS CI now runs the whole workspace suite and Clippy. Windows CI
  cross-builds and runs core/daemon regressions under Wine in addition to
  the existing CLI, named-pipe, MCP and GUI smoke checks.
- Updated official GitHub checkout/upload/download actions after checking
  their current runtime requirements and artifact contracts.
- The latest-download publisher passed against the previous verified build;
  all four public assets, their SHA-256 values and source commit were checked.

Full Linux install/removal acceptance runs on disposable Ubuntu and Arch CI
hosts. A real Omarchy desktop and full native web-stack acceptance on macOS
and Windows remain separate from compilation, unit tests and Wine smoke.
Their reference packages contain application binaries and use separately
installed native tools; the README now states this explicitly.

macOS/Windows upgrades should stop LocalWeb before replacing its binaries.
An older PID-only record is reported as unverifiable rather than trusted to
terminate an arbitrary process. No service-ownership checks are bypassed.

Source references:
[PHP-FPM environment](https://www.php.net/manual/en/install.fpm.configuration.php),
[Apache canonical port](https://httpd.apache.org/docs/2.4/en/mod/core.html#usecanonicalphysicalport),
[Tokio process cancellation](https://docs.rs/tokio/latest/tokio/process/index.html),
[artifact download contract](https://github.com/actions/download-artifact).

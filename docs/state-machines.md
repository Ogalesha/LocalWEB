# Application lifecycle audit

Reviewed the application-owned workflows in the desktop, CLI, daemon,
gateway, privileged helper, MCP server and release scripts. Ordinary
on/off settings remain booleans. Lifecycle states use enums, correlated
requests or ownership of a resource.

| Workflow | States and transition rules | Result |
| --- | --- | --- |
| Desktop daemon lifecycle | Checking, running, unavailable, explicitly stopped, setting up, starting, stopping. Confirmed results advance the state; unrelated probes cannot cancel an explicit stop. | One `DaemonState` replaces contradictory connection/transition flags. |
| Desktop asynchronous results | Pending ticket, accepted completion, superseded completion. Mutations advance a generation; only the latest query in that generation may update a snapshot. | Late status/project/environment replies cannot overwrite newer state. Pending counts derive from owned tickets; duplicate completions do nothing. |
| Project controls | Starting, stopping or restarting one named project; awaiting the authoritative project refresh. | Background errors no longer clear a project's progress. |
| Tray initialization | Uninitialized, unavailable, ready with an owned tray icon. | Closing the window hides it only when a working tray can restore it. |
| Form and folder navigation | Add/edit mode, pending command, optional queued add; bounded back/forward history. | Existing identity and draft-preservation rules remain; queued adds wait for a reachable daemon. |
| Daemon request lifecycle | Restoring → serving → stopping → stopped. Failed shutdown returns to serving for retry. | Status stays responsive during restoration while mutations queue behind the startup guard. Queued mutations cannot start services after successful shutdown. |
| Project activation | Start, restore or restart intent; prepare → health check → durable commit, with rollback on failure. | Typed intent replaces paired flags. Restart cancels the project's old scheduled commands before replacing services. |
| Observed/persisted running set | Committed or retry needed. A completed stop changes memory before persistence; failed writes retry during maintenance. | A disk error cannot keep a stopped project marked running. Boot rewrites the successfully restored set. |
| Partial runtime failure | Running or recovery needed; successful activation clears recovery. | Failed stop/rollback cannot become a false idempotent Start. Recovering a project recreates its owned services; boot reconciles leftover inactive services. |
| Unexpected service/listener exit | A saved running entry is reusable only while its required services and owned proxy task remain alive. | Start recreates interrupted services and finished proxies. Native acceptance deliberately stops the web server and checks recovery through Start. |
| Project update/remove | Validate → stop → save → restart; restore settings and restart separately on failure. | Recovery messages distinguish failed settings restoration from failed restart. Removal clears obsolete schedule history. |
| Scheduler | Idle, running with an owned task, completed or cancelled; one dispatch per matching minute. | Shutdown cancels and joins all jobs. Maintenance releases finished/panicked task records. |
| Database proxies | Bound listener with owned connection tasks → shutdown → released listener. | Existing `DbProxy` ownership closes established connections on stop and rollback. Shutdown drains the proxy registry. |
| Worker/service ownership | Absent, live matching process, stale identity, unverifiable legacy record; optional supervised restart owner. | Existing PID/creation-time verification, cancellation and service-manager stop guards remain. Windows system tools now resolve through the native system directory. |
| Certificates | Missing pair, incomplete pair or complete pair; complete material must pass validation before reuse. | Preserve and reject invalid CA identity. Reissue a stale leaf when its CA, hostname, key, validity or server usage no longer matches. Restore the previous key if replacing its certificate fails. |
| Certificate trust | System and browser trust are checked independently against the exact CA. | Current and legacy NSS stores are handled during install and removal; SSL trust flags are required. |
| Gateway | Selected endpoint → service start → persisted active endpoint; fallback is selected only after standard binding fails. Routes distinguish unknown, stopped and active projects. | Existing endpoint validation and loopback ownership remain. |
| MCP session | Awaiting initialize → awaiting initialized notification → ready. | Tool requests before the handshake and repeated initialization are rejected. Input frames are bounded; oversized input ends the session safely. |
| Bounded child commands | Spawned owned process tree → drained output → reaped child; cancellation drops the owner and terminates descendants. | Existing bounded-output and ownership rules remain. |
| Package removal | Stop owned services → remove integration/trust → preserve source and data. | Remove the LocalWeb CA from both NSS locations, preserving unrelated entries. |
| Publication | Required checks pending/failed/passed → complete matching installers → publish aliases/checksums/source → obsolete-asset cleanup. | Existing same-commit release gates remain. Windows smoke phases now have bounded execution and public progress diagnostics; cleanup targets its own Wine prefix. |

Regression cases exercise late replies, duplicate completion, failed state
file writes followed by recovery, shutdown delivery to late subscribers,
job cancellation, invalid certificate pairs, stale leaf reuse and MCP
handshake ordering. Run the workspace and native suites after completing
the implementation changes. Full web-stack acceptance on macOS and Windows
still requires native runtime installations; compilation and Wine checks
are reported separately.
Wine does not enforce `FILE_FLAG_FIRST_PIPE_INSTANCE`; its smoke suite
excludes only that OS exclusivity test. The required native Windows job
runs it, along with the complete workspace suite, without that exclusion.

Verified locally on Ubuntu on 8 November 2026: 134 workspace tests, seven
native service tests, nine publisher regressions, Clippy with warnings
denied, package layout and an MCP process-level handshake/input-limit check.
A fresh RustSec scan reported no vulnerability advisories; seven notices
about unmaintained or yanked dependencies remain.
A controlled stop of the local nginx process was recovered through Start;
the application returned HTTP 200 again without replacing its data.

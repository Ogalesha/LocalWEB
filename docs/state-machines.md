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
| Form and folder navigation | Add/edit mode, pending command, optional queued add; bounded back/forward history. | Existing identity and draft-preservation rules remain; single click highlights a folder, double click opens it, and Use accepts the highlight. Queued adds wait for a reachable, serving daemon. |
| Daemon request lifecycle | Restoring → serving → stopping → stopped. Failed shutdown returns to serving for retry. | Status stays responsive during restoration and reports readiness; CLI, MCP and desktop wait for serving before mutations. Queued mutations cannot start services after successful shutdown. Shutdown requested during restoration waits for the restore commit instead of cancelling it halfway. |
| Unix daemon endpoint ownership | Lock acquired → listener bound → listener closed → explicit unlock. | A child retaining a duplicated descriptor cannot leave ownership stuck after the listener is dropped. The regression retains that duplicate through rebinding. |
| Project activation | Start, restore or restart intent; prepare → health check → durable commit, with rollback on failure. | Typed intent replaces paired flags. Restart cancels the project's old scheduled commands before replacing services. |
| Observed/persisted running set | Committed or retry needed. A completed stop changes memory before persistence; failed writes retry during maintenance. | A disk error cannot keep a stopped project marked running, abort a completed stop, or terminate a daemon whose services were already restored. Boot treats corrupt desired state as empty and rewrites the successfully restored set. |
| Partial runtime failure | Running, recovery needed or cleared; successful activation or stop clears recovery. | Failed stop/rollback cannot become a false idempotent Start. Status, project responses and Doctor expose recovery; Update stops a recovery project before changing its settings, and boot reconciles leftover inactive services. |
| Unexpected service/listener exit | A saved running entry is reusable only while its required services and owned proxy task remain alive. | Start recreates interrupted services and finished proxies. Native acceptance deliberately stops the web server and checks recovery through Start. |
| Project add/update/remove | Validate → commit configuration → derive routes and certificates; update also stops and restarts when needed. | Adding a stopped project creates its route and leaf immediately. Route refresh after a committed add or remove is best effort: failures mark routes dirty and maintenance retries, so a committed mutation is never reported as an error. Removal clears obsolete schedule history. |
| Scheduler | Idle, running with an owned task, completed or cancelled; one dispatch per matching minute. | Shutdown cancels and joins all jobs. Maintenance releases finished/panicked task records. |
| Database proxies | Bound listener with owned connection tasks → shutdown → released listener. | Existing `DbProxy` ownership closes established connections on stop and rollback. Shutdown drains the proxy registry. |
| Worker/service ownership | Absent, live matching process, stale identity, unverifiable legacy record; optional supervised restart owner. | Existing PID/creation-time verification, cancellation and service-manager stop guards remain. Windows system tools now resolve through the native system directory. Linux and Windows gateways are supervised and restarted after exit like other managed services. |
| Certificates | Missing pair, incomplete pair or complete pair; complete material must pass validation before reuse. | Preserve and reject invalid CA identity; serialize concurrent first-run CA creation. Reissue a stale or partial leaf when its CA, hostname, key, validity or server usage no longer matches. Restore the previous key if replacing its certificate fails. |
| Certificate trust | System and browser trust are checked independently against the exact CA. | Current and legacy NSS stores are handled during install and removal; SSL trust flags are required. |
| Windows user trust | Import → verify exact DER thumbprint → trusted; delete that thumbprint → verify absent. | CLI, desktop and MCP act in the caller's user store without UAC account switching. A required native Windows check verifies import, idempotency, PEM-format independence and removal. |
| Gateway | Selected endpoint → service start → persisted active endpoint; fallback is selected only after standard binding fails. Routes distinguish unknown, stopped and active projects; adds and removals rewrite them. | Endpoint validation and loopback ownership remain. An active gateway is restarted after project changes so hostnames appear and disappear immediately; failed route rewrites are retried by maintenance. macOS liveness requires launchd state `running` and its agent socket is rewritten to match the persisted endpoint before activation. A failed accept is logged and retried with a short delay instead of stopping the gateway. |
| Hosts synchronization | Request snapshot → cross-process lock → privileged helper rewrite → release. | CLI and MCP hold one per-user lock across project lookup and the helper run, so an older snapshot cannot overwrite a newer managed block. |
| MCP session | Awaiting initialize → awaiting initialized notification → ready. | Tool requests before the handshake and repeated initialization are rejected. Input frames are bounded; oversized input ends the session safely. An id-bearing frame without a method is answered as an invalid request, while stray responses stay silent. |
| Bounded child commands | Spawned owned process tree → drained output → reaped child; cancellation drops the owner and terminates descendants. | Existing bounded-output and ownership rules remain. |
| Package removal | Stop owned services → remove integration/trust → preserve source and data. | Remove the LocalWeb CA from both NSS locations, preserving unrelated entries. |
| Publication | Required checks pending/failed/passed → complete matching installers → publish aliases/checksums/source → obsolete-asset cleanup. | Existing same-commit release gates remain. Windows smoke phases now have bounded execution and public progress diagnostics; cleanup targets its own Wine prefix. |

Regression cases exercise late replies, duplicate completion, failed state
file writes followed by recovery, corrupt desired state, shutdown delivery
to late subscribers, shutdown during restoration, job cancellation, invalid
and partial certificate pairs, concurrent CA creation, stale leaf reuse,
bounded picker history, route creation after adding a stopped project, route
rewrites after removal, route-failure retry after a committed removal,
recovery visibility and recovery-aware updates, MCP handshake ordering and
malformed requests, hosts-lock exclusivity, and Windows pipe instance
recovery. Run the workspace and native suites after completing the
implementation changes. Full
web-stack acceptance on macOS and Windows
still requires native runtime installations; compilation and Wine checks
are reported separately.
Wine does not enforce `FILE_FLAG_FIRST_PIPE_INSTANCE`; its smoke suite
excludes only that OS exclusivity test. The required native Windows job
runs it, along with the complete workspace suite, without that exclusion.
The user-certificate integration test is opt-in for local developers and
runs explicitly on the isolated Windows CI host, with cleanup on failure.
The CI wrapper accepts the Windows confirmation only when a native certutil
dialog contains the generated test CA's exact thumbprint. Interactive users
keep the system confirmation; a bounded timeout exposes stalled tests.

Verified locally on Ubuntu on 8 November 2026: 134 workspace tests, seven
native service tests, nine publisher regressions, Clippy with warnings
denied, package layout and an MCP process-level handshake/input-limit check.
A fresh RustSec scan reported no vulnerability advisories; seven notices
about unmaintained or yanked dependencies remain.
A controlled stop of the local nginx process was recovered through Start;
the application returned HTTP 200 again without replacing its data.

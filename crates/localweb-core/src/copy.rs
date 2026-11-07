//! Shared English terminology and outcomes for the desktop, CLI and MCP.

/// The service state shown after startup.
pub const RUNNING: &str = "Running";
/// The service state shown after stopping.
pub const STOPPED: &str = "Stopped";
/// Successful application startup.
pub const LOCALWEB_STARTED: &str = "LocalWeb started";
/// Successful application shutdown.
pub const LOCALWEB_STOPPED: &str = "LocalWeb stopped";
/// Successful trust installation for the development certificate.
pub const HTTPS_TRUSTED: &str = "HTTPS certificate trusted. Restart open browsers if needed.";
/// Successful removal of development certificate trust.
pub const HTTPS_TRUST_REMOVED: &str = "HTTPS trust removed";
/// Successful synchronization of local project domains.
pub const DOMAINS_SYNCED: &str = "Local domains synced";
/// A `LocalWeb` request returned an unexpected response type.
pub const UNEXPECTED_RESPONSE: &str = "Unexpected response from LocalWeb";
/// A request timed out without establishing the service state.
pub const NO_RESPONSE: &str = "LocalWeb didn't respond";
/// Configured background process, distinct from its running state.
pub const BACKGROUND_WORKER: &str = "Background worker";
/// Short worker label for dense project summaries.
pub const WORKER: &str = "Worker";
/// Project removal leaves user data available.
pub const PROJECT_DATA_KEPT: &str = "Source files and databases are kept.";

/// Explains the outcome of removing a project before confirmation.
#[must_use]
pub fn remove_project_prompt(name: &str) -> String {
    format!(
        "Remove {name} from LocalWeb? This stops the project and removes its settings and local domain. {PROJECT_DATA_KEPT}"
    )
}

/// Confirms removal while stating what happened to the project's data.
#[must_use]
pub fn project_removed(name: &str, was_running: bool) -> String {
    let action = if was_running {
        "stopped and removed"
    } else {
        "removed"
    };
    format!("{name} {action}. {PROJECT_DATA_KEPT}")
}

/// Confirms domain synchronization with the selected loopback address.
#[must_use]
pub fn domains_synced(count: usize, address: std::net::Ipv4Addr) -> String {
    format!("{DOMAINS_SYNCED}: {count} to {address}")
}

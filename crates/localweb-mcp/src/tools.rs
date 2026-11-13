//! Tool catalogue for the MCP server.
//!
//! [`definitions`] advertises every tool with a JSON schema; [`call`]
//! dispatches a tool invocation to the daemon protocol or to the privileged
//! helper, mirroring what the desktop application and the `localweb` CLI do.

use localweb_core::{
    Cache, Database, ExecutionSpec, NewProject, Paths, Request, Response, SupervisorConfig,
    WebServer, copy,
    hosts::{HostsSync, HostsSyncLock},
    parse_cron_line, send_request, validate_php_version,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::{process::Command, time::timeout};

/// How long one daemon request may take before the tool reports a timeout.
const DAEMON_TIMEOUT: Duration = Duration::from_secs(120);
/// How long `localweb_daemon_start` waits for project restoration.
const STARTUP_READY_TIMEOUT: Duration = Duration::from_secs(60);
/// Default wall-clock limit for `localweb_project_exec`.
const EXEC_TIMEOUT_DEFAULT: u64 = 300;
/// Hard cap for the `timeout_seconds` argument of `localweb_project_exec`.
const EXEC_TIMEOUT_MAX: u64 = 3600;
/// Bytes of stdout and stderr each `localweb_project_exec` call returns.
const OUTPUT_CAP: usize = 200_000;
/// Hard cap for the `lines` argument of `localweb_logs`.
const LOG_LINES_MAX: u64 = 2000;

/// Returns every tool with its JSON schema, in stable order.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "one declarative schema block per tool is the clearest form"
)]
pub(crate) fn definitions() -> Value {
    let name_property = json!({
        "type": "string",
        "description": "Project name as reported by localweb_projects_list",
    });
    json!([
        {
            "name": "localweb_status",
            "description": "Show LocalWeb, HTTPS gateway and project status.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_daemon_start",
            "description": "Start or restart LocalWeb and wait until it is ready.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_daemon_shutdown",
            "description": "Stop LocalWeb and its services.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_doctor",
            "description": "Check the local setup and suggest fixes.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_runtime_list",
            "description": "List runtimes and their availability.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_projects_list",
            "description": "List projects with settings, URLs and running state.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_project_info",
            "description": "Show a project's settings, URLs and running state.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": name_property },
                "required": ["name"],
            },
        },
        {
            "name": "localweb_project_add",
            "description": "Add an existing folder with detected or default settings. Assign local HTTP and any required database port, then sync its domain. HTTPS needs the gateway. Domain sync may ask for administrator approval.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute project folder path",
                    },
                    "name": {
                        "type": "string",
                        "description": "Project name; defaults to the folder name",
                    },
                    "hostname": {
                        "type": "string",
                        "description": "Local domain for HTTPS. Default: <name>.<domain suffix>",
                    },
                    "web_port": {
                        "type": "integer",
                        "minimum": 1024,
                        "maximum": 65535,
                        "description": "Local HTTP port. Auto-assigned from 18000-18999 if omitted",
                    },
                    "database_port": {
                        "type": "integer",
                        "minimum": 1024,
                        "maximum": 65535,
                        "description": "Local database proxy port. Auto-assigned from 19000-19999 if omitted",
                    },
                    "document_root": {
                        "type": "string",
                        "description": "Document root relative to the project. Auto-detected if omitted",
                    },
                    "web": {
                        "type": "string",
                        "enum": ["nginx", "apache"],
                        "description": "Web server. Uses the configured default if omitted",
                    },
                    "php": {
                        "type": "string",
                        "enum": ["8.0", "8.1", "8.2", "8.3", "8.4", "8.5"],
                        "description": "PHP version for CLI and FPM",
                    },
                    "node": {
                        "type": "string",
                        "enum": ["20", "22", "24"],
                        "description": "Node.js major version for project commands",
                    },
                    "composer": {
                        "type": "string",
                        "enum": ["2"],
                        "description": "Composer major version",
                    },
                    "database": {
                        "type": "string",
                        "enum": ["none", "mariadb", "mysql", "postgres", "sqlite"],
                        "description": "Database engine",
                    },
                    "cache": {
                        "type": "string",
                        "enum": ["none", "redis"],
                        "description": "Cache service",
                    },
                    "supervisor": {
                        "type": "boolean",
                        "description": "Run the Laravel/Symfony worker preset or supervisor_command",
                    },
                    "supervisor_command": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Worker executable and literal arguments. @php uses project PHP",
                    },
                    "supervisor_args": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Extra literal arguments for supervisor_command",
                    },
                    "cron": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Scheduled tasks as strings like '*/5 * * * * @php artisan schedule:run'",
                    },
                    "cron_preset": {
                        "type": "boolean",
                        "description": "Use the app preset when cron is empty",
                    },
                    "mail": {
                        "type": "boolean",
                        "description": "Capture test email with Mailpit",
                    },
                    "autostart": {
                        "type": "boolean",
                        "description": "Start the project when LocalWeb starts",
                    },
                    "sync_hosts": {
                        "type": "boolean",
                        "description": "Sync domains after adding. Default: true",
                    },
                },
                "required": ["path"],
            },
        },
        {
            "name": "localweb_project_remove",
            "description": "Stop and remove a project from LocalWeb, then sync domains. Keep source files and databases.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": name_property,
                    "sync_hosts": {
                        "type": "boolean",
                        "description": "Sync domains after removal. Default: true",
                    },
                },
                "required": ["name"],
            },
        },
        {
            "name": "localweb_project_start",
            "description": "Start the project's services. Return its local HTTP and available HTTPS URLs.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": name_property },
                "required": ["name"],
            },
        },
        {
            "name": "localweb_project_stop",
            "description": "Stop the project. Keep its settings and data.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": name_property },
                "required": ["name"],
            },
        },
        {
            "name": "localweb_project_restart",
            "description": "Restart the project.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": name_property },
                "required": ["name"],
            },
        },
        {
            "name": "localweb_project_environment",
            "description": "Return database, cache and email settings for the app.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": name_property },
                "required": ["name"],
            },
        },
        {
            "name": "localweb_project_exec",
            "description": "Run a project command and return its exit code and output. Use @php, @composer, @node, @npm or @npx for the selected runtime, e.g. [\"@php\", \"artisan\", \"test\"].",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": name_property,
                    "command": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Executable followed by literal arguments; @-tools resolve to the project runtime",
                    },
                    "timeout_seconds": {
                        "type": "integer",
                        "description": "Time limit in seconds. Default: 300; maximum: 3600",
                    },
                },
                "required": ["name", "command"],
            },
        },
        {
            "name": "localweb_ca_status",
            "description": "Check HTTPS certificate files and trust status.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_ca_install",
            "description": "Trust LocalWeb's HTTPS certificate. Requires administrator approval.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_hosts_sync",
            "description": "Sync project domains in the system hosts file. Requires administrator approval.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_login_autostart",
            "description": "Turn LocalWeb login autostart on or off.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "enable": {
                        "type": "boolean",
                        "description": "true enables login autostart, false disables it",
                    },
                },
                "required": ["enable"],
            },
        },
        {
            "name": "localweb_logs",
            "description": "List log files, or read the last lines of one file.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file": {
                        "type": "string",
                        "description": "Log filename, without a folder path",
                    },
                    "lines": {
                        "type": "integer",
                        "description": "Lines from the end. Default: 200; maximum: 2000",
                    },
                },
            },
        },
    ])
}

/// Dispatches one tool call; the error string becomes MCP tool content.
pub(crate) async fn call(name: &str, arguments: &Value) -> Result<String, String> {
    match name {
        "localweb_status" => status().await,
        "localweb_daemon_start" => daemon_start().await,
        "localweb_daemon_shutdown" => daemon_shutdown().await,
        "localweb_doctor" => doctor().await,
        "localweb_runtime_list" => runtime_list().await,
        "localweb_projects_list" => projects_list().await,
        "localweb_project_info" => project_info(arguments).await,
        "localweb_project_add" => project_add(arguments).await,
        "localweb_project_remove" => project_remove(arguments).await,
        "localweb_project_start" => project_action(arguments, "start").await,
        "localweb_project_stop" => project_action(arguments, "stop").await,
        "localweb_project_restart" => project_action(arguments, "restart").await,
        "localweb_project_environment" => project_environment(arguments).await,
        "localweb_project_exec" => project_exec(arguments).await,
        "localweb_ca_status" => ca_status().await,
        "localweb_ca_install" => ca_install().await,
        "localweb_hosts_sync" => hosts_sync().await,
        "localweb_login_autostart" => login_autostart(arguments).await,
        "localweb_logs" => logs(arguments).await,
        other => Err(format!("unknown tool: {other}")),
    }
}

/// Resolves the per-user directories used by every tool.
fn paths() -> Result<Paths, String> {
    Paths::discover().map_err(|error| format!("cannot resolve LocalWeb directories: {error}"))
}

/// Sends one request with a timeout and unwraps daemon-reported errors.
async fn daemon(request: Request) -> Result<Response, String> {
    let paths = paths()?;
    converse(&paths, request).await
}

/// Sends one request over an already resolved [`Paths`] value.
async fn converse(paths: &Paths, request: Request) -> Result<Response, String> {
    let response = timeout(DAEMON_TIMEOUT, send_request(paths, request))
        .await
        .map_err(|_elapsed| "LocalWeb did not respond in time. The operation may still complete. Check localweb_status and the project before retrying.".to_owned())?
        .map_err(|error| format!("Can't reach LocalWeb ({error}). Check localweb_status before restarting."))?;
    if let Response::Error { code, message } = &response {
        return Err(format!("{code}: {message}"));
    }
    Ok(response)
}

/// Serializes one daemon payload as pretty JSON for the model.
fn render<T: Serialize>(value: &T) -> Result<String, String> {
    serde_json::to_string_pretty(value).map_err(|error| error.to_string())
}

/// Fails when a daemon reply is not the expected variant.
fn unexpected() -> String {
    copy::UNEXPECTED_RESPONSE.to_owned()
}

/// Reports daemon and gateway health; stays useful while the daemon is down.
async fn status() -> Result<String, String> {
    let paths = paths()?;
    match timeout(DAEMON_TIMEOUT, send_request(&paths, Request::Status)).await {
        Ok(Ok(Response::Status(status))) => render(&status),
        Ok(Err(localweb_core::LocalWebError::Io(error)))
            if matches!(error.kind(), std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused) => render(&json!({
                "daemon_running": false,
                "error": error.to_string(),
                "hint": "Call localweb_daemon_start to start LocalWeb.",
            })),
        Ok(Err(error)) => Err(format!("LocalWeb state is unknown: {error}. Check again before restarting.")),
        Ok(Ok(_)) => Err(unexpected()),
        Err(_) => Err("LocalWeb state is unknown: the status request timed out. Check again before restarting.".into()),
    }
}

/// Starts the user daemon and waits until restoration finished.
async fn daemon_start() -> Result<String, String> {
    run_cli(&["start"]).await?;
    let paths = paths()?;
    let mut last = String::new();
    let deadline = tokio::time::Instant::now() + STARTUP_READY_TIMEOUT;
    loop {
        match converse(&paths, Request::Status).await {
            Ok(Response::Status(status)) if status.ready => return render(&status),
            Ok(Response::Status(_)) => {
                "LocalWeb is still restoring projects".clone_into(&mut last);
            }
            Ok(_) => last = unexpected(),
            Err(reason) => last = reason,
        }
        if tokio::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(format!(
        "LocalWeb did not become ready after startup: {last}"
    ))
}

/// Stops the daemon and every managed service.
async fn daemon_shutdown() -> Result<String, String> {
    match daemon(Request::Shutdown).await? {
        Response::Message(message) => Ok(message),
        _ => Err(unexpected()),
    }
}

/// Runs environment diagnostics.
async fn doctor() -> Result<String, String> {
    match daemon(Request::Doctor).await? {
        Response::Diagnostics(items) => render(&items),
        _ => Err(unexpected()),
    }
}

/// Lists bundled runtime components.
async fn runtime_list() -> Result<String, String> {
    match daemon(Request::RuntimeList).await? {
        Response::Diagnostics(items) => render(&items),
        _ => Err(unexpected()),
    }
}

/// Lists every project with live state.
async fn projects_list() -> Result<String, String> {
    match daemon(Request::ProjectList).await? {
        Response::Projects(items) => render(&items),
        _ => Err(unexpected()),
    }
}

/// Returns one project with live state.
async fn project_info(arguments: &Value) -> Result<String, String> {
    let name = required_string(arguments, "name")?;
    match daemon(Request::ProjectGet { name }).await? {
        Response::Project(item) => render(&item),
        _ => Err(unexpected()),
    }
}

/// Validates, creates, and optionally synchronizes hosts for one project.
async fn project_add(arguments: &Value) -> Result<String, String> {
    let path = PathBuf::from(required_string(arguments, "path")?);
    let name = optional_string(arguments, "name");
    let hostname = optional_string(arguments, "hostname");
    let document_root = optional_string(arguments, "document_root").map(PathBuf::from);
    let web_server = match optional_string(arguments, "web").as_deref() {
        None => None,
        Some("nginx") => Some(WebServer::Nginx),
        Some("apache") => Some(WebServer::Apache),
        Some(other) => {
            return Err(format!(
                "`web` must be \"nginx\" or \"apache\", got \"{other}\""
            ));
        }
    };
    let php = match optional_string(arguments, "php") {
        None => None,
        Some(version) => {
            validate_php_version(&version).map_err(|error| error.to_string())?;
            Some(version)
        }
    };
    let node = match optional_string(arguments, "node").as_deref() {
        None => None,
        Some(version @ ("20" | "22" | "24")) => Some(version.to_owned()),
        Some(other) => {
            return Err(format!(
                "`node` must be \"20\", \"22\" or \"24\", got \"{other}\""
            ));
        }
    };
    let composer = match optional_string(arguments, "composer").as_deref() {
        None => None,
        Some("2") => Some("2".to_owned()),
        Some(other) => return Err(format!("`composer` must be \"2\", got \"{other}\"")),
    };
    let database = match optional_string(arguments, "database").as_deref() {
        None => None,
        Some("none") => Some(Database::None),
        Some("mariadb") => Some(Database::Mariadb),
        Some("mysql") => Some(Database::Mysql),
        Some("postgres") => Some(Database::Postgres),
        Some("sqlite") => Some(Database::Sqlite),
        Some(other) => return Err(format!("unknown database engine \"{other}\"")),
    };
    let cache = match optional_string(arguments, "cache").as_deref() {
        None => None,
        Some("none") => Some(Cache::None),
        Some("redis") => Some(Cache::Redis),
        Some(other) => return Err(format!("unknown cache engine \"{other}\"")),
    };
    let supervisor_command = string_array(arguments, "supervisor_command")?;
    let mut command = supervisor_command;
    command.extend(string_array(arguments, "supervisor_args")?);
    let supervisor_enabled = optional_bool(arguments, "supervisor", false) || !command.is_empty();
    let mut cron = Vec::new();
    for line in string_array(arguments, "cron")? {
        cron.push(parse_cron_line(&line).map_err(|error| error.to_string())?);
    }
    let web_port = optional_port(arguments, "web_port")?;
    let database_port = optional_port(arguments, "database_port")?;
    let project = NewProject {
        path,
        name,
        hostname,
        document_root,
        profile: None,
        web_server,
        php,
        composer,
        node,
        database,
        cache,
        supervisor: SupervisorConfig {
            enabled: supervisor_enabled,
            command,
        },
        cron,
        cron_preset: optional_bool(arguments, "cron_preset", false),
        clear_cron: false,
        mail: optional_bool(arguments, "mail", false),
        web_port,
        database_port,
        autostart: optional_bool(arguments, "autostart", false),
    };
    let added = daemon(Request::ProjectAdd(project)).await?;
    let mut output = match added {
        Response::Project(item) => json!({ "project": item }),
        Response::Message(message) => json!({ "result": message }),
        _ => return Err(unexpected()),
    };
    if optional_bool(arguments, "sync_hosts", true) {
        append_hosts_outcome(&mut output).await;
    }
    render(&output)
}

/// Removes a project and optionally synchronizes hosts.
async fn project_remove(arguments: &Value) -> Result<String, String> {
    let name = required_string(arguments, "name")?;
    let Response::Message(message) = daemon(Request::ProjectRemove { name }).await? else {
        return Err(unexpected());
    };
    let mut output = json!({ "result": message });
    if optional_bool(arguments, "sync_hosts", true) {
        append_hosts_outcome(&mut output).await;
    }
    render(&output)
}

/// Starts, stops, or restarts one project.
async fn project_action(arguments: &Value, action: &str) -> Result<String, String> {
    let name = required_string(arguments, "name")?;
    let request = match action {
        "start" => Request::ProjectStart { name },
        "stop" => Request::ProjectStop { name },
        _ => Request::ProjectRestart { name },
    };
    let response = daemon(request).await.map_err(|error| {
        if action == "start" && error.contains("hosts") {
            format!("{error}; call localweb_hosts_sync to sync local domains")
        } else {
            error
        }
    })?;
    match response {
        Response::Message(message) => Ok(message),
        _ => Err(unexpected()),
    }
}

/// Returns the exported environment of one project.
async fn project_environment(arguments: &Value) -> Result<String, String> {
    let name = required_string(arguments, "name")?;
    match daemon(Request::ProjectEnvironment { name }).await? {
        Response::Environment(values) => render(&values),
        _ => Err(unexpected()),
    }
}

/// Runs one command inside the managed project runtime.
async fn project_exec(arguments: &Value) -> Result<String, String> {
    let name = required_string(arguments, "name")?;
    let command = string_array(arguments, "command")?;
    let Some((tool, extra)) = command.split_first() else {
        return Err("`command` must contain at least the executable".to_owned());
    };
    let seconds = arguments
        .get("timeout_seconds")
        .and_then(Value::as_u64)
        .unwrap_or(EXEC_TIMEOUT_DEFAULT)
        .clamp(1, EXEC_TIMEOUT_MAX);
    let spec = match daemon(Request::ResolveExecution { name: name.clone() }).await? {
        Response::Execution(spec) => spec,
        _ => Err(unexpected())?,
    };
    let (program, prefix) = resolve_program(&spec, tool)?;
    let mut process = Command::new(&program);
    spec.configure_command(process.as_std_mut(), &name)
        .map_err(|error| error.to_string())?;
    process.args(prefix).args(extra);
    let output = timeout(
        Duration::from_secs(seconds),
        localweb_core::process::output(&mut process, OUTPUT_CAP),
    )
    .await
    .map_err(|_elapsed| format!("the command timed out after {seconds}s"))?
    .map_err(|error| format!("cannot run {}: {error}", program.display()))?;
    render(&json!({
        "exit_code": output.status.code(),
        "stdout": cap_output(&output.stdout),
        "stderr": cap_output(&output.stderr),
    }))
}

/// Maps a tool name onto the managed binary, returning the argv prefix.
fn resolve_program(spec: &ExecutionSpec, tool: &str) -> Result<(PathBuf, Vec<OsString>), String> {
    match tool {
        "@php" => Ok((spec.php.clone(), php_prefix(spec))),
        "@composer" => {
            let phar = spec
                .composer_phar
                .clone()
                .ok_or("Composer 2 is not installed in the LocalWeb runtime")?;
            let mut prefix = php_prefix(spec);
            prefix.push(phar.into());
            Ok((spec.php.clone(), prefix))
        }
        "@node" => Ok((node_binary(spec)?, Vec::new())),
        "@npm" => {
            let npm = spec
                .npm_cli
                .clone()
                .ok_or("npm is not available for this project")?;
            Ok((node_binary(spec)?, vec![npm.into()]))
        }
        "@npx" => {
            let npm = spec
                .npm_cli
                .clone()
                .ok_or("npm is not available for this project")?;
            let npx = npm.with_file_name("npx-cli.js");
            if !npx.is_file() {
                return Err("npx is not available for this project".to_owned());
            }
            Ok((node_binary(spec)?, vec![npx.into()]))
        }
        program => Ok((PathBuf::from(program), Vec::new())),
    }
}

/// PHP argv prefix that loads the project php.ini when one exists.
fn php_prefix(spec: &ExecutionSpec) -> Vec<OsString> {
    match &spec.php_ini {
        Some(ini) => vec!["-n".into(), "-c".into(), ini.clone().into()],
        None => Vec::new(),
    }
}

/// Node.js binary of the project's managed runtime.
fn node_binary(spec: &ExecutionSpec) -> Result<PathBuf, String> {
    spec.node
        .clone()
        .ok_or_else(|| "Node.js is not available for this project".to_owned())
}

/// Converts process output to text, truncating on a character boundary.
fn cap_output(bytes: &[u8]) -> String {
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    if text.len() > OUTPUT_CAP {
        let mut end = OUTPUT_CAP;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
        text.push_str("\n… (output truncated)");
    }
    text
}

/// Reports certificate authority presence and trust.
async fn ca_status() -> Result<String, String> {
    match daemon(Request::CaStatus).await? {
        Response::Ca(status) => render(&status),
        _ => Err(unexpected()),
    }
}

/// Ensures the CA exists and installs it into the system trust store.
async fn ca_install() -> Result<String, String> {
    run_cli(&["ca", "install"]).await?;
    Ok(copy::HTTPS_TRUSTED.to_owned())
}

/// Rewrites the managed hosts block with every project hostname.
async fn hosts_sync() -> Result<String, String> {
    let paths = paths()?;
    // Serialize with CLI and other MCP processes before reading the project
    // snapshot and invoking the privileged helper.
    let _lock = timeout(Duration::from_secs(300), async {
        loop {
            if let Some(lock) = HostsSyncLock::try_acquire(&paths)? {
                break localweb_core::Result::Ok(lock);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .map_err(|_elapsed| {
        "Another hosts update is still running. Refresh before retrying.".to_owned()
    })?
    .map_err(|error| error.to_string())?;
    let hostnames = project_hostnames(&paths).await?;
    let Response::Status(status) = converse(&paths, Request::Status).await? else {
        return Err(unexpected());
    };
    let address = status
        .gateway_bind_address
        .unwrap_or(localweb_core::hosts::DEFAULT_GATEWAY_ADDRESS);
    let request = HostsSync {
        address,
        domains: hostnames,
    };
    let payload = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
    run_helper("sync-hosts", payload).await?;
    Ok(copy::domains_synced(request.domains.len(), address))
}

/// Adds the hosts synchronization outcome to a project mutation result.
async fn append_hosts_outcome(output: &mut Value) {
    match hosts_sync().await {
        Ok(synced) => {
            output["hosts"] = json!(synced);
        }
        Err(error) => {
            output["hosts"] = json!(format!("not synchronized: {error}"));
            output["hint"] =
                json!("Call localweb_hosts_sync when administrator approval is available.");
        }
    }
}

/// Collects the hostname of every configured project.
async fn project_hostnames(paths: &Paths) -> Result<Vec<String>, String> {
    let Response::Projects(items) = converse(paths, Request::ProjectList).await? else {
        return Err(unexpected());
    };
    Ok(items
        .into_iter()
        .map(|item| item.project.hostname)
        .collect())
}

/// Enables or disables daemon autostart at login.
async fn login_autostart(arguments: &Value) -> Result<String, String> {
    let enable = arguments
        .get("enable")
        .and_then(Value::as_bool)
        .ok_or("missing required boolean argument `enable`")?;
    let action = if enable { "enable" } else { "disable" };
    run_cli(&["autostart", action]).await?;
    Ok(format!("LocalWeb login autostart {action}d"))
}

/// Lists log files or returns the tail of one of them.
async fn logs(arguments: &Value) -> Result<String, String> {
    let paths = paths()?;
    let directory = paths.state.join("logs");
    let Some(file) = optional_string(arguments, "file") else {
        let mut entries = tokio::fs::read_dir(&directory)
            .await
            .map_err(|error| format!("cannot list {}: {error}", directory.display()))?;
        let mut files = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|error| format!("cannot list {}: {error}", directory.display()))?
        {
            let metadata = entry
                .metadata()
                .await
                .map_err(|error| format!("cannot inspect a log file: {error}"))?;
            if metadata.is_file() {
                files.push((
                    entry.file_name().to_string_lossy().into_owned(),
                    metadata.len(),
                ));
            }
        }
        files.sort_by(|left, right| left.0.cmp(&right.0));
        let listing: Vec<Value> = files
            .into_iter()
            .map(|(file, bytes)| json!({ "file": file, "bytes": bytes }))
            .collect();
        return render(&listing);
    };
    let name = Path::new(&file)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| *name == file)
        .ok_or("`file` must be a bare log file name")?
        .to_owned();
    let content = tokio::fs::read_to_string(directory.join(&name))
        .await
        .map_err(|error| format!("cannot read log {name}: {error}"))?;
    let lines = arguments
        .get("lines")
        .and_then(Value::as_u64)
        .unwrap_or(200)
        .clamp(1, LOG_LINES_MAX);
    let lines = usize::try_from(lines).map_err(|_overflow| "invalid `lines`".to_owned())?;
    let total = content.lines().count();
    let tail: Vec<&str> = content.lines().rev().take(lines).collect();
    let tail: Vec<&str> = tail.into_iter().rev().collect();
    Ok(format!(
        "{name} (last {} of {total} lines)\n{}",
        tail.len(),
        tail.join("\n")
    ))
}

/// Runs one privileged helper operation through pkexec.
async fn run_helper(operation: &str, input: Vec<u8>) -> Result<(), String> {
    let helper = helper_path()?;
    localweb_core::privilege::run_helper_async(&helper, operation, &input)
        .await
        .map_err(|error| error.to_string())
}

/// Runs a sibling CLI action with owned cancellation and a bounded prompt.
async fn run_cli(arguments: &[&str]) -> Result<(), String> {
    let binary = std::env::current_exe()
        .map_err(|error| error.to_string())?
        .with_file_name(format!("localweb{}", std::env::consts::EXE_SUFFIX));
    let mut command = Command::new(binary);
    command.args(arguments);
    let output = timeout(
        Duration::from_secs(300),
        localweb_core::process::output(&mut command, OUTPUT_CAP),
    )
    .await
    .map_err(|_elapsed| {
        "The action timed out. Refresh the current state before retrying.".to_owned()
    })?
    .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok(())
}

/// Finds the installed helper, falling back to a development build.
fn helper_path() -> Result<PathBuf, String> {
    let name = if cfg!(windows) {
        "localweb-helper.exe"
    } else {
        "localweb-helper"
    };
    #[cfg(not(windows))]
    {
        let installed = PathBuf::from("/usr/lib/localweb/localweb-helper");
        if installed.is_file() {
            return Ok(installed);
        }
    }
    let development = std::env::current_exe()
        .map_err(|error| format!("cannot locate the current binary: {error}"))?
        .with_file_name(name);
    if development.is_file() {
        return Ok(development);
    }
    Err("localweb-helper is missing; install the LocalWeb package".to_owned())
}

/// Reads one required string argument.
fn required_string(arguments: &Value, key: &str) -> Result<String, String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("missing required string argument `{key}`"))
}

/// Reads one optional string argument.
fn optional_string(arguments: &Value, key: &str) -> Option<String> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Reads one optional TCP port argument in the unprivileged range.
fn optional_port(arguments: &Value, key: &str) -> Result<Option<u16>, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let port = value
                .as_u64()
                .filter(|port| (1024..=65535).contains(port))
                .and_then(|port| u16::try_from(port).ok())
                .ok_or_else(|| format!("`{key}` must be a port between 1024 and 65535"))?;
            Ok(Some(port))
        }
    }
}

/// Reads one optional boolean argument with a fallback.
fn optional_bool(arguments: &Value, key: &str, fallback: bool) -> bool {
    arguments
        .get(key)
        .and_then(Value::as_bool)
        .unwrap_or(fallback)
}

/// Reads an optional array of strings, rejecting other shapes.
fn string_array(arguments: &Value, key: &str) -> Result<Vec<String>, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("`{key}` must be an array of strings"))
            })
            .collect(),
        Some(_) => Err(format!("`{key}` must be an array of strings")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ExecutionSpec {
        ExecutionSpec {
            project_path: PathBuf::from("/tmp/example"),
            php: PathBuf::from("/opt/php"),
            composer_phar: Some(PathBuf::from("/opt/composer.phar")),
            node: Some(PathBuf::from("/opt/node")),
            npm_cli: Some(PathBuf::from("/opt/npm/bin/npm-cli.js")),
            shim_dir: PathBuf::from("/tmp/shims"),
            library_path: None,
            php_ini: Some(PathBuf::from("/tmp/php.ini")),
            environment: std::collections::BTreeMap::new(),
        }
    }

    #[test]
    fn definitions_cover_every_documented_tool() {
        let tools = definitions();
        let tools = tools.as_array().expect("definitions is an array");
        assert_eq!(tools.len(), 19);
        for tool in tools {
            assert!(
                tool["name"]
                    .as_str()
                    .is_some_and(|n| n.starts_with("localweb_"))
            );
            assert!(
                tool["description"]
                    .as_str()
                    .is_some_and(|d| !d.trim().is_empty())
            );
            assert_eq!(tool["inputSchema"]["type"], "object");
        }
    }

    #[test]
    fn composer_resolves_to_php_with_the_shared_phar() {
        let (program, prefix) = resolve_program(&spec(), "@composer").expect("composer resolves");
        assert_eq!(program, PathBuf::from("/opt/php"));
        assert_eq!(
            prefix,
            [
                OsString::from("-n"),
                OsString::from("-c"),
                OsString::from("/tmp/php.ini"),
                OsString::from("/opt/composer.phar"),
            ]
        );
    }

    #[test]
    fn npx_resolves_next_to_npm() {
        let mut spec = spec();
        spec.npm_cli = Some(PathBuf::from("/tmp/npm-cli.js"));
        let error = resolve_program(&spec, "@npx").expect_err("missing npx is rejected");
        assert!(error.contains("npx"));
    }

    #[test]
    fn arbitrary_programs_pass_through() {
        let (program, prefix) = resolve_program(&spec(), "git").expect("plain programs resolve");
        assert_eq!(program, PathBuf::from("git"));
        assert_eq!(prefix, Vec::<std::ffi::OsString>::new());
    }

    #[test]
    fn output_truncation_stays_on_character_boundaries() {
        let mut bytes = "è".repeat(OUTPUT_CAP).into_bytes();
        bytes.extend_from_slice(b"tail");
        let text = cap_output(&bytes);
        assert!(text.ends_with("(output truncated)"));
        assert!(text.len() <= OUTPUT_CAP + "\n… (output truncated)".len());
    }

    #[test]
    fn argument_helpers_validate_shapes() {
        let arguments = json!({ "name": "demo", "flag": true, "items": ["a", "b"] });
        assert_eq!(required_string(&arguments, "name").unwrap(), "demo");
        let _ = required_string(&arguments, "missing").unwrap_err();
        assert!(optional_bool(&arguments, "flag", false));
        assert_eq!(string_array(&arguments, "items").unwrap(), ["a", "b"]);
        let _ = string_array(&json!({ "items": [1] }), "items").unwrap_err();
        assert_eq!(
            string_array(&json!({}), "items").unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(
            optional_port(&json!({ "port": 18_000 }), "port").unwrap(),
            Some(18_000)
        );
        assert_eq!(optional_port(&json!({}), "port").unwrap(), None);
        let _ = optional_port(&json!({ "port": 80 }), "port").unwrap_err();
        let _ = optional_port(&json!({ "port": "18000" }), "port").unwrap_err();
    }
}

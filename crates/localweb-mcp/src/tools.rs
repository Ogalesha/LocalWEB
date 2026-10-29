//! Tool catalogue for the MCP server.
//!
//! [`definitions`] advertises every tool with a JSON schema; [`call`]
//! dispatches a tool invocation to the daemon protocol or to the privileged
//! helper, mirroring what the desktop application and the `localweb` CLI do.

use localweb_core::{
    Cache, Database, ExecutionSpec, NewProject, Paths, Request, Response, SupervisorConfig,
    WebServer, certificate::CaPaths, hosts::HostsSync, parse_cron_line, send_request,
    validate_php_version,
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{io::AsyncReadExt, process::Command, time::timeout};

/// How long one daemon request may take before the tool reports a timeout.
const DAEMON_TIMEOUT: Duration = Duration::from_secs(120);
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
            "description": "Reports whether the LocalWeb daemon and HTTPS gateway are running, plus configured and active project counts.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_daemon_start",
            "description": "Starts or restarts the LocalWeb user daemon and waits until it answers. Call this when other tools report that the daemon is unreachable.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_daemon_shutdown",
            "description": "Stops the LocalWeb daemon and every service it manages.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_doctor",
            "description": "Runs LocalWeb environment diagnostics and returns findings with suggested fixes.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_runtime_list",
            "description": "Lists the bundled runtime components (PHP, Node.js, web servers, databases, cache, Mailpit) and whether each is available.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_projects_list",
            "description": "Lists every configured project with hostname, path, configuration, and live running state.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_project_info",
            "description": "Returns one project with its full configuration and live state.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": name_property },
                "required": ["name"],
            },
        },
        {
            "name": "localweb_project_add",
            "description": "Registers an existing folder as a LocalWeb project. Unset options use detected or global defaults. Every project receives a fixed direct HTTP port and, for network databases, a fixed loopback database port; HTTPS hostnames work when the gateway can bind port 443. After creation the system hosts file is synchronized so the hostname resolves.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path of the project folder to adopt",
                    },
                    "name": {
                        "type": "string",
                        "description": "Project name; defaults to the folder name",
                    },
                    "hostname": {
                        "type": "string",
                        "description": "Hostname served over HTTPS when the gateway is available; defaults to <name>.<domain suffix>",
                    },
                    "web_port": {
                        "type": "integer",
                        "minimum": 1024,
                        "maximum": 65535,
                        "description": "Fixed loopback HTTP port; a free port from 18000-18999 is assigned when omitted",
                    },
                    "database_port": {
                        "type": "integer",
                        "minimum": 1024,
                        "maximum": 65535,
                        "description": "Fixed loopback database port forwarded to the shared database engine; a free port from 19000-19999 is assigned when omitted",
                    },
                    "document_root": {
                        "type": "string",
                        "description": "Document root relative to the project; defaults to the detected public folder",
                    },
                    "web": {
                        "type": "string",
                        "enum": ["nginx", "apache"],
                        "description": "Web server; defaults to the global default",
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
                        "description": "Database engine to provision",
                    },
                    "cache": {
                        "type": "string",
                        "enum": ["none", "redis"],
                        "description": "In-memory cache to provision",
                    },
                    "supervisor": {
                        "type": "boolean",
                        "description": "Enable the framework worker preset (Laravel/Symfony) or the command given in supervisor_command",
                    },
                    "supervisor_command": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Worker executable followed by literal arguments; use @php for the project PHP CLI",
                    },
                    "supervisor_args": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Extra literal worker arguments appended to supervisor_command",
                    },
                    "cron": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Scheduled tasks as strings like '*/5 * * * * @php artisan schedule:run'",
                    },
                    "cron_preset": {
                        "type": "boolean",
                        "description": "Add the framework conventional schedule alongside explicit cron entries",
                    },
                    "mail": {
                        "type": "boolean",
                        "description": "Capture project email with a private Mailpit instance",
                    },
                    "autostart": {
                        "type": "boolean",
                        "description": "Start the project when the daemon starts on login",
                    },
                    "sync_hosts": {
                        "type": "boolean",
                        "description": "Synchronize the system hosts file after adding; defaults to true",
                    },
                },
                "required": ["path"],
            },
        },
        {
            "name": "localweb_project_remove",
            "description": "Deletes a project and its managed services, then synchronizes the system hosts file.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": name_property,
                    "sync_hosts": {
                        "type": "boolean",
                        "description": "Synchronize the system hosts file after removal; defaults to true",
                    },
                },
                "required": ["name"],
            },
        },
        {
            "name": "localweb_project_start",
            "description": "Starts the project services (web server, PHP-FPM, database proxy, cache, worker, mail) and makes the hostname answer over HTTPS when the gateway is available.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": name_property },
                "required": ["name"],
            },
        },
        {
            "name": "localweb_project_stop",
            "description": "Stops the project services while keeping the configuration.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": name_property },
                "required": ["name"],
            },
        },
        {
            "name": "localweb_project_restart",
            "description": "Restarts the project services.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": name_property },
                "required": ["name"],
            },
        },
        {
            "name": "localweb_project_environment",
            "description": "Returns the environment variables LocalWeb exports to commands run inside the project.",
            "inputSchema": {
                "type": "object",
                "properties": { "name": name_property },
                "required": ["name"],
            },
        },
        {
            "name": "localweb_project_exec",
            "description": "Runs a command inside the project's managed runtime and returns its exit code, stdout, and stderr. Use @php, @composer, @node, @npm, or @npx to run tools through the exact binaries the project serves with, for example [\"@composer\", \"install\"] or [\"@php\", \"artisan\", \"test\"].",
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
                        "description": "Wall-clock limit in seconds; defaults to 300, maximum 3600",
                    },
                },
                "required": ["name", "command"],
            },
        },
        {
            "name": "localweb_ca_status",
            "description": "Reports whether the LocalWeb development certificate authority exists and is trusted by the system.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_ca_install",
            "description": "Creates the development CA if needed and installs it into the system trust store. Requires an interactive administrator authentication.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_hosts_sync",
            "description": "Rewrites the managed block of the system hosts file with the exact hostnames of every project. Requires an interactive administrator authentication.",
            "inputSchema": { "type": "object", "properties": {} },
        },
        {
            "name": "localweb_login_autostart",
            "description": "Enables or disables starting the LocalWeb daemon automatically at login.",
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
            "description": "Lists the daemon log files, or returns the tail of one log file such as nginx-error.log, php-8.5.log, or <project>-cron.log.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file": {
                        "type": "string",
                        "description": "Bare log file name inside the LocalWeb log directory",
                    },
                    "lines": {
                        "type": "integer",
                        "description": "Number of trailing lines to return; defaults to 200, maximum 2000",
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
        .map_err(|_elapsed| "the LocalWeb daemon did not answer in time".to_owned())?
        .map_err(|error| {
            format!("cannot reach the LocalWeb daemon ({error}); call localweb_daemon_start first")
        })?;
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
    "the daemon returned an unexpected response".to_owned()
}

/// Reports daemon and gateway health; stays useful while the daemon is down.
async fn status() -> Result<String, String> {
    match daemon(Request::Status).await {
        Ok(Response::Status(status)) => render(&status),
        Ok(_) => Err(unexpected()),
        Err(reason) => render(&json!({
            "daemon_running": false,
            "error": reason,
            "hint": "call localweb_daemon_start to start the daemon",
        })),
    }
}

/// Starts the user daemon and waits until it answers.
async fn daemon_start() -> Result<String, String> {
    tokio::task::spawn_blocking(localweb_core::service::restart_daemon)
        .await
        .map_err(|error| format!("cannot start the daemon: {error}"))?
        .map_err(|error| format!("cannot start the daemon: {error}"))?;
    let paths = paths()?;
    let mut last = String::new();
    for _ in 0..20 {
        match converse(&paths, Request::Status).await {
            Ok(Response::Status(status)) => return render(&status),
            Ok(_) => last = unexpected(),
            Err(reason) => last = reason,
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(format!(
        "the daemon was started but does not answer yet ({last})"
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
            format!("{error}; call localweb_hosts_sync to synchronize the hostname")
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
    let mut path = OsString::from(spec.shim_dir.as_os_str());
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
    process
        .args(prefix)
        .args(extra)
        .current_dir(&spec.project_path)
        .env("PATH", path)
        .env("LOCALWEB_PROJECT", &name)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(library_path) = &spec.library_path {
        process.env("LD_LIBRARY_PATH", library_path);
    }
    if let Some(php_ini) = &spec.php_ini {
        process.env("PHPRC", php_ini);
    }
    let mut child = process
        .spawn()
        .map_err(|error| format!("cannot launch {}: {error}", program.display()))?;
    let mut stdout = child.stdout.take().ok_or("stdout is unavailable")?;
    let mut stderr = child.stderr.take().ok_or("stderr is unavailable")?;
    let collect = async {
        let (out, err, state) = tokio::join!(
            read_stream(&mut stdout),
            read_stream(&mut stderr),
            child.wait(),
        );
        (out, err, state)
    };
    let (out, err, state) = timeout(Duration::from_secs(seconds), collect)
        .await
        .map_err(|_elapsed| format!("the command timed out after {seconds}s"))?;
    let state = state.map_err(|error| format!("cannot wait for the command: {error}"))?;
    render(&json!({
        "exit_code": state.code(),
        "stdout": cap_output(&out),
        "stderr": cap_output(&err),
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

/// Reads one child stream to the end; unreadable streams count as empty.
async fn read_stream(stream: &mut (impl AsyncReadExt + Unpin)) -> Vec<u8> {
    let mut buffer = Vec::new();
    let _ = stream.read_to_end(&mut buffer).await;
    buffer
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
    let paths = paths()?;
    let ca = CaPaths::new(&paths);
    ca.ensure_ca().map_err(|error| error.to_string())?;
    let certificate = tokio::fs::read(&ca.cert)
        .await
        .map_err(|error| format!("cannot read the CA certificate: {error}"))?;
    run_helper("install-ca", certificate).await?;
    localweb_core::platform::install_browser_ca(&ca.cert).map_err(|error| error.to_string())?;
    Ok("LocalWeb Development CA installed for the system and Chromium-family browsers".to_owned())
}

/// Rewrites the managed hosts block with every project hostname.
async fn hosts_sync() -> Result<String, String> {
    let paths = paths()?;
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
    Ok(format!(
        "Synchronized {} LocalWeb hostnames to {address} in the system hosts file",
        request.domains.len()
    ))
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
                json!("call localweb_hosts_sync once administrator authentication is available");
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
    tokio::task::spawn_blocking(move || localweb_core::service::set_autostart(enable))
        .await
        .map_err(|error| format!("cannot update login autostart: {error}"))?
        .map_err(|error| format!("cannot update login autostart: {error}"))?;
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
    let operation = operation.to_owned();
    tokio::task::spawn_blocking(move || {
        localweb_core::privilege::run_helper(&helper, &operation, &input)
    })
    .await
    .map_err(|error| format!("cannot run the privileged helper: {error}"))?
    .map_err(|error| error.to_string())
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
            assert!(tool["description"].as_str().is_some_and(|d| d.len() > 20));
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

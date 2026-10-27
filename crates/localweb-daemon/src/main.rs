use anyhow::{Context, Result};
use localweb_core::{
    CaStatus, Diagnostic, Envelope, ExecutionSpec, GatewayRoute, GlobalConfig, LocalWebError,
    MAX_MESSAGE_BYTES, PROTOCOL_VERSION, Paths, Project, ProjectStatus, Request, Response, Status,
    WebServer, atomic_write, certificate::CaPaths, runtime::RuntimeLayout,
};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    net::{Ipv4Addr, TcpListener},
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    sync::{Mutex, Notify},
};
use tracing::{error, info, warn};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// How often the cron loop wakes up to run due scheduled tasks.
const CRON_TICK_INTERVAL: Duration = Duration::from_secs(30);
/// Standard HTTPS port used on a LocalWeb-owned loopback address.
const GATEWAY_HTTPS_PORT: u16 = 443;
/// First dedicated IPv4 loopback host considered on Linux.
const GATEWAY_LOOPBACK_START: u8 = 2;
/// Last dedicated IPv4 loopback host considered on Linux.
const GATEWAY_LOOPBACK_END: u8 = 254;
/// First unprivileged HTTPS fallback port. The selected port is loopback-only.
const GATEWAY_FALLBACK_START: u16 = 8_443;
/// Last unprivileged HTTPS fallback port considered by `LocalWeb`.
const GATEWAY_FALLBACK_END: u16 = 8_499;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
struct GatewayState {
    #[serde(default)]
    schema_version: u16,
    #[serde(default = "default_gateway_bind_address")]
    bind_address: Ipv4Addr,
    https_port: u16,
}

const fn default_gateway_bind_address() -> Ipv4Addr {
    Ipv4Addr::LOCALHOST
}

mod backend;
mod proxy;
mod services;
use proxy::DbProxy;
use services::{Services, is_active, php_unit};

struct State {
    paths: Paths,
    config: GlobalConfig,
    running: Mutex<BTreeSet<String>>,
    operations: Mutex<()>,
    db_proxies: Mutex<BTreeMap<String, DbProxy>>,
    cron_busy: Mutex<HashSet<(String, usize)>>,
    cron_last_minute: Mutex<BTreeMap<(String, usize), i64>>,
    shutdown: Notify,
}

#[tokio::main]
async fn main() -> Result<()> {
    use std::io::IsTerminal as _;
    tracing_subscriber::fmt()
        .with_ansi(std::io::stdout().is_terminal())
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("localwebd=info")),
        )
        .init();
    let paths = Paths::discover()?;
    paths.create()?;
    let config = GlobalConfig::load_or_create(&paths)?;
    CaPaths::new(&paths).ensure_ca()?;
    ensure_project_ports(&paths)?;
    initialize_gateway_endpoint(&paths)?;
    let mut listener = localweb_core::transport::DaemonListener::bind(&paths)
        .await
        .context("bind daemon endpoint")?;
    let mut restore = load_desired_running(&paths)?;
    restore.extend(
        load_projects(&paths)?
            .into_iter()
            .filter(|project| project.autostart && project.enabled)
            .map(|project| project.name),
    );
    let state = Arc::new(State {
        paths,
        config,
        running: Mutex::new(BTreeSet::new()),
        operations: Mutex::new(()),
        db_proxies: Mutex::new(BTreeMap::new()),
        cron_busy: Mutex::new(HashSet::new()),
        cron_last_minute: Mutex::new(BTreeMap::new()),
        shutdown: Notify::new(),
    });
    for name in restore {
        if let Err(error) = restore_project(&state, &name).await {
            error!(project = %name, %error, "project restore failed");
        }
    }
    tokio::spawn(cron_loop(Arc::clone(&state)));
    info!(endpoint = %listener.path().display(), "daemon listening");
    loop {
        tokio::select! {
            incoming = listener.accept() => {
                match incoming {
                    Ok(stream) => {
                        let state = Arc::clone(&state);
                        tokio::spawn(async move {
                            if let Err(err) = serve(stream, state).await {
                                error!(%err, "client request failed");
                            }
                        });
                    }
                    Err(error) => {
                        error!(%error, "daemon endpoint accept failed; retrying");
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                }
            }
            () = state.shutdown.notified() => break,
        }
    }
    listener.cleanup()?;
    info!("daemon stopped");
    Ok(())
}

async fn serve<S>(mut stream: S, state: Arc<State>) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut bytes = Vec::new();
    {
        let reader = BufReader::new(&mut stream);
        reader
            .take((MAX_MESSAGE_BYTES + 1) as u64)
            .read_until(b'\n', &mut bytes)
            .await?
    };
    let mut shutdown = false;
    let response = if bytes.len() > MAX_MESSAGE_BYTES {
        Response::Error {
            code: "Protocol".into(),
            message: "request too large".into(),
        }
    } else {
        match serde_json::from_slice::<Envelope<Request>>(&bytes) {
            Ok(envelope) if envelope.version == PROTOCOL_VERSION => {
                shutdown = matches!(&envelope.payload, Request::Shutdown);
                dispatch(envelope.payload, &state).await
            }
            Ok(_) => Response::Error {
                code: "Protocol".into(),
                message: "incompatible protocol version".into(),
            },
            Err(err) => Response::Error {
                code: "Protocol".into(),
                message: err.to_string(),
            },
        }
    };
    let mut data = serde_json::to_vec(&Envelope {
        version: PROTOCOL_VERSION,
        payload: response,
    })?;
    data.push(b'\n');
    stream.write_all(&data).await?;
    if shutdown {
        state.shutdown.notify_waiters();
    }
    Ok(())
}

async fn dispatch(request: Request, state: &Arc<State>) -> Response {
    match execute(request, state).await {
        Ok(response) => response,
        Err(error) => Response::Error {
            code: error.code().into(),
            message: error.to_string(),
        },
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "protocol dispatch keeps request handling auditable in one match"
)]
async fn execute(request: Request, state: &Arc<State>) -> localweb_core::Result<Response> {
    match request {
        Request::Status => {
            let projects = load_projects(&state.paths)?;
            let gateway_running = is_active("localweb-gateway");
            let gateway_endpoint = load_gateway_endpoint(&state.paths);
            Ok(Response::Status(Status {
                daemon_running: true,
                gateway_running,
                gateway_bind_address: gateway_endpoint.map(|endpoint| endpoint.bind_address),
                gateway_https_port: gateway_endpoint.map(|endpoint| endpoint.https_port),
                project_count: projects.len(),
                active_projects: state.running.lock().await.len(),
            }))
        }
        Request::Shutdown => {
            let _operations = state.operations.lock().await;
            let result = {
                let state = Arc::clone(state);
                tokio::task::spawn_blocking(move || shutdown_services(&state))
                    .await
                    .map_err(|error| join_failed(&error))?
            };
            match result {
                Ok(()) => {
                    persist_running(&state.paths, &BTreeSet::new())?;
                    state.running.lock().await.clear();
                    Ok(Response::Message("LocalWeb daemon stopped".into()))
                }
                Err(error) => {
                    error!(%error, "shutdown left services running; desired state was preserved");
                    Err(error)
                }
            }
        }
        Request::ProjectList => {
            let running = state.running.lock().await;
            let gateway_endpoint = active_gateway_endpoint(&state.paths);
            Ok(Response::Projects(
                load_projects(&state.paths)?
                    .into_iter()
                    .map(|project| {
                        let active = running.contains(&project.name);
                        let mail_web_url = mail_web_url(&state.paths, &project);
                        ProjectStatus {
                            public_url: gateway_endpoint
                                .map(|endpoint| gateway_url(&project.hostname, endpoint)),
                            project,
                            running: active,
                            mail_web_url,
                        }
                    })
                    .collect(),
            ))
        }
        Request::ProjectGet { name } => {
            let project = find_project(&state.paths, &name)?;
            let running = state.running.lock().await.contains(&name);
            let mail_web_url = mail_web_url(&state.paths, &project);
            Ok(Response::Project(ProjectStatus {
                public_url: active_gateway_endpoint(&state.paths)
                    .map(|endpoint| gateway_url(&project.hostname, endpoint)),
                project,
                running,
                mail_web_url,
            }))
        }
        Request::ProjectAdd(input) => {
            let _operations = state.operations.lock().await;
            let mut project = Project::from_new(input, &state.config)?;
            for owned in [
                &state.paths.config,
                &state.paths.data,
                &state.paths.state,
                &state.paths.runtime,
            ] {
                if project.path.starts_with(owned.canonicalize()?) {
                    return Err(LocalWebError::Config(
                        "source folders must be outside LocalWeb-owned directories".into(),
                    ));
                }
            }
            let projects = load_projects(&state.paths)?;
            if projects.iter().any(|p| p.name == project.name) {
                return Err(LocalWebError::Config(format!(
                    "project name already exists: {}",
                    project.name
                )));
            }
            if projects.iter().any(|p| p.hostname == project.hostname) {
                return Err(LocalWebError::DomainConflict(project.hostname));
            }
            assign_project_ports(&projects, &mut project)?;
            save_project(&state.paths, &project)?;
            Ok(Response::Project(ProjectStatus {
                public_url: active_gateway_endpoint(&state.paths)
                    .map(|endpoint| gateway_url(&project.hostname, endpoint)),
                project,
                running: false,
                mail_web_url: None,
            }))
        }
        Request::ProjectUpdate { name, input } => {
            let _operations = state.operations.lock().await;
            let existing = find_project(&state.paths, &name)?;
            let mut updated = Project::from_update(&existing, input, &state.config)?;
            let projects = load_projects(&state.paths)?;
            let mut used: BTreeSet<u16> = projects
                .iter()
                .filter(|project| project.id != existing.id)
                .flat_map(|project| [project.web_port, project.database_port])
                .flatten()
                .collect();
            if let Some(port) = updated.web_port {
                used.insert(port);
            }
            if services::database_service_port(updated.database).is_some()
                && updated.database_port.is_none()
            {
                updated.database_port = Some(allocate_port(
                    DATABASE_PORT_START,
                    DATABASE_PORT_END,
                    &used,
                )?);
            }
            let services = Services {
                paths: &state.paths,
            };
            services.check_project(&updated)?;
            let was_running = state.running.lock().await.contains(&name);
            if was_running {
                stop_project_locked(state, &name).await?;
            }
            if let Err(error) = save_project(&state.paths, &updated) {
                if was_running {
                    let _ = start_project_locked(state, &name, false, true).await;
                }
                return Err(error);
            }
            let running = if was_running {
                match start_project_locked(state, &name, false, true).await {
                    Ok(()) => true,
                    Err(error) => {
                        let restore = save_project(&state.paths, &existing);
                        let recovery = match restore {
                            Ok(()) => start_project_locked(state, &name, false, true).await.err(),
                            Err(restore) => Some(restore),
                        };
                        let mut message =
                            format!("could not start {name} with the updated settings: {error}");
                        match recovery {
                            Some(recovery) => {
                                message.push_str(
                                    "; the previous settings were restored but the project could not be restarted: ",
                                );
                                message.push_str(&recovery.to_string());
                            }
                            None => message.push_str("; the previous settings were restored"),
                        }
                        return Err(LocalWebError::Service(message));
                    }
                }
            } else {
                false
            };
            Ok(Response::Project(ProjectStatus {
                public_url: active_gateway_endpoint(&state.paths)
                    .map(|endpoint| gateway_url(&updated.hostname, endpoint)),
                mail_web_url: mail_web_url(&state.paths, &updated),
                project: updated,
                running,
            }))
        }
        Request::ProjectRemove { name } => {
            let _operations = state.operations.lock().await;
            let project = find_project(&state.paths, &name)?;
            if state.running.lock().await.contains(&name) {
                return Err(LocalWebError::Service(
                    "stop the project before removing it".into(),
                ));
            }
            fs::remove_file(state.paths.project_file(project.id))?;
            Ok(Response::Message(format!(
                "Removed LocalWeb configuration for {name}; source files were retained"
            )))
        }
        Request::ProjectStart { name } => {
            start_project(state, &name).await?;
            running_message(state, &name)
        }
        Request::ProjectRestart { name } => {
            restart_project(state, &name).await?;
            running_message(state, &name)
        }
        Request::ProjectStop { name } => {
            let _ = find_project(&state.paths, &name)?;
            stop_project(state, &name).await?;
            Ok(Response::Message(format!("{name} is stopped")))
        }
        Request::ProjectEnvironment { name } => {
            let project = find_project(&state.paths, &name)?;
            let services = Services {
                paths: &state.paths,
            };
            Ok(Response::Environment(services.environment(&project)?))
        }
        Request::RuntimeList => Ok(Response::Diagnostics(runtime_diagnostics())),
        Request::ResolveExecution { name } => {
            let project = find_project(&state.paths, &name)?;
            let services = Services {
                paths: &state.paths,
            };
            let (php, _) = services.php_binaries(&project.php)?;
            let (node, npm_cli) = Services::node_binaries(&project.node)
                .map_or((None, None), |(node, npm)| (Some(node), Some(npm)));
            let shim_dir = state
                .paths
                .runtime
                .join("services/shims")
                .join(&project.name);
            fs::create_dir_all(&shim_dir)?;
            install_shim(&shim_dir.join("php"), &php)?;
            let localweb = std::env::current_exe()?
                .with_file_name(format!("localweb{}", std::env::consts::EXE_SUFFIX));
            if localweb.is_file() {
                install_shim(&shim_dir.join("composer"), &localweb)?;
                install_shim(&shim_dir.join("node"), &localweb)?;
                install_shim(&shim_dir.join("npm"), &localweb)?;
                install_shim(&shim_dir.join("npx"), &localweb)?;
            }
            let runtime = RuntimeLayout::discover();
            let phar = runtime.composer(&project.composer).ok();
            let bundled = php.starts_with(runtime.root());
            let php_ini = services.php_ini(&project.php);
            Ok(Response::Execution(ExecutionSpec {
                project_path: project.path,
                php,
                composer_phar: phar,
                node,
                npm_cli,
                shim_dir,
                library_path: bundled.then(|| runtime.library_path()),
                php_ini,
            }))
        }
        Request::CaStatus => {
            let ca = CaPaths::new(&state.paths);
            let exists = ca.cert.is_file() && ca.key.is_file();
            let system_trusted =
                exists && localweb_core::platform::TrustStore::detect()?.trusted(&ca.cert);
            let browser_trusted = exists && localweb_core::platform::browser_ca_trusted(&ca.cert);
            Ok(Response::Ca(CaStatus {
                exists,
                trusted: system_trusted && browser_trusted,
                system_trusted,
                browser_trusted,
                certificate: ca.cert,
            }))
        }
        Request::Doctor => Ok(Response::Diagnostics(doctor(state))),
    }
}

fn running_message(state: &State, name: &str) -> localweb_core::Result<Response> {
    let project = find_project(&state.paths, name)?;
    let direct = project.web_port.map_or_else(
        || "an unknown direct port".to_owned(),
        |port| format!("http://127.0.0.1:{port}"),
    );
    Ok(Response::Message(
        if let Some(endpoint) = active_gateway_endpoint(&state.paths) {
            format!(
                "{name} is running at {direct} and {}",
                gateway_url(&project.hostname, endpoint)
            )
        } else {
            format!("{name} is running at {direct}")
        },
    ))
}

#[cfg(unix)]
fn install_shim(link: &std::path::Path, target: &std::path::Path) -> localweb_core::Result<()> {
    use std::os::unix::fs::symlink;
    let temp = link.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    symlink(target, &temp)?;
    if let Err(error) = fs::rename(&temp, link) {
        let _ = fs::remove_file(&temp);
        return Err(error.into());
    }
    Ok(())
}

#[cfg(windows)]
fn install_shim(link: &std::path::Path, target: &std::path::Path) -> localweb_core::Result<()> {
    let script = format!("@echo off\r\n\"{}\" %*\r\n", target.display());
    atomic_write(&link.with_extension("cmd"), script.as_bytes(), 0)
}

async fn start_project(state: &Arc<State>, name: &str) -> localweb_core::Result<()> {
    let _operations = state.operations.lock().await;
    start_project_locked(state, name, false, true).await
}

/// Restarts a project that was running before the daemon restarted.
///
/// The preflight that rejects a busy web port is skipped because the daemon's
/// own nginx or Apache unit may still be serving the port after a crash.
async fn restore_project(state: &Arc<State>, name: &str) -> localweb_core::Result<()> {
    let _operations = state.operations.lock().await;
    start_project_locked(state, name, false, false).await
}

async fn restart_project(state: &Arc<State>, name: &str) -> localweb_core::Result<()> {
    let _operations = state.operations.lock().await;
    let running = state.running.lock().await.contains(name);
    start_project_locked(state, name, running, !running).await
}

async fn start_project_locked(
    state: &Arc<State>,
    name: &str,
    restart: bool,
    preflight_ports: bool,
) -> localweb_core::Result<()> {
    let project = find_project(&state.paths, name)?;
    if !project.enabled {
        return Err(LocalWebError::Config("project is disabled".into()));
    }
    let old = state.running.lock().await.clone();
    if old.contains(name) && !restart {
        return Ok(());
    }
    let mut next = old.clone();
    next.insert(name.to_owned());
    let fresh = !old.contains(name);
    if fresh
        && preflight_ports
        && let Some(port) = project.web_port
        && !port_available(port)
    {
        return Err(LocalWebError::Config(format!(
            "web port {port} for {name} is already in use"
        )));
    }
    let needs_proxy = !state.db_proxies.lock().await.contains_key(name);
    if needs_proxy {
        stop_db_proxy(state, name).await;
        if let Err(error) = start_db_proxy(state, &project).await {
            return Err(combine_rollback(
                error,
                rollback_start(state, &project, name, &old, fresh).await,
            ));
        }
    }
    let setup = {
        let state = Arc::clone(state);
        let project = project.clone();
        let next = next.clone();
        tokio::task::spawn_blocking(move || setup_project_services(&state, &project, &next))
            .await
            .map_err(|error| join_failed(&error))
    };
    let setup = match setup {
        Ok(result) => result,
        Err(error) => {
            return Err(combine_rollback(
                error,
                rollback_start(state, &project, name, &old, fresh).await,
            ));
        }
    };
    if let Err(error) = setup {
        return Err(combine_rollback(
            error,
            rollback_start(state, &project, name, &old, fresh).await,
        ));
    }
    if let Err(error) = health_check(&project).await {
        return Err(combine_rollback(
            error,
            rollback_start(state, &project, name, &old, fresh).await,
        ));
    }
    if let Err(error) = persist_running(&state.paths, &next) {
        return Err(combine_rollback(
            error,
            rollback_start(state, &project, name, &old, fresh).await,
        ));
    }
    *state.running.lock().await = next;
    Ok(())
}

fn setup_project_services(
    state: &State,
    project: &Project,
    next: &BTreeSet<String>,
) -> localweb_core::Result<()> {
    let services = Services {
        paths: &state.paths,
    };
    services.check_project(project)?;
    CaPaths::new(&state.paths).ensure_leaf(&project.hostname)?;
    let gateway_address = load_gateway_endpoint(&state.paths)
        .map_or(Ipv4Addr::LOCALHOST, |endpoint| endpoint.bind_address);
    if !hosts_has_domain(&project.hostname, gateway_address) {
        return Err(LocalWebError::HostMapping(format!(
            "{} must map to {gateway_address} in the managed /etc/hosts block; run `localweb hosts sync` and start the project again",
            project.hostname
        )));
    }
    if project.database == localweb_core::Database::Mariadb {
        services.ensure_mariadb()?;
        services.ensure_project_database(project)?;
    }
    if project.database == localweb_core::Database::Mysql {
        services.ensure_mysql()?;
        services.ensure_project_mysql_database(project)?;
    }
    if project.database == localweb_core::Database::Postgres {
        services.ensure_postgres()?;
        services.ensure_project_postgres_database(project)?;
    }
    if project.database == localweb_core::Database::Sqlite {
        services.ensure_sqlite(project)?;
    }
    if project.cache == localweb_core::Cache::Redis {
        services.start_redis(project)?;
    }
    if project.mail {
        services.start_mailpit(project)?;
    }
    apply_services(state, next)?;
    services.start_supervisor(project)?;
    Ok(())
}

async fn rollback_start(
    state: &Arc<State>,
    project: &Project,
    name: &str,
    old: &BTreeSet<String>,
    fresh: bool,
) -> localweb_core::Result<()> {
    if fresh {
        stop_db_proxy(state, name).await;
    }
    let state = Arc::clone(state);
    let project = project.clone();
    let name = name.to_owned();
    let old = old.clone();
    tokio::task::spawn_blocking(move || {
        rollback_start_services(&state, &project, &name, &old, fresh)
    })
    .await
    .map_err(|error| join_failed(&error))?
}

fn rollback_start_services(
    state: &State,
    project: &Project,
    _name: &str,
    old: &BTreeSet<String>,
    fresh: bool,
) -> localweb_core::Result<()> {
    let mut failures = Vec::new();
    record(
        &mut failures,
        "restore previous web configuration",
        apply_services(state, old),
    );
    if fresh {
        if project.cache == localweb_core::Cache::Redis {
            record(&mut failures, "stop Redis", Services::stop_redis(project));
        }
        if project.mail {
            record(
                &mut failures,
                "stop Mailpit",
                Services::stop_mailpit(project),
            );
        }
        record(
            &mut failures,
            "stop supervisor",
            Services::stop_supervisor(project),
        );
    }
    if !active_uses_database(&state.paths, old, localweb_core::Database::Mariadb) {
        record(
            &mut failures,
            "stop MariaDB",
            Services::stop("localweb-db-mariadb"),
        );
    }
    if !active_uses_database(&state.paths, old, localweb_core::Database::Mysql) {
        record(
            &mut failures,
            "stop MySQL",
            Services::stop("localweb-db-mysql"),
        );
    }
    if !active_uses_database(&state.paths, old, localweb_core::Database::Postgres) {
        record(
            &mut failures,
            "stop PostgreSQL",
            Services::stop("localweb-db-postgres"),
        );
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(LocalWebError::Service(failures.join("; ")))
    }
}

fn combine_rollback(primary: LocalWebError, rollback: localweb_core::Result<()>) -> LocalWebError {
    match rollback {
        Ok(()) => primary,
        Err(rollback) => {
            LocalWebError::Service(format!("{primary}; rollback also failed: {rollback}"))
        }
    }
}

fn record(failures: &mut Vec<String>, label: &str, result: localweb_core::Result<()>) {
    if let Err(error) = result {
        failures.push(format!("{label}: {error}"));
    }
}

fn join_failed(error: &tokio::task::JoinError) -> LocalWebError {
    LocalWebError::Service(format!("background task failed: {error}"))
}

async fn stop_project(state: &Arc<State>, name: &str) -> localweb_core::Result<()> {
    let _operations = state.operations.lock().await;
    stop_project_locked(state, name).await
}

async fn stop_project_locked(state: &Arc<State>, name: &str) -> localweb_core::Result<()> {
    let project = find_project(&state.paths, name)?;
    let mut active = state.running.lock().await.clone();
    if !active.remove(name) {
        let project_owned = project.clone();
        let stopped =
            tokio::task::spawn_blocking(move || stop_project_owned_services(&project_owned))
                .await
                .map_err(|error| join_failed(&error))?;
        stop_db_proxy(state, name).await;
        return stopped;
    }
    let state_owned = Arc::clone(state);
    let project_owned = project.clone();
    let active_owned = active.clone();
    let stopped = tokio::task::spawn_blocking(move || {
        stop_project_services(&state_owned, &project_owned, &active_owned)
    })
    .await
    .map_err(|error| join_failed(&error))?;
    if let Err(error) = stopped {
        error!(project = %name, %error, "project stop failed; committed state was preserved");
        return Err(error);
    }
    stop_db_proxy(state, name).await;
    *state.running.lock().await = active;
    Ok(())
}

async fn start_db_proxy(state: &Arc<State>, project: &Project) -> localweb_core::Result<()> {
    let Some(target) = services::database_service_port(project.database) else {
        return Ok(());
    };
    let listen = project
        .database_port
        .ok_or_else(|| LocalWebError::Config(format!("{} has no database port", project.name)))?;
    let proxy = DbProxy::start(listen, target).await.map_err(|error| {
        LocalWebError::Service(format!(
            "cannot bind database port {listen} for {}: {error}",
            project.name
        ))
    })?;
    state
        .db_proxies
        .lock()
        .await
        .insert(project.name.clone(), proxy);
    Ok(())
}

async fn stop_db_proxy(state: &Arc<State>, name: &str) {
    let proxy = state.db_proxies.lock().await.remove(name);
    if let Some(proxy) = proxy {
        proxy.stop().await;
    }
}

fn stop_project_services(
    state: &State,
    project: &Project,
    active: &BTreeSet<String>,
) -> localweb_core::Result<()> {
    let mut failures = Vec::new();
    record(
        &mut failures,
        "stop project-owned services",
        stop_project_owned_services(project),
    );
    record(
        &mut failures,
        "rewrite web configuration",
        apply_services(state, active),
    );
    if !failures.is_empty() {
        return Err(LocalWebError::Service(format!(
            "project stop completed with errors: {}",
            failures.join("; ")
        )));
    }
    persist_running(&state.paths, active)?;
    if !active_uses_database(&state.paths, active, localweb_core::Database::Mariadb)
        && let Err(error) = Services::stop("localweb-db-mariadb")
    {
        warn!(%error, "could not stop the shared MariaDB service");
    }
    if !active_uses_database(&state.paths, active, localweb_core::Database::Mysql)
        && let Err(error) = Services::stop("localweb-db-mysql")
    {
        warn!(%error, "could not stop the shared MySQL service");
    }
    if !active_uses_database(&state.paths, active, localweb_core::Database::Postgres)
        && let Err(error) = Services::stop("localweb-db-postgres")
    {
        warn!(%error, "could not stop the shared PostgreSQL service");
    }
    Ok(())
}

fn stop_project_owned_services(project: &Project) -> localweb_core::Result<()> {
    let mut failures = Vec::new();
    record(
        &mut failures,
        "stop supervisor",
        Services::stop_supervisor(project),
    );
    if project.cache == localweb_core::Cache::Redis {
        record(&mut failures, "stop Redis", Services::stop_redis(project));
    }
    if project.mail {
        record(
            &mut failures,
            "stop Mailpit",
            Services::stop_mailpit(project),
        );
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(LocalWebError::Service(failures.join("; ")))
    }
}

fn load_desired_running(paths: &Paths) -> localweb_core::Result<BTreeSet<String>> {
    let file = paths.state.join("active-projects.json");
    if !file.is_file() {
        return Ok(BTreeSet::new());
    }
    serde_json::from_slice(&fs::read(file)?)
        .map_err(|error| LocalWebError::Config(format!("invalid active project state: {error}")))
}

fn persist_running(paths: &Paths, running: &BTreeSet<String>) -> localweb_core::Result<()> {
    let encoded = serde_json::to_vec_pretty(running)
        .map_err(|error| LocalWebError::Config(error.to_string()))?;
    atomic_write(&paths.state.join("active-projects.json"), &encoded, 0o600)
}

/// First port of the fixed direct web range for projects.
const WEB_PORT_START: u16 = 18_000;
/// Last port of the fixed direct web range for projects.
const WEB_PORT_END: u16 = 18_999;
/// First port of the fixed database proxy range for projects.
const DATABASE_PORT_START: u16 = 19_000;
/// Last port of the fixed database proxy range for projects.
const DATABASE_PORT_END: u16 = 19_999;

fn gateway_state_file(paths: &Paths) -> std::path::PathBuf {
    paths.state.join("gateway-state.json")
}

fn legacy_gateway_state_file(paths: &Paths) -> std::path::PathBuf {
    paths.runtime.join("gateway-state.json")
}

fn load_gateway_endpoint(paths: &Paths) -> Option<GatewayState> {
    let bytes = fs::read(gateway_state_file(paths)).ok()?;
    let state: GatewayState = serde_json::from_slice(&bytes).ok()?;
    (state.schema_version == 1 && state.bind_address.is_loopback() && state.https_port != 0)
        .then_some(state)
}

fn load_legacy_gateway_endpoint(paths: &Paths) -> Option<GatewayState> {
    let bytes = fs::read(legacy_gateway_state_file(paths)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn persist_gateway_endpoint(paths: &Paths, endpoint: GatewayState) -> localweb_core::Result<()> {
    let bytes = serde_json::to_vec_pretty(&endpoint)
        .map_err(|error| LocalWebError::Config(error.to_string()))?;
    atomic_write(&gateway_state_file(paths), &bytes, 0o600)?;
    match fs::remove_file(legacy_gateway_state_file(paths)) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn active_gateway_endpoint(paths: &Paths) -> Option<GatewayState> {
    is_active("localweb-gateway")
        .then(|| load_gateway_endpoint(paths))
        .flatten()
}

fn gateway_url(hostname: &str, endpoint: GatewayState) -> String {
    if endpoint.https_port == GATEWAY_HTTPS_PORT {
        format!("https://{hostname}")
    } else {
        format!("https://{hostname}:{}", endpoint.https_port)
    }
}

fn initialize_gateway_endpoint(paths: &Paths) -> localweb_core::Result<GatewayState> {
    let gateway_active = is_active("localweb-gateway");
    if let Some(endpoint) = load_gateway_endpoint(paths)
        && (gateway_active || !endpoint_in_use(endpoint))
    {
        return Ok(endpoint);
    }
    if gateway_active || load_legacy_gateway_endpoint(paths).is_some() {
        Services::stop("localweb-gateway")?;
    }
    let endpoint = choose_gateway_endpoint(
        |address| {
            !endpoint_in_use(GatewayState {
                schema_version: 1,
                bind_address: address,
                https_port: GATEWAY_HTTPS_PORT,
            })
        },
        port_available_at,
    )?;
    persist_gateway_endpoint(paths, endpoint)?;
    Ok(endpoint)
}

fn select_gateway_endpoint(
    paths: &Paths,
    gateway_active: bool,
) -> localweb_core::Result<GatewayState> {
    if let Some(endpoint) = load_gateway_endpoint(paths)
        && (gateway_active || !endpoint_in_use(endpoint))
    {
        return Ok(endpoint);
    }
    initialize_gateway_endpoint(paths)
}

fn choose_gateway_endpoint(
    mut standard_available: impl FnMut(Ipv4Addr) -> bool,
    mut fallback_available: impl FnMut(Ipv4Addr, u16) -> bool,
) -> localweb_core::Result<GatewayState> {
    #[cfg(all(unix, not(target_os = "macos")))]
    for host in GATEWAY_LOOPBACK_START..=GATEWAY_LOOPBACK_END {
        let address = Ipv4Addr::new(127, 0, 0, host);
        if standard_available(address) {
            return Ok(GatewayState {
                schema_version: 1,
                bind_address: address,
                https_port: GATEWAY_HTTPS_PORT,
            });
        }
    }
    if standard_available(Ipv4Addr::LOCALHOST) {
        return Ok(GatewayState {
            schema_version: 1,
            bind_address: Ipv4Addr::LOCALHOST,
            https_port: GATEWAY_HTTPS_PORT,
        });
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    let fallback_address = Ipv4Addr::new(127, 0, 0, GATEWAY_LOOPBACK_START);
    #[cfg(any(not(unix), target_os = "macos"))]
    let fallback_address = Ipv4Addr::LOCALHOST;
    for port in GATEWAY_FALLBACK_START..=GATEWAY_FALLBACK_END {
        if fallback_available(fallback_address, port) {
            return Ok(GatewayState {
                schema_version: 1,
                bind_address: fallback_address,
                https_port: port,
            });
        }
    }
    Err(LocalWebError::Service(format!(
        "no isolated LocalWeb HTTPS endpoint is available on loopback port 443 or in {GATEWAY_FALLBACK_START}-{GATEWAY_FALLBACK_END}"
    )))
}

fn port_available(port: u16) -> bool {
    port_available_at(Ipv4Addr::LOCALHOST, port)
}

fn port_available_at(address: Ipv4Addr, port: u16) -> bool {
    TcpListener::bind((address, port)).is_ok()
}

fn endpoint_in_use(endpoint: GatewayState) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from((endpoint.bind_address, endpoint.https_port)),
        Duration::from_millis(100),
    )
    .is_ok()
}

fn allocate_port(start: u16, end: u16, used: &BTreeSet<u16>) -> localweb_core::Result<u16> {
    for port in start..=end {
        if !used.contains(&port) && port_available(port) {
            return Ok(port);
        }
    }
    Err(LocalWebError::Service(format!(
        "no free LocalWeb port is available in {start}-{end}"
    )))
}

fn save_project(paths: &Paths, project: &Project) -> localweb_core::Result<()> {
    let contents = toml::to_string_pretty(project)
        .map_err(|error| LocalWebError::Config(error.to_string()))?;
    atomic_write(&paths.project_file(project.id), contents.as_bytes(), 0o600)
}

fn ensure_project_ports(paths: &Paths) -> localweb_core::Result<()> {
    let projects = load_projects(paths)?;
    let mut used_web = BTreeSet::new();
    let mut used_database = BTreeSet::new();
    for project in &projects {
        if let Some(port) = project.web_port
            && !used_web.insert(port)
        {
            return Err(LocalWebError::Config(format!(
                "duplicate web port {port}; edit the project configuration"
            )));
        }
        if let Some(port) = project.database_port
            && !used_database.insert(port)
        {
            return Err(LocalWebError::Config(format!(
                "duplicate database port {port}; edit the project configuration"
            )));
        }
        if project.web_port.is_some() && project.web_port == project.database_port {
            return Err(LocalWebError::Config(
                "web and database ports must differ".into(),
            ));
        }
    }
    if used_web.intersection(&used_database).next().is_some() {
        return Err(LocalWebError::Config(
            "a web port is also assigned as a database port; edit the project configuration".into(),
        ));
    }
    for mut project in projects {
        let mut changed = false;
        if project.web_port.is_none() {
            let port = allocate_port(WEB_PORT_START, WEB_PORT_END, &used_web)?;
            project.web_port = Some(port);
            used_web.insert(port);
            changed = true;
        }
        if services::database_service_port(project.database).is_some() {
            if project.database_port.is_none() {
                let port = allocate_port(DATABASE_PORT_START, DATABASE_PORT_END, &used_database)?;
                project.database_port = Some(port);
                used_database.insert(port);
                changed = true;
            }
        } else if project.database_port.take().is_some() {
            changed = true;
        }
        if project.web_port == project.database_port {
            return Err(LocalWebError::Config(
                "web and database ports must differ".into(),
            ));
        }
        if changed {
            save_project(paths, &project)?;
        }
    }
    Ok(())
}

fn assign_project_ports(projects: &[Project], project: &mut Project) -> localweb_core::Result<()> {
    let mut used: BTreeSet<u16> = projects
        .iter()
        .flat_map(|item| [item.web_port, item.database_port])
        .flatten()
        .collect();
    if let Some(port) = project.web_port {
        if used.contains(&port) {
            return Err(LocalWebError::Config(format!(
                "web port {port} is already assigned to another project"
            )));
        }
        if !port_available(port) {
            return Err(LocalWebError::Config(format!(
                "web port {port} is already in use"
            )));
        }
    } else {
        let port = allocate_port(WEB_PORT_START, WEB_PORT_END, &used)?;
        project.web_port = Some(port);
        used.insert(port);
    }
    if services::database_service_port(project.database).is_some() {
        if let Some(port) = project.database_port {
            if used.contains(&port) {
                return Err(LocalWebError::Config(format!(
                    "database port {port} is already assigned to another project"
                )));
            }
            if !port_available(port) {
                return Err(LocalWebError::Config(format!(
                    "database port {port} is already in use"
                )));
            }
        } else {
            let port = allocate_port(DATABASE_PORT_START, DATABASE_PORT_END, &used)?;
            project.database_port = Some(port);
            used.insert(port);
        }
    } else {
        project.database_port = None;
    }
    if project.web_port == project.database_port {
        return Err(LocalWebError::Config(
            "web and database ports must differ".into(),
        ));
    }
    Ok(())
}

fn shutdown_services(state: &State) -> localweb_core::Result<()> {
    let mut failures = Vec::new();
    if let Err(error) = write_routes(state, &BTreeSet::new()) {
        failures.push(error.to_string());
    }
    if let Ok(projects) = load_projects(&state.paths) {
        for project in projects {
            if let Err(error) = Services::stop_supervisor(&project) {
                failures.push(error.to_string());
            }
            if project.cache == localweb_core::Cache::Redis
                && let Err(error) = Services::stop_redis(&project)
            {
                failures.push(error.to_string());
            }
            if project.mail
                && let Err(error) = Services::stop_mailpit(&project)
            {
                failures.push(error.to_string());
            }
        }
    }
    for version in localweb_core::PHP_VERSIONS {
        if let Err(error) = Services::stop(&php_unit(version)) {
            failures.push(error.to_string());
        }
    }
    for unit in [
        "localweb-gateway",
        "localweb-nginx",
        "localweb-apache",
        "localweb-db-mariadb",
        "localweb-db-mysql",
        "localweb-db-postgres",
    ] {
        if let Err(error) = Services::stop(unit) {
            failures.push(error.to_string());
        }
    }
    if !is_active("localweb-gateway") {
        let _ = fs::remove_file(legacy_gateway_state_file(&state.paths));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(LocalWebError::Service(format!(
            "shutdown completed with errors: {}",
            failures.join("; ")
        )))
    }
}

fn active_uses_database(
    paths: &Paths,
    active: &BTreeSet<String>,
    database: localweb_core::Database,
) -> bool {
    load_projects(paths).is_ok_and(|projects| {
        projects
            .iter()
            .any(|project| active.contains(&project.name) && project.database == database)
    })
}

fn apply_services(state: &State, active: &BTreeSet<String>) -> localweb_core::Result<()> {
    let services = Services {
        paths: &state.paths,
    };
    let projects: Vec<_> = load_projects(&state.paths)?
        .into_iter()
        .filter(|p| active.contains(&p.name))
        .collect();
    let versions: BTreeSet<_> = projects.iter().map(|p| p.php.clone()).collect();
    for version in &versions {
        services.start_php(version, &projects)?;
    }
    for version in localweb_core::PHP_VERSIONS {
        if !versions.contains(version) {
            Services::stop(&php_unit(version))?;
        }
    }
    let has_nginx = projects.iter().any(|p| p.web_server == WebServer::Nginx);
    let has_apache = projects.iter().any(|p| p.web_server == WebServer::Apache);
    if has_nginx {
        let config = services.render_nginx(&projects)?;
        let config_str = config
            .to_str()
            .ok_or_else(|| LocalWebError::Config("non-UTF-8 nginx config path".into()))?;
        let runtime = RuntimeLayout::discover();
        let nginx = runtime.nginx()?;
        let prefix = runtime.nginx_prefix();
        let prefix_str = prefix
            .to_str()
            .ok_or_else(|| LocalWebError::Config("non-UTF-8 nginx prefix".into()))?;
        Services::start_or_restart(
            "localweb-nginx",
            &nginx,
            &["-p", prefix_str, "-c", config_str, "-g", "daemon off;"],
        )?;
    } else {
        Services::stop("localweb-nginx")?;
    }
    if has_apache {
        let config = services.render_apache(&projects)?;
        let config_str = config
            .to_str()
            .ok_or_else(|| LocalWebError::Config("non-UTF-8 Apache config path".into()))?;
        let runtime = RuntimeLayout::discover();
        let apache = runtime.apache()?;
        Services::start_or_restart(
            "localweb-apache",
            &apache,
            &["-f", config_str, "-DFOREGROUND"],
        )?;
    } else {
        Services::stop("localweb-apache")?;
    }
    write_routes(state, active)?;
    let gateway = std::env::current_exe()?
        .with_file_name(format!("localweb-gateway{}", std::env::consts::EXE_SUFFIX));
    let gateway_active = is_active("localweb-gateway");
    let mut endpoint = select_gateway_endpoint(&state.paths, gateway_active)?;
    if let Err(primary) = start_gateway_service(&gateway, endpoint) {
        if endpoint.https_port != GATEWAY_HTTPS_PORT {
            return Err(primary);
        }
        Services::stop("localweb-gateway")?;
        let fallback_port = (GATEWAY_FALLBACK_START..=GATEWAY_FALLBACK_END)
            .find(|port| port_available_at(endpoint.bind_address, *port))
            .ok_or_else(|| {
                LocalWebError::Service(format!(
                    "standard HTTPS endpoint failed ({primary}); no fallback port is free in {GATEWAY_FALLBACK_START}-{GATEWAY_FALLBACK_END}"
                ))
            })?;
        endpoint.https_port = fallback_port;
        start_gateway_service(&gateway, endpoint).map_err(|fallback| {
            LocalWebError::Service(format!(
                "standard HTTPS endpoint failed ({primary}); fallback failed ({fallback})"
            ))
        })?;
    }
    persist_gateway_endpoint(&state.paths, endpoint)?;
    if endpoint.https_port != GATEWAY_HTTPS_PORT {
        warn!(
            standard_port = GATEWAY_HTTPS_PORT,
            fallback_port = endpoint.https_port,
            bind_address = %endpoint.bind_address,
            "standard HTTPS binding is unavailable; using a loopback fallback without modifying other services"
        );
    } else if endpoint.bind_address != Ipv4Addr::LOCALHOST {
        info!(
            bind_address = %endpoint.bind_address,
            "gateway isolated on a dedicated loopback address"
        );
    }
    Ok(())
}

fn start_gateway_service(
    gateway: &std::path::Path,
    endpoint: GatewayState,
) -> localweb_core::Result<()> {
    let bind_address = endpoint.bind_address.to_string();
    let https_port = endpoint.https_port.to_string();
    Services::start_or_restart(
        "localweb-gateway",
        gateway,
        &["--bind-address", &bind_address, "--https-port", &https_port],
    )?;
    services::wait_for_unit_active("localweb-gateway", "HTTPS gateway")
}

fn write_routes(state: &State, active: &BTreeSet<String>) -> localweb_core::Result<()> {
    let routes: Vec<_> = load_projects(&state.paths)?
        .into_iter()
        .map(|project| {
            let upstream_port = active
                .contains(&project.name)
                .then_some(project.web_port)
                .flatten();
            GatewayRoute {
                hostname: project.hostname,
                upstream_port,
            }
        })
        .collect();
    for route in &routes {
        CaPaths::new(&state.paths).ensure_leaf(&route.hostname)?;
    }
    let bytes =
        serde_json::to_vec_pretty(&routes).map_err(|e| LocalWebError::Config(e.to_string()))?;
    atomic_write(
        &state.paths.runtime.join("gateway-routes.json"),
        &bytes,
        0o600,
    )
}

fn hosts_has_domain(domain: &str, address: Ipv4Addr) -> bool {
    let Ok(contents) = fs::read_to_string("/etc/hosts") else {
        return false;
    };
    let expected_address = address.to_string();
    let mut managed = false;
    for line in contents.lines() {
        if line == localweb_core::hosts::BEGIN {
            managed = true;
            continue;
        }
        if line == localweb_core::hosts::END {
            managed = false;
        }
        let mut fields = line.split_whitespace();
        if managed
            && fields.next() == Some(expected_address.as_str())
            && fields.next() == Some(domain)
            && fields.next().is_none()
        {
            return true;
        }
    }
    false
}

async fn health_check(project: &Project) -> localweb_core::Result<()> {
    use tokio::{
        net::TcpStream,
        time::{sleep, timeout},
    };
    let port = project
        .web_port
        .ok_or_else(|| LocalWebError::Config(format!("{} has no direct web port", project.name)))?;
    for _ in 0..20 {
        if let Ok(Ok(stream)) = timeout(
            Duration::from_secs(1),
            TcpStream::connect(("127.0.0.1", port)),
        )
        .await
            && let Ok(Some(status)) = probe_http_status(stream, &project.hostname).await
            && healthy_status(status)
        {
            return Ok(());
        }
        sleep(Duration::from_millis(150)).await;
    }
    Err(LocalWebError::Service(format!(
        "{} did not pass the HTTP health check; inspect the selected web server and PHP-FPM logs",
        project.name
    )))
}

/// Sends a minimal HTTP/1.1 request and returns the status code from a
/// bounded, CRLF-terminated status line. Responses that do not start with a
/// supported HTTP version or exceed 512 bytes without a line ending are
/// rejected instead of being treated as healthy.
async fn probe_http_status(
    mut stream: tokio::net::TcpStream,
    hostname: &str,
) -> localweb_core::Result<Option<u16>> {
    use tokio::{io::AsyncBufReadExt as _, io::AsyncWriteExt as _, time::timeout};
    let request = format!("GET / HTTP/1.1\r\nHost: {hostname}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await?;
    let mut reader = tokio::io::BufReader::new(stream.take(512));
    let mut line = String::new();
    let read = timeout(Duration::from_secs(2), reader.read_line(&mut line)).await;
    let Ok(Ok(read)) = read else {
        return Ok(None);
    };
    if read == 0 || !line.ends_with("\r\n") {
        return Ok(None);
    }
    Ok(status_code(line.as_bytes()))
}

fn status_code(line: &[u8]) -> Option<u16> {
    if line.len() > 512 {
        return None;
    }
    let line = std::str::from_utf8(line).ok()?.trim_end_matches("\r\n");
    let mut fields = line.split_ascii_whitespace();
    let version = fields.next()?;
    if version != "HTTP/1.0" && version != "HTTP/1.1" {
        return None;
    }
    fields.next()?.parse::<u16>().ok()
}

/// 2xx/3xx and authentication challenges (401/403) mean the web server is
/// reachable and the application answered; 404/5xx, malformed responses and
/// timeouts are not usable for a freshly started project.
fn healthy_status(status: u16) -> bool {
    matches!(status, 200..=399 | 401 | 403)
}

#[cfg(test)]
fn healthy_response(response: &[u8]) -> bool {
    let Some(end) = response.windows(2).position(|window| window == b"\r\n") else {
        return false;
    };
    status_code(&response[..=end + 1]).is_some_and(healthy_status)
}

async fn cron_loop(state: Arc<State>) {
    let mut ticker = tokio::time::interval(CRON_TICK_INTERVAL);
    loop {
        tokio::select! {
            () = state.shutdown.notified() => break,
            _ = ticker.tick() => cron_tick(&state).await,
        }
    }
}

async fn cron_tick(state: &Arc<State>) {
    let now = chrono::Local::now();
    let projects = match load_projects(&state.paths) {
        Ok(projects) => projects,
        Err(error) => {
            error!(%error, "cron tick could not list projects");
            return;
        }
    };
    let running = state.running.lock().await.clone();
    for project in projects {
        if !running.contains(&project.name) || project.cron.is_empty() {
            continue;
        }
        for (index, entry) in project.cron.iter().enumerate() {
            if !entry.enabled || !localweb_core::cron_matches(&entry.schedule, &now) {
                continue;
            }
            let key = (project.name.clone(), index);
            let minute = now.timestamp().div_euclid(60);
            let mut last = state.cron_last_minute.lock().await;
            if last.get(&key) == Some(&minute) {
                continue;
            }
            if !state.cron_busy.lock().await.insert(key.clone()) {
                continue;
            }
            last.insert(key.clone(), minute);
            drop(last);
            let state = Arc::clone(state);
            let project = project.clone();
            let entry = entry.clone();
            tokio::spawn(async move {
                run_cron_entry(&state, &project, &entry).await;
                state.cron_busy.lock().await.remove(&key);
            });
        }
    }
}

async fn run_cron_entry(state: &State, project: &Project, entry: &localweb_core::CronEntry) {
    let log_path = state
        .paths
        .state
        .join(format!("logs/{}-cron.log", project.name));
    let started = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%z")
        .to_string();
    let summary = execute_cron_entry(state, project, entry).await;
    let line = match &summary {
        Ok(detail) => format!("[{started}] {detail}\n"),
        Err(error) => format!("[{started}] failed: {error}\n"),
    };
    if let Err(error) = append_cron_log(&log_path, &line).await {
        error!(project = %project.name, %error, "cron log append failed");
    }
    if let Err(error) = summary {
        error!(project = %project.name, %error, "cron entry failed");
    }
}

async fn execute_cron_entry(
    state: &State,
    project: &Project,
    entry: &localweb_core::CronEntry,
) -> localweb_core::Result<String> {
    if entry.command.is_empty() {
        return Err(localweb_core::LocalWebError::Config(
            "cron entry has no command".into(),
        ));
    }
    let services = Services {
        paths: &state.paths,
    };
    let executable = if entry.command[0] == "@php" {
        services.php_binaries(&project.php)?.0
    } else {
        let requested = std::path::PathBuf::from(&entry.command[0]);
        let candidate = if requested.is_absolute() {
            requested.clone()
        } else {
            project.path.join(&requested)
        };
        let resolved = candidate.canonicalize().map_err(|error| {
            localweb_core::LocalWebError::Config(format!(
                "cannot resolve cron executable {}: {error}",
                candidate.display()
            ))
        })?;
        if !requested.is_absolute() && !resolved.starts_with(&project.path) {
            return Err(localweb_core::LocalWebError::Config(
                "relative cron executable escapes the project directory".into(),
            ));
        }
        resolved
    };
    let runtime = RuntimeLayout::discover();
    let args = if entry.command[0] == "@php" {
        services.php_command_args(&project.php, &entry.command[1..])
    } else {
        entry.command[1..].to_vec()
    };
    let output = tokio::process::Command::new(&executable)
        .args(args)
        .current_dir(&project.path)
        .env("LD_LIBRARY_PATH", runtime.library_path())
        .output()
        .await?;
    let code = output.status.code().unwrap_or(-1);
    let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    let tail: Vec<&str> = combined.lines().rev().take(5).collect();
    let tail: Vec<&str> = tail.into_iter().rev().collect();
    Ok(format!("exit={code} {}", tail.join(" | ")))
}

async fn append_cron_log(path: &std::path::Path, line: &str) -> localweb_core::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await?;
    file.write_all(line.as_bytes()).await?;
    Ok(())
}

fn mail_web_url(paths: &Paths, project: &Project) -> Option<String> {
    if !project.mail {
        return None;
    }
    Services { paths }
        .environment(project)
        .ok()?
        .get("MAIL_WEB_URL")
        .cloned()
}

fn load_projects(paths: &Paths) -> localweb_core::Result<Vec<Project>> {
    let mut projects = Vec::new();
    for entry in fs::read_dir(paths.config.join("projects"))? {
        let entry = entry?;
        if entry.path().extension().is_some_and(|e| e == "toml") {
            let project: Project = toml::from_str(&fs::read_to_string(entry.path())?)
                .map_err(|e| LocalWebError::Config(e.to_string()))?;
            projects.push(project);
        }
    }
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(projects)
}

fn find_project(paths: &Paths, name: &str) -> localweb_core::Result<Project> {
    load_projects(paths)?
        .into_iter()
        .find(|project| project.name == name)
        .ok_or_else(|| LocalWebError::ProjectNotFound(name.into()))
}

#[expect(
    clippy::too_many_lines,
    reason = "the ordered Doctor checklist stays auditable in one function"
)]
fn doctor(state: &State) -> Vec<Diagnostic> {
    let mut checks = Vec::new();
    let detail = if cfg!(target_os = "macos") {
        "macOS".to_owned()
    } else {
        let release = fs::read_to_string("/etc/os-release").unwrap_or_default();
        release
            .lines()
            .find(|line| line.starts_with("PRETTY_NAME="))
            .unwrap_or("unknown")
            .into()
    };
    checks.push(Diagnostic {
        level: "OK".into(),
        check: "Operating system".into(),
        detail,
        fix: None,
    });
    checks.push(Diagnostic {
        level: "OK".into(),
        check: "daemon socket".into(),
        detail: state.paths.socket().display().to_string(),
        fix: None,
    });
    let gateway_endpoint = active_gateway_endpoint(&state.paths);
    let selected_gateway = load_gateway_endpoint(&state.paths);
    checks.push(Diagnostic {
        level: if gateway_endpoint.is_some() {
            "OK"
        } else {
            "WARNING"
        }
        .into(),
        check: "HTTPS gateway".into(),
        detail: gateway_endpoint.map_or_else(
            || "stopped; it starts when the first project starts".into(),
            |endpoint| {
                format!(
                    "listening on {}:{}",
                    endpoint.bind_address, endpoint.https_port
                )
            },
        ),
        fix: gateway_endpoint
            .is_none()
            .then(|| "Start a project with `localweb project start NAME`".into()),
    });
    if selected_gateway.is_some_and(|endpoint| endpoint.https_port != GATEWAY_HTTPS_PORT) {
        checks.push(Diagnostic {
            level: "WARNING".into(),
            check: "standard HTTPS port".into(),
            detail: "standard binding is unavailable; LocalWeb leaves its owner untouched".into(),
            fix: Some(format!(
                "Use the HTTPS URL shown by LocalWeb with its fallback port ({GATEWAY_FALLBACK_START}-{GATEWAY_FALLBACK_END})"
            )),
        });
    } else if let Some(endpoint) =
        selected_gateway.filter(|endpoint| endpoint.bind_address != Ipv4Addr::LOCALHOST)
    {
        checks.push(Diagnostic {
            level: "OK".into(),
            check: "HTTPS isolation".into(),
            detail: format!(
                "LocalWeb owns {}:443; services on 127.0.0.1 remain untouched",
                endpoint.bind_address
            ),
            fix: None,
        });
    } else {
        checks.push(Diagnostic {
            level: "OK".into(),
            check: "standard HTTPS port".into(),
            detail: if gateway_endpoint
                .is_some_and(|endpoint| endpoint.https_port == GATEWAY_HTTPS_PORT)
            {
                "owned by the LocalWeb gateway".into()
            } else {
                "available for LocalWeb".into()
            },
            fix: None,
        });
    }
    let ca = CaPaths::new(&state.paths);
    let ca_exists = ca.cert.is_file() && ca.key.is_file();
    let system_ca_trusted = ca_exists
        && localweb_core::platform::TrustStore::detect().is_ok_and(|store| store.trusted(&ca.cert));
    let browser_ca_trusted = ca_exists && localweb_core::platform::browser_ca_trusted(&ca.cert);
    let ca_trusted = system_ca_trusted && browser_ca_trusted;
    checks.push(Diagnostic {
        level: if ca_trusted { "OK" } else { "WARNING" }.into(),
        check: "HTTPS certificate authority".into(),
        detail: if ca_trusted {
            "LocalWeb Development CA is trusted by the system and Chromium-family browsers".into()
        } else if system_ca_trusted {
            "CA is trusted by the operating system but not by Brave/Chromium".into()
        } else if browser_ca_trusted {
            "CA is trusted by Brave/Chromium but not by the operating system".into()
        } else if ca_exists {
            "CA exists but is not trusted by the system or Brave/Chromium".into()
        } else {
            "LocalWeb Development CA is missing".into()
        },
        fix: (!ca_trusted).then(|| "Run `localweb ca install`".into()),
    });
    let expected_address =
        selected_gateway.map_or(Ipv4Addr::LOCALHOST, |endpoint| endpoint.bind_address);
    let missing_hosts = load_projects(&state.paths)
        .unwrap_or_default()
        .into_iter()
        .filter(|project| !hosts_has_domain(&project.hostname, expected_address))
        .map(|project| project.hostname)
        .collect::<Vec<_>>();
    checks.push(Diagnostic {
        level: if missing_hosts.is_empty() {
            "OK"
        } else {
            "WARNING"
        }
        .into(),
        check: "exact host mappings".into(),
        detail: if missing_hosts.is_empty() {
            format!("all configured project hostnames map to {expected_address}")
        } else {
            format!("missing: {}", missing_hosts.join(", "))
        },
        fix: (!missing_hosts.is_empty()).then(|| "Run `localweb hosts sync`".into()),
    });
    checks.extend(runtime_diagnostics());
    checks
}

fn runtime_diagnostics() -> Vec<Diagnostic> {
    let mut checks = Vec::new();
    let runtime = RuntimeLayout::discover();
    #[cfg(target_os = "macos")]
    {
        checks.push(Diagnostic {
            level: "OK".into(),
            check: "runtime".into(),
            detail: "Homebrew-managed system packages".into(),
            fix: None,
        });
    }
    #[cfg(not(target_os = "macos"))]
    {
        match runtime.manifest() {
            Ok(manifest) => checks.push(Diagnostic {
                level: "OK".into(),
                check: "bundled runtime".into(),
                detail: format!(
                    "{} components for {}",
                    manifest.components.len(),
                    manifest.target
                ),
                fix: None,
            }),
            Err(error) => checks.push(Diagnostic {
                level: "ERROR".into(),
                check: "bundled runtime".into(),
                detail: error.to_string(),
                fix: Some("Reinstall the complete LocalWeb package".into()),
            }),
        }
    }
    let runtime_checks = [
        ("nginx", runtime.nginx().map(|_| ())),
        ("Apache", runtime.apache().map(|_| ())),
        ("PHP 8.5 CLI/FPM", runtime.php("8.5").map(|_| ())),
        ("Composer 2", runtime.composer("2").map(|_| ())),
        (
            "Node.js + npm",
            runtime.node().and(runtime.npm_cli()).map(|_| ()),
        ),
        ("MariaDB", runtime.mariadb().map(|_| ())),
        ("MySQL", runtime.mysql().map(|_| ())),
        ("PostgreSQL", runtime.postgres().map(|_| ())),
        ("Redis", runtime.redis().map(|_| ())),
        ("Mailpit", runtime.mailpit().map(|_| ())),
    ];
    for (component, result) in runtime_checks {
        let exists = result.is_ok();
        checks.push(Diagnostic {
            level: if exists { "OK" } else { "ERROR" }.into(),
            check: component.into(),
            detail: result.map_or_else(|error| error.to_string(), |()| "available".into()),
            fix: if exists {
                None
            } else {
                Some(
                    "Install the missing component (or reinstall the complete LocalWeb package)"
                        .into(),
                )
            },
        });
    }
    checks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_project_state_roundtrips_with_private_file() {
        let base = std::env::temp_dir().join(format!("localweb-state-{}", uuid::Uuid::new_v4()));
        let paths = Paths {
            config: base.join("config"),
            data: base.join("data"),
            state: base.join("state"),
            runtime: base.join("runtime"),
        };
        paths.create().unwrap();
        let expected = BTreeSet::from(["first".to_owned(), "second".to_owned()]);
        persist_running(&paths, &expected).unwrap();
        assert_eq!(load_desired_running(&paths).unwrap(), expected);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(paths.state.join("active-projects.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        };
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn health_check_rejects_server_errors() {
        assert!(healthy_response(
            b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n"
        ));
        assert!(healthy_response(b"HTTP/1.0 204 No Content\r\n"));
        assert!(healthy_response(b"HTTP/1.1 302 Found\r\n"));
        assert!(healthy_response(b"HTTP/1.1 401 Unauthorized\r\n"));
        assert!(healthy_response(b"HTTP/1.1 403 Forbidden\r\n"));
        assert!(!healthy_response(b"HTTP/1.1 404 Not Found\r\n"));
        assert!(!healthy_response(b"HTTP/1.1 500 Internal Server Error\r\n"));
        assert!(!healthy_response(b"HTTP/1.1 502 Bad Gateway\r\n"));
        assert!(!healthy_response(b"HTTP/1.1 503 Service Unavailable\r\n"));
        assert!(!healthy_response(b"garbage"));
        assert!(!healthy_response(b""));
        assert!(!healthy_response(b"HTTP/2 200 OK\r\n"));
        assert!(!healthy_response(b"HTTP/1.1 nope Bad\r\n"));
        assert!(!healthy_response(b"HTTP/1.1 200 OK"));
        assert!(!healthy_response(
            format!("HTTP/1.1 200 {}\r\n", "a".repeat(600)).as_bytes()
        ));
    }

    #[tokio::test]
    async fn probe_reads_a_fragmented_status_line() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 512];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            socket.write_all(b"HTTP/1.1 20").await.unwrap();
            tokio::time::sleep(Duration::from_millis(20)).await;
            socket
                .write_all(b"0 OK\r\nContent-Length: 0\r\n\r\n")
                .await
                .unwrap();
        });
        let stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        assert_eq!(
            probe_http_status(stream, "example.test").await.unwrap(),
            Some(200)
        );
        server.await.unwrap();
    }

    #[tokio::test]
    async fn probe_returns_the_failure_status() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 512];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            socket
                .write_all(b"HTTP/1.1 500 Internal Server Error\r\n\r\n")
                .await
                .unwrap();
        });
        let stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .unwrap();
        let status = probe_http_status(stream, "example.test").await.unwrap();
        assert_eq!(status, Some(500));
        assert!(!healthy_status(status.unwrap()));
        server.await.unwrap();
    }

    #[test]
    fn gateway_url_includes_only_nonstandard_ports() {
        assert_eq!(
            gateway_url(
                "demo.test",
                GatewayState {
                    schema_version: 1,
                    bind_address: Ipv4Addr::new(127, 0, 0, 2),
                    https_port: 443,
                }
            ),
            "https://demo.test"
        );
        assert_eq!(
            gateway_url(
                "demo.test",
                GatewayState {
                    schema_version: 1,
                    bind_address: Ipv4Addr::new(127, 0, 0, 2),
                    https_port: 8_443,
                }
            ),
            "https://demo.test:8443"
        );
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn gateway_prefers_a_free_dedicated_loopback_address() {
        let endpoint = choose_gateway_endpoint(
            |address| address == Ipv4Addr::new(127, 0, 0, 4),
            |_, _| false,
        )
        .unwrap();
        assert_eq!(endpoint.bind_address, Ipv4Addr::new(127, 0, 0, 4));
        assert_eq!(endpoint.https_port, GATEWAY_HTTPS_PORT);
    }

    #[test]
    fn gateway_uses_a_high_port_only_after_standard_endpoints_fail() {
        let endpoint =
            choose_gateway_endpoint(|_| false, |_, port| port == GATEWAY_FALLBACK_START + 2)
                .unwrap();
        assert_eq!(endpoint.https_port, GATEWAY_FALLBACK_START + 2);
        assert!(endpoint.bind_address.is_loopback());
    }

    #[test]
    fn port_probe_detects_a_listening_socket() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let endpoint = GatewayState {
            schema_version: 1,
            bind_address: Ipv4Addr::LOCALHOST,
            https_port: port,
        };
        assert!(endpoint_in_use(endpoint));
        drop(listener);
        assert!(!endpoint_in_use(endpoint));
    }

    #[test]
    fn allocate_port_skips_busy_and_reserved_ports() {
        let busy = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let busy_port = busy.local_addr().unwrap().port();
        let reserved = if busy_port == WEB_PORT_START {
            WEB_PORT_START + 1
        } else {
            WEB_PORT_START
        };
        let used = BTreeSet::from([reserved]);
        let chosen = allocate_port(WEB_PORT_START, WEB_PORT_END, &used).unwrap();
        assert_ne!(chosen, busy_port);
        assert!(!used.contains(&chosen));
    }

    #[test]
    fn persist_failure_is_reported_instead_of_silently_ignored() {
        let base = std::env::temp_dir().join(format!("localweb-persist-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(base.join("config")).unwrap();
        fs::create_dir_all(base.join("data")).unwrap();
        fs::create_dir_all(base.join("runtime")).unwrap();
        fs::write(base.join("state"), b"not a directory").unwrap();
        let paths = Paths {
            config: base.join("config"),
            data: base.join("data"),
            state: base.join("state"),
            runtime: base.join("runtime"),
        };
        assert!(persist_running(&paths, &BTreeSet::from(["demo".to_owned()])).is_err());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn rollback_errors_are_combined_with_the_primary_error() {
        let primary = LocalWebError::Config("primary".into());
        let combined = combine_rollback(
            primary,
            Err(LocalWebError::Service("compensation failed".into())),
        );
        let message = combined.to_string();
        assert!(message.contains("primary"));
        assert!(message.contains("compensation failed"));
    }

    #[test]
    fn runtime_list_reports_bundled_components() {
        let checks = runtime_diagnostics();
        assert_eq!(checks.len(), 11);
        assert_eq!(checks[0].check, "bundled runtime");
        let names: Vec<_> = checks.iter().skip(1).map(|c| c.check.as_str()).collect();
        assert_eq!(
            names,
            [
                "nginx",
                "Apache",
                "PHP 8.5 CLI/FPM",
                "Composer 2",
                "Node.js + npm",
                "MariaDB",
                "MySQL",
                "PostgreSQL",
                "Redis",
                "Mailpit"
            ]
        );
    }
}

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use localweb_core::{
    Cache, Database, ExecutionSpec, NewProject, Paths, Profile, Project, Request, Response,
    SupervisorConfig, WebServer, certificate::CaPaths, hosts::HostsSync, send_request,
};
use std::{
    ffi::OsString,
    io::{BufRead, IsTerminal, Write},
    path::PathBuf,
    process::Command,
};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[derive(Parser)]
#[command(
    name = "localweb",
    version,
    about = "Native Linux local web development manager"
)]
struct Cli {
    #[command(subcommand)]
    command: CommandKind,
}

#[expect(
    clippy::large_enum_variant,
    reason = "clap subcommands carry every project option in one place"
)]
#[derive(Subcommand)]
enum CommandKind {
    Setup,
    Start,
    Shutdown,
    Status {
        #[arg(long)]
        json: bool,
    },
    Doctor,
    Php {
        name: String,
        #[arg(last = true)]
        args: Vec<OsString>,
    },
    Composer {
        name: String,
        #[arg(last = true)]
        args: Vec<OsString>,
    },
    Node {
        name: String,
        #[arg(last = true)]
        args: Vec<OsString>,
    },
    Npm {
        name: String,
        #[arg(last = true)]
        args: Vec<OsString>,
    },
    Npx {
        name: String,
        #[arg(last = true)]
        args: Vec<OsString>,
    },
    Exec {
        name: String,
        #[arg(last = true, required = true)]
        command: Vec<OsString>,
    },
    Shell {
        name: String,
    },
    Env {
        name: String,
        #[arg(long, value_enum, default_value_t = EnvFormat::Human)]
        format: EnvFormat,
    },
    Ca {
        #[command(subcommand)]
        command: CaCommand,
    },
    Hosts {
        #[command(subcommand)]
        command: HostsCommand,
    },
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    Autostart {
        #[command(subcommand)]
        command: Toggle,
    },
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommand,
    },
    Data {
        #[command(subcommand)]
        command: DataCommand,
    },
}

#[derive(Subcommand)]
enum DataCommand {
    /// Permanently delete this account's `LocalWeb` data after confirmation.
    Purge {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum Toggle {
    Enable,
    Disable,
}

#[derive(Subcommand)]
enum CaCommand {
    Status,
    Install,
    Remove,
}

#[derive(Subcommand)]
enum HostsCommand {
    Sync,
}

#[derive(Subcommand)]
enum RuntimeCommand {
    List,
}

#[derive(Subcommand)]
enum ProjectCommand {
    List {
        #[arg(long)]
        json: bool,
    },
    #[command(alias = "create")]
    Add {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
        #[arg(long = "domain")]
        hostname: Option<String>,
        #[arg(long = "document-root")]
        document_root: Option<PathBuf>,
        #[arg(long = "web")]
        web_server: Option<WebArg>,
        /// PHP version for both CLI and FPM.
        #[arg(long, value_parser = localweb_core::PHP_VERSIONS)]
        php: Option<String>,
        /// Node.js version; 24 is included, 20/22 need a matching installation.
        #[arg(long, value_parser = ["20", "22", "24"])]
        node: Option<String>,
        #[arg(long = "db")]
        database: Option<DbArg>,
        #[arg(long)]
        cache: Option<CacheArg>,
        /// Composer major version (currently 2 only).
        #[arg(long, value_parser = ["2"])]
        composer: Option<String>,
        /// Enable the detected Laravel or Symfony worker preset (other apps need a custom command).
        #[arg(long)]
        supervisor: bool,
        /// Worker executable. Use @php for the project's selected PHP CLI.
        #[arg(long = "supervisor-command")]
        supervisor_command: Option<String>,
        /// One literal worker argument; repeat for multiple arguments.
        #[arg(
            long = "supervisor-arg",
            allow_hyphen_values = true,
            requires = "supervisor_command"
        )]
        supervisor_args: Vec<String>,
        /// One scheduled task (`"*/5 * * * * @php artisan schedule:run"`);
        /// repeat for multiple tasks. Runs while the project is active.
        #[arg(long = "cron")]
        cron: Vec<String>,
        /// Use the framework's conventional schedule when no --cron is given.
        #[arg(long = "cron-preset")]
        cron_preset: bool,
        /// Run a private Mailpit instance for this project (dev email).
        #[arg(long)]
        mail: bool,
        /// Fixed loopback web port; a free port is assigned when omitted.
        #[arg(long = "web-port", value_parser = clap::value_parser!(u16).range(1024..))]
        web_port: Option<u16>,
        /// Fixed loopback database port; a free port is assigned when omitted.
        #[arg(long = "db-port", value_parser = clap::value_parser!(u16).range(1024..))]
        database_port: Option<u16>,
        #[arg(long)]
        autostart: bool,
    },
    #[command(alias = "show")]
    Info {
        name: String,
        #[arg(long)]
        json: bool,
    },
    #[command(alias = "edit")]
    Update {
        name: String,
        #[arg(long = "document-root")]
        document_root: Option<PathBuf>,
        #[arg(long)]
        profile: Option<ProfileArg>,
        #[arg(long = "web")]
        web_server: Option<WebArg>,
        /// PHP version for both CLI and FPM.
        #[arg(long, value_parser = localweb_core::PHP_VERSIONS)]
        php: Option<String>,
        /// Node.js version; 24 is included, 20/22 need a matching installation.
        #[arg(long, value_parser = ["20", "22", "24"])]
        node: Option<String>,
        #[arg(long = "db")]
        database: Option<DbArg>,
        #[arg(long)]
        cache: Option<CacheArg>,
        /// Composer major version (currently 2 only).
        #[arg(long, value_parser = ["2"])]
        composer: Option<String>,
        /// Enable the detected Laravel or Symfony worker preset.
        #[arg(long, conflicts_with = "no_supervisor")]
        supervisor: bool,
        /// Disable the project worker.
        #[arg(long = "no-supervisor", conflicts_with = "supervisor")]
        no_supervisor: bool,
        /// Replace the worker executable; implies --supervisor.
        #[arg(long = "supervisor-command", conflicts_with = "no_supervisor")]
        supervisor_command: Option<String>,
        /// One literal worker argument; repeat for multiple arguments.
        #[arg(
            long = "supervisor-arg",
            allow_hyphen_values = true,
            requires = "supervisor_command"
        )]
        supervisor_args: Vec<String>,
        /// Replace the enabled scheduled tasks; repeat for multiple tasks.
        #[arg(long = "cron", conflicts_with = "clear_cron")]
        cron: Vec<String>,
        /// Use the framework's conventional schedule when no --cron is given.
        #[arg(long = "cron-preset", conflicts_with = "clear_cron")]
        cron_preset: bool,
        /// Remove every scheduled task, including disabled ones.
        #[arg(long = "clear-cron", conflicts_with_all = ["cron", "cron_preset"])]
        clear_cron: bool,
        /// Run a private Mailpit instance for this project.
        #[arg(long, conflicts_with = "no_mail")]
        mail: bool,
        /// Disable Mailpit for this project.
        #[arg(long = "no-mail", conflicts_with = "mail")]
        no_mail: bool,
        /// Start the project on login.
        #[arg(long, conflicts_with = "no_autostart")]
        autostart: bool,
        /// Do not start the project on login.
        #[arg(long = "no-autostart", conflicts_with = "autostart")]
        no_autostart: bool,
        #[arg(long)]
        json: bool,
    },
    Start {
        name: String,
    },
    Stop {
        name: String,
    },
    Restart {
        name: String,
    },
    #[command(alias = "delete")]
    Remove {
        name: String,
    },
}

#[derive(Clone, Debug, Default)]
enum SupervisorUpdate {
    #[default]
    Keep,
    Enable,
    Disable,
    Command(Vec<String>),
}

#[derive(Default)]
struct UpdateOptions {
    document_root: Option<PathBuf>,
    profile: Option<Profile>,
    web_server: Option<WebServer>,
    php: Option<String>,
    node: Option<String>,
    database: Option<Database>,
    cache: Option<Cache>,
    composer: Option<String>,
    supervisor: SupervisorUpdate,
    cron: Option<Vec<localweb_core::CronEntry>>,
    cron_preset: bool,
    clear_cron: bool,
    mail: Option<bool>,
    autostart: Option<bool>,
}

/// Merges the flags supplied to `project update` with the existing project.
///
/// `Project::from_update` receives a complete input so unset flags keep their
/// current values instead of falling back to global defaults.
fn update_input(existing: &Project, options: UpdateOptions) -> NewProject {
    let supervisor = match options.supervisor {
        SupervisorUpdate::Keep => existing.supervisor.clone(),
        SupervisorUpdate::Enable => SupervisorConfig {
            enabled: true,
            command: existing.supervisor.command.clone(),
        },
        SupervisorUpdate::Disable => SupervisorConfig {
            enabled: false,
            command: Vec::new(),
        },
        SupervisorUpdate::Command(command) => SupervisorConfig {
            enabled: true,
            command,
        },
    };
    let cron = options.cron.unwrap_or_else(|| {
        existing
            .cron
            .iter()
            .filter(|entry| entry.enabled)
            .cloned()
            .collect()
    });
    NewProject {
        path: existing.path.clone(),
        name: Some(existing.name.clone()),
        hostname: Some(existing.hostname.clone()),
        web_port: existing.web_port,
        database_port: existing.database_port,
        document_root: options
            .document_root
            .or_else(|| Some(existing.document_root.clone())),
        profile: options.profile.or(Some(existing.profile)),
        web_server: options.web_server.or(Some(existing.web_server)),
        php: options.php.or_else(|| Some(existing.php.clone())),
        composer: options.composer.or_else(|| Some(existing.composer.clone())),
        node: options.node.or_else(|| Some(existing.node.clone())),
        database: options.database.or(Some(existing.database)),
        cache: options.cache.or(Some(existing.cache)),
        supervisor,
        cron,
        cron_preset: options.cron_preset,
        clear_cron: options.clear_cron,
        mail: options.mail.unwrap_or(existing.mail),
        autostart: options.autostart.unwrap_or(existing.autostart),
    }
}

async fn project_status(paths: &Paths, name: &str) -> Result<localweb_core::ProjectStatus> {
    let response = send_request(
        paths,
        Request::ProjectGet {
            name: name.to_owned(),
        },
    )
    .await
    .context("cannot reach localwebd; run `localweb start` first")?;
    match response {
        Response::Project(status) => Ok(status),
        Response::Error { code, message } => bail!("{code}: {message}"),
        _ => bail!("unexpected daemon response"),
    }
}

#[derive(Clone, Copy, ValueEnum)]
enum ProfileArg {
    #[value(name = "genericphp", alias = "generic-php")]
    GenericPhp,
    Laravel,
    Symfony,
    Drupal,
    Wordpress,
    #[value(name = "processwire", alias = "process-wire")]
    ProcessWire,
}
impl From<ProfileArg> for Profile {
    fn from(value: ProfileArg) -> Self {
        match value {
            ProfileArg::GenericPhp => Self::GenericPhp,
            ProfileArg::Laravel => Self::Laravel,
            ProfileArg::Symfony => Self::Symfony,
            ProfileArg::Drupal => Self::Drupal,
            ProfileArg::Wordpress => Self::Wordpress,
            ProfileArg::ProcessWire => Self::ProcessWire,
        }
    }
}

#[derive(Clone, ValueEnum)]
enum WebArg {
    Nginx,
    Apache,
}
#[derive(Clone, ValueEnum)]
enum DbArg {
    None,
    Mariadb,
    Mysql,
    Postgres,
    Sqlite,
}
#[derive(Clone, ValueEnum)]
enum CacheArg {
    None,
    Redis,
}

#[derive(Clone, Copy, ValueEnum)]
enum EnvFormat {
    Human,
    Dotenv,
}
impl From<WebArg> for WebServer {
    fn from(value: WebArg) -> Self {
        match value {
            WebArg::Nginx => Self::Nginx,
            WebArg::Apache => Self::Apache,
        }
    }
}
impl From<DbArg> for Database {
    fn from(value: DbArg) -> Self {
        match value {
            DbArg::None => Self::None,
            DbArg::Mariadb => Self::Mariadb,
            DbArg::Mysql => Self::Mysql,
            DbArg::Postgres => Self::Postgres,
            DbArg::Sqlite => Self::Sqlite,
        }
    }
}
impl From<CacheArg> for Cache {
    fn from(value: CacheArg) -> Self {
        match value {
            CacheArg::None => Self::None,
            CacheArg::Redis => Self::Redis,
        }
    }
}

#[tokio::main]
#[expect(
    clippy::too_many_lines,
    reason = "CLI dispatch mirrors the public command tree"
)]
async fn main() -> Result<()> {
    if let Some(tool) = std::env::args_os()
        .next()
        .and_then(|a| PathBuf::from(a).file_name().map(OsString::from))
        .and_then(|name| name.into_string().ok())
        .filter(|tool| matches!(tool.as_str(), "composer" | "node" | "npm" | "npx"))
    {
        let name =
            std::env::var("LOCALWEB_PROJECT").context("tool shim requires LOCALWEB_PROJECT")?;
        let args = std::env::args_os().skip(1).collect();
        let kind = match tool.as_str() {
            "node" => ExecutionKind::Node(args),
            "npm" => ExecutionKind::Npm(args),
            "npx" => ExecutionKind::Npx(args),
            _ => ExecutionKind::Composer(args),
        };
        return execute_project(&Paths::discover()?, &name, kind).await;
    }
    let cli = Cli::parse();
    let paths = Paths::discover()?;
    let mut json = false;
    let mut sync_after = false;
    let mut env_format = None;
    let request = match cli.command {
        CommandKind::Setup => {
            setup(&paths)?;
            return Ok(());
        }
        CommandKind::Ca {
            command: CaCommand::Install,
        } => {
            install_ca(&paths)?;
            return Ok(());
        }
        CommandKind::Ca {
            command: CaCommand::Remove,
        } => {
            remove_ca(&paths)?;
            return Ok(());
        }
        CommandKind::Hosts {
            command: HostsCommand::Sync,
        } => {
            sync_hosts(&paths).await?;
            return Ok(());
        }
        CommandKind::Php { name, args } => {
            return execute_project(&paths, &name, ExecutionKind::Php(args)).await;
        }
        CommandKind::Composer { name, args } => {
            return execute_project(&paths, &name, ExecutionKind::Composer(args)).await;
        }
        CommandKind::Node { name, args } => {
            return execute_project(&paths, &name, ExecutionKind::Node(args)).await;
        }
        CommandKind::Npm { name, args } => {
            return execute_project(&paths, &name, ExecutionKind::Npm(args)).await;
        }
        CommandKind::Npx { name, args } => {
            return execute_project(&paths, &name, ExecutionKind::Npx(args)).await;
        }
        CommandKind::Exec { name, command } => {
            return execute_project(&paths, &name, ExecutionKind::Exec(command)).await;
        }
        CommandKind::Shell { name } => {
            return execute_project(&paths, &name, ExecutionKind::Shell).await;
        }
        CommandKind::Env { name, format } => {
            env_format = Some(format);
            Request::ProjectEnvironment { name }
        }
        CommandKind::Start => {
            start_daemon()?;
            return Ok(());
        }
        CommandKind::Autostart { command } => {
            let (enable, action) = match command {
                Toggle::Enable => (true, "enable"),
                Toggle::Disable => (false, "disable"),
            };
            localweb_core::service::set_autostart(enable)?;
            println!("LocalWeb login autostart {action}d");
            return Ok(());
        }
        CommandKind::Shutdown => Request::Shutdown,
        CommandKind::Status { json: enabled } => {
            json = enabled;
            Request::Status
        }
        CommandKind::Doctor => Request::Doctor,
        CommandKind::Runtime {
            command: RuntimeCommand::List,
        } => Request::RuntimeList,
        CommandKind::Data {
            command: DataCommand::Purge { dry_run, yes },
        } => {
            data_purge(&paths, dry_run, yes).await?;
            return Ok(());
        }
        CommandKind::Ca {
            command: CaCommand::Status,
        } => Request::CaStatus,
        CommandKind::Project { command } => match command {
            ProjectCommand::List { json: enabled } => {
                json = enabled;
                Request::ProjectList
            }
            ProjectCommand::Add {
                path,
                name,
                hostname,
                document_root,
                web_server,
                php,
                node,
                database,
                cache,
                composer,
                supervisor,
                supervisor_command,
                supervisor_args,
                cron,
                cron_preset,
                mail,
                web_port,
                database_port,
                autostart,
            } => {
                sync_after = true;
                let supervisor_enabled = supervisor || supervisor_command.is_some();
                let mut supervisor_command = supervisor_command.into_iter().collect::<Vec<_>>();
                supervisor_command.extend(supervisor_args);
                let mut cron_entries = Vec::new();
                for line in cron {
                    cron_entries.push(localweb_core::parse_cron_line(&line)?);
                }
                Request::ProjectAdd(NewProject {
                    path,
                    name,
                    hostname,
                    document_root,
                    profile: None,
                    web_server: web_server.map(Into::into),
                    php,
                    node,
                    composer,
                    database: database.map(Into::into),
                    cache: cache.map(Into::into),
                    supervisor: SupervisorConfig {
                        enabled: supervisor_enabled,
                        command: supervisor_command,
                    },
                    cron: cron_entries,
                    cron_preset,
                    clear_cron: false,
                    mail,
                    web_port,
                    database_port,
                    autostart,
                })
            }
            ProjectCommand::Info {
                name,
                json: enabled,
            } => {
                json = enabled;
                Request::ProjectGet { name }
            }
            ProjectCommand::Update {
                name,
                document_root,
                profile,
                web_server,
                php,
                node,
                database,
                cache,
                composer,
                supervisor,
                no_supervisor,
                supervisor_command,
                supervisor_args,
                cron,
                cron_preset,
                clear_cron,
                mail,
                no_mail,
                autostart,
                no_autostart,
                json: enabled,
            } => {
                json = enabled;
                sync_after = true;
                let existing = project_status(&paths, &name).await?;
                let cron = if clear_cron || (cron_preset && cron.is_empty()) {
                    Some(Vec::new())
                } else if cron.is_empty() {
                    None
                } else {
                    let mut entries = Vec::new();
                    for line in &cron {
                        entries.push(localweb_core::parse_cron_line(line)?);
                    }
                    Some(entries)
                };
                let supervisor = if let Some(command) = supervisor_command {
                    let mut command = vec![command];
                    command.extend(supervisor_args);
                    SupervisorUpdate::Command(command)
                } else if no_supervisor {
                    SupervisorUpdate::Disable
                } else if supervisor {
                    SupervisorUpdate::Enable
                } else {
                    SupervisorUpdate::Keep
                };
                let options = UpdateOptions {
                    document_root,
                    profile: profile.map(Into::into),
                    web_server: web_server.map(Into::into),
                    php,
                    node,
                    database: database.map(Into::into),
                    cache: cache.map(Into::into),
                    composer,
                    supervisor,
                    cron,
                    cron_preset,
                    clear_cron,
                    mail: if mail {
                        Some(true)
                    } else if no_mail {
                        Some(false)
                    } else {
                        None
                    },
                    autostart: if autostart {
                        Some(true)
                    } else if no_autostart {
                        Some(false)
                    } else {
                        None
                    },
                };
                Request::ProjectUpdate {
                    name,
                    input: update_input(&existing.project, options),
                }
            }
            ProjectCommand::Start { name } => Request::ProjectStart { name },
            ProjectCommand::Stop { name } => Request::ProjectStop { name },
            ProjectCommand::Restart { name } => Request::ProjectRestart { name },
            ProjectCommand::Remove { name } => {
                sync_after = true;
                Request::ProjectRemove { name }
            }
        },
    };
    let response = send_request(&paths, request)
        .await
        .context("cannot reach localwebd; run `localweb start` first")?;
    if let Response::Error { code, message } = &response {
        bail!("{code}: {message}");
    }
    if sync_after {
        sync_hosts(&paths).await.context("project configuration saved, but /etc/hosts synchronization failed; run `localweb hosts sync`")?;
    }
    match response {
        other if json => println!("{}", serde_json::to_string_pretty(&other)?),
        Response::Status(status) => {
            let gateway = if status.gateway_running {
                match (status.gateway_bind_address, status.gateway_https_port) {
                    (Some(address), Some(443)) => format!("running on https://{address}"),
                    (Some(address), Some(port)) => {
                        format!("running on https://{address}:{port}")
                    }
                    _ => "running".to_owned(),
                }
            } else {
                "stopped".to_owned()
            };
            println!(
                "Daemon: running\nGateway: {gateway}\nProjects: {} total, {} active",
                status.project_count, status.active_projects
            );
        }
        Response::Projects(projects) => {
            for item in projects {
                println!(
                    "{:<20} {:<30} {:<8} {}",
                    item.project.name,
                    item.project.hostname,
                    if item.running { "running" } else { "stopped" },
                    item.project.path.display()
                );
            }
        }
        Response::Project(item) => {
            let direct_url = item
                .project
                .web_port
                .map_or_else(|| "-".to_owned(), |port| format!("http://127.0.0.1:{port}"));
            println!(
                "{} ({})\nPath: {}\nDocument root: {}\nPHP: {}\nWeb: {:?}\nPublic URL: {}\nDirect URL: {}\nDatabase: {:?}\nDatabase URL: {}\nCache: {:?}\nSupervisor: {}\nState: {}",
                item.project.name,
                item.project.hostname,
                item.project.path.display(),
                item.project.document_root.display(),
                item.project.php,
                item.project.web_server,
                item.public_url.as_deref().unwrap_or("gateway stopped"),
                direct_url,
                item.project.database,
                item.project
                    .database_port
                    .map_or_else(|| "-".to_owned(), |port| format!("127.0.0.1:{port}")),
                item.project.cache,
                if item.project.supervisor.enabled {
                    "enabled"
                } else {
                    "disabled"
                },
                if item.running { "running" } else { "stopped" }
            );
        }
        Response::Message(message) => println!("{message}"),
        Response::Ca(status) => println!(
            "CA: {}\nSystem trust: {}\nBrave/Chromium trust: {}\nCertificate: {}",
            if status.exists { "present" } else { "missing" },
            if status.system_trusted {
                "installed"
            } else {
                "not installed"
            },
            if status.browser_trusted {
                "installed"
            } else {
                "not installed"
            },
            status.certificate.display()
        ),
        Response::Execution(_) => bail!("unexpected execution response"),
        Response::Environment(values) => {
            for (key, value) in values {
                match env_format.unwrap_or(EnvFormat::Human) {
                    EnvFormat::Human => println!("{key}: {value}"),
                    EnvFormat::Dotenv => println!("{key}={value}"),
                }
            }
        }
        Response::Error { .. } => unreachable!("errors were handled before rendering"),
        Response::Diagnostics(items) => {
            for item in items {
                println!("{:<7} {:<20} {}", item.level, item.check, item.detail);
                if let Some(fix) = item.fix {
                    println!("        fix: {fix}");
                }
            }
        }
    }
    Ok(())
}

fn start_daemon() -> Result<()> {
    // Restart so a stale daemon left behind by a package upgrade is replaced.
    localweb_core::service::restart_daemon()?;
    println!("LocalWeb daemon started");
    Ok(())
}

fn setup(paths: &Paths) -> Result<()> {
    paths.create()?;
    let _ = localweb_core::GlobalConfig::load_or_create(paths)?;
    CaPaths::new(paths).ensure_ca()?;
    register_paths(paths)?;
    localweb_core::service::write_user_units(paths)?;
    if cfg!(target_os = "macos") {
        println!(
            "LocalWeb directories, CA, and launch agents installed. Run `localweb ca install`, then `localweb start`. Login autostart is enabled; disable it with `localweb autostart disable`."
        );
    } else {
        println!(
            "LocalWeb directories, CA, and user service installed. Run `localweb ca install`, then `localweb start`. Login autostart remains disabled."
        );
    }
    Ok(())
}

fn register_paths(paths: &Paths) -> Result<()> {
    let payload = serde_json::to_vec(&serde_json::json!({
        "config": paths.config,
        "data": paths.data,
        "state": paths.state,
        "runtime": paths.runtime,
    }))?;
    run_helper("register-paths", &payload)
}

enum ExecutionKind {
    Php(Vec<OsString>),
    Composer(Vec<OsString>),
    Node(Vec<OsString>),
    Npm(Vec<OsString>),
    Npx(Vec<OsString>),
    Exec(Vec<OsString>),
    Shell,
}

async fn execute_project(paths: &Paths, name: &str, kind: ExecutionKind) -> Result<()> {
    let response = send_request(paths, Request::ResolveExecution { name: name.into() })
        .await
        .context("cannot resolve project runtime; start localwebd first")?;
    let spec = match response {
        Response::Execution(spec) => spec,
        Response::Error { code, message } => bail!("{code}: {message}"),
        _ => bail!("unexpected daemon response"),
    };
    let status = run_execution(name, spec, kind)?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn run_execution(
    name: &str,
    spec: ExecutionSpec,
    kind: ExecutionKind,
) -> Result<std::process::ExitStatus> {
    let mut paths = vec![spec.shim_dir];
    paths.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(paths).context("cannot build the project PATH")?;
    let mut command = match kind {
        ExecutionKind::Php(args) => {
            let mut cmd = Command::new(&spec.php);
            if let Some(ini) = &spec.php_ini {
                cmd.arg("-n").arg("-c").arg(ini);
            }
            cmd.args(args);
            cmd
        }
        ExecutionKind::Composer(args) => {
            let phar = spec
                .composer_phar
                .context("Composer 2 is not installed in LocalWeb runtime storage")?;
            let mut cmd = Command::new(&spec.php);
            if let Some(ini) = &spec.php_ini {
                cmd.arg("-n").arg("-c").arg(ini);
            }
            cmd.arg(phar).args(args);
            cmd
        }
        ExecutionKind::Node(args) => {
            let node = spec
                .node
                .context("Node.js is not available for this project")?;
            let mut cmd = Command::new(&node);
            cmd.args(args);
            cmd
        }
        ExecutionKind::Npm(args) => {
            let node = spec
                .node
                .context("Node.js is not available for this project")?;
            let npm = spec
                .npm_cli
                .context("npm is not available for this project")?;
            let mut cmd = Command::new(&node);
            cmd.arg(npm).args(args);
            cmd
        }
        ExecutionKind::Npx(args) => {
            let node = spec
                .node
                .context("Node.js is not available for this project")?;
            let npm = spec
                .npm_cli
                .context("npm is not available for this project")?;
            let npx = npm.with_file_name("npx-cli.js");
            if !npx.is_file() {
                bail!("npx is not available for this project");
            }
            let mut cmd = Command::new(&node);
            cmd.arg(npx).args(args);
            cmd
        }
        ExecutionKind::Exec(args) => {
            let mut args = args.into_iter();
            let program = args.next().context("missing command after --")?;
            let mut cmd = Command::new(program);
            cmd.args(args);
            cmd
        }
        ExecutionKind::Shell => {
            #[cfg(windows)]
            let shell = std::env::var_os("COMSPEC").unwrap_or_else(|| OsString::from("cmd.exe"));
            #[cfg(not(windows))]
            let shell = std::env::var_os("SHELL").unwrap_or_else(|| OsString::from("/bin/sh"));
            Command::new(shell)
        }
    };
    command
        .current_dir(&spec.project_path)
        .env("PATH", path)
        .env("LOCALWEB_PROJECT", name);
    if let Some(library_path) = spec.library_path {
        command.env("LD_LIBRARY_PATH", library_path);
    }
    if let Some(php_ini) = spec.php_ini {
        command.env("PHPRC", php_ini);
    }
    command.status().context("failed to launch project command")
}

fn helper_path() -> Result<PathBuf> {
    #[cfg(unix)]
    {
        let installed = PathBuf::from("/usr/lib/localweb/localweb-helper");
        if installed.is_file() {
            return Ok(installed);
        }
    }
    let development = std::env::current_exe()?
        .with_file_name(format!("localweb-helper{}", std::env::consts::EXE_SUFFIX));
    if development.is_file() {
        return Ok(development);
    }
    bail!("localweb-helper is missing; install the LocalWeb package")
}

fn run_helper(operation: &str, input: &[u8]) -> Result<()> {
    let helper = helper_path()?;
    Ok(localweb_core::privilege::run_helper(
        &helper, operation, input,
    )?)
}

async fn data_purge(paths: &Paths, dry_run: bool, yes: bool) -> Result<()> {
    if localweb_core::send_request(paths, Request::Status)
        .await
        .is_ok()
    {
        bail!("the LocalWeb daemon is running; run `localweb shutdown` before purging data");
    }
    #[cfg(unix)]
    if localweb_core::current_uid() == 0 {
        bail!("run `localweb data purge` as the account that owns the data, not as root");
    }
    if CaPaths::new(paths).cert.is_file() {
        println!(
            "note: the LocalWeb CA is still installed in the system trust store; run `localweb ca remove` first if you want to revoke it"
        );
    }
    let targets = [
        ("configuration", paths.config.clone()),
        ("data", paths.data.clone()),
        ("state", paths.state.clone()),
        ("runtime", paths.runtime.clone()),
    ];
    #[cfg(unix)]
    let current_uid = localweb_core::current_uid();
    let mut existing = Vec::new();
    for (label, path) in targets {
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(error).with_context(|| format!("cannot inspect {}", path.display()));
            }
        };
        if metadata.file_type().is_symlink() {
            bail!(
                "refusing to purge {} because it is a symbolic link",
                path.display()
            );
        }
        if !metadata.is_dir() {
            bail!(
                "refusing to purge {} because it is not a directory",
                path.display()
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            if metadata.uid() != current_uid {
                bail!(
                    "refusing to purge {} because it is owned by uid {}, not by the current user",
                    path.display(),
                    metadata.uid()
                );
            }
        }
        let size = directory_size(&path);
        existing.push((label, path, size));
    }
    if existing.is_empty() {
        println!("No LocalWeb data directories exist for this account");
        return Ok(());
    }
    println!("LocalWeb data for this account:");
    for (label, path, size) in &existing {
        println!("  {label:14} {:>10}  {}", human_size(*size), path.display());
    }
    if dry_run {
        println!("Dry run: nothing was deleted");
        return Ok(());
    }
    if !yes {
        if !std::io::stdin().is_terminal() {
            bail!("refusing to purge without --yes (standard input is not a terminal)");
        }
        print!("Type 'purge' to delete these directories permanently: ");
        std::io::stdout().flush()?;
        let mut answer = String::new();
        std::io::stdin().lock().read_line(&mut answer)?;
        if answer.trim() != "purge" {
            bail!("purge cancelled");
        }
    }
    for (label, path, _) in &existing {
        std::fs::remove_dir_all(path)
            .with_context(|| format!("cannot remove {label} directory {}", path.display()))?;
        println!("Removed {label}: {}", path.display());
    }
    println!("LocalWeb data for this account was purged");
    Ok(())
}

fn directory_size(path: &std::path::Path) -> u64 {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return 0;
    };
    if metadata.file_type().is_symlink() {
        return 0;
    }
    if metadata.is_file() {
        return metadata.len();
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| directory_size(&entry.path()))
        .sum()
}

#[expect(
    clippy::cast_precision_loss,
    reason = "human-readable sizes do not need exact integer precision"
)]
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn install_ca(paths: &Paths) -> Result<()> {
    let ca = CaPaths::new(paths);
    ca.ensure_ca()?;
    run_helper("install-ca", &std::fs::read(&ca.cert)?)?;
    localweb_core::platform::install_browser_ca(&ca.cert)?;
    println!("LocalWeb Development CA installed for the system and Chromium-family browsers");
    Ok(())
}

fn remove_ca(paths: &Paths) -> Result<()> {
    let ca = CaPaths::new(paths);
    if !ca.cert.is_file() {
        bail!("this account has no LocalWeb CA");
    }
    localweb_core::platform::remove_browser_ca()?;
    run_helper("remove-ca", &std::fs::read(&ca.cert)?)?;
    println!("LocalWeb Development CA removed from system and browser trust stores");
    Ok(())
}

async fn sync_hosts(paths: &Paths) -> Result<()> {
    let projects = match send_request(paths, Request::ProjectList).await? {
        Response::Projects(items) => items,
        Response::Error { code, message } => bail!("{code}: {message}"),
        _ => bail!("unexpected daemon response"),
    };
    let names: Vec<_> = projects
        .into_iter()
        .map(|item| item.project.hostname)
        .collect();
    let address = match send_request(paths, Request::Status).await? {
        Response::Status(status) => status
            .gateway_bind_address
            .unwrap_or(localweb_core::hosts::DEFAULT_GATEWAY_ADDRESS),
        Response::Error { code, message } => bail!("{code}: {message}"),
        _ => bail!("unexpected daemon response"),
    };
    let request = HostsSync {
        address,
        domains: names,
    };
    run_helper("sync-hosts", &serde_json::to_vec(&request)?)?;
    println!(
        "Synchronized {} exact LocalWeb hostnames to {address}",
        request.domains.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_uses_binary_units() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1024), "1.0 KiB");
        assert_eq!(human_size(1536), "1.5 KiB");
        assert_eq!(human_size(1024 * 1024), "1.0 MiB");
    }

    fn sample_project() -> (Project, PathBuf) {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "localweb-cli-update-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(path.join("public")).unwrap();
        let project = Project::from_new(
            NewProject {
                path: path.clone(),
                name: Some("cli-update".into()),
                hostname: Some("cli-update.test".into()),
                web_port: Some(9_123),
                database_port: Some(33_123),
                document_root: Some(PathBuf::from("public")),
                profile: Some(Profile::Laravel),
                web_server: Some(WebServer::Apache),
                php: Some("8.4".into()),
                composer: Some("2".into()),
                node: Some("22".into()),
                database: Some(Database::Mariadb),
                cache: Some(Cache::Redis),
                supervisor: SupervisorConfig {
                    enabled: true,
                    command: vec!["@php".into(), "artisan".into(), "queue:work".into()],
                },
                cron: vec![localweb_core::CronEntry {
                    enabled: true,
                    schedule: "*/5 * * * *".into(),
                    command: vec!["@php".into(), "artisan".into(), "schedule:run".into()],
                }],
                cron_preset: false,
                clear_cron: false,
                mail: true,
                autostart: true,
            },
            &localweb_core::GlobalConfig::default(),
        )
        .unwrap();
        (project, path)
    }

    #[test]
    fn update_merges_unset_flags_with_existing_project() {
        let (mut existing, path) = sample_project();
        existing.enabled = false;
        existing
            .environment
            .insert("APP_ENV".into(), "local".into());
        existing.cron.push(localweb_core::CronEntry {
            enabled: false,
            schedule: "0 3 * * *".into(),
            command: vec!["@php".into(), "artisan".into(), "queue:prune".into()],
        });
        let input = update_input(
            &existing,
            UpdateOptions {
                php: Some("8.3".into()),
                ..Default::default()
            },
        );
        let updated =
            Project::from_update(&existing, input, &localweb_core::GlobalConfig::default())
                .unwrap();
        assert_eq!(updated.id, existing.id);
        assert_eq!(updated.name, existing.name);
        assert_eq!(updated.path, existing.path);
        assert_eq!(updated.hostname, existing.hostname);
        assert_eq!(updated.web_port, existing.web_port);
        assert_eq!(updated.database_port, existing.database_port);
        assert_eq!(updated.document_root, existing.document_root);
        assert_eq!(updated.profile, existing.profile);
        assert_eq!(updated.web_server, existing.web_server);
        assert_eq!(updated.composer, existing.composer);
        assert_eq!(updated.node, existing.node);
        assert_eq!(updated.database, existing.database);
        assert_eq!(updated.cache, existing.cache);
        assert_eq!(updated.supervisor, existing.supervisor);
        assert_eq!(updated.mail, existing.mail);
        assert_eq!(updated.autostart, existing.autostart);
        assert_eq!(updated.enabled, existing.enabled);
        assert_eq!(updated.environment, existing.environment);
        assert_eq!(updated.created_at, existing.created_at);
        assert_eq!(updated.php, "8.3");
        assert_eq!(updated.cron.len(), 2);
        assert_eq!(
            updated.cron.iter().filter(|entry| !entry.enabled).count(),
            1
        );
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn update_can_disable_services_and_clear_cron() {
        let (existing, path) = sample_project();
        let input = update_input(
            &existing,
            UpdateOptions {
                supervisor: SupervisorUpdate::Disable,
                cron: Some(Vec::new()),
                clear_cron: true,
                mail: Some(false),
                autostart: Some(false),
                ..Default::default()
            },
        );
        let updated =
            Project::from_update(&existing, input, &localweb_core::GlobalConfig::default())
                .unwrap();
        assert!(!updated.supervisor.enabled);
        assert!(updated.cron.is_empty());
        assert!(!updated.mail);
        assert!(!updated.autostart);
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn update_enables_the_profile_worker_preset() {
        let (mut existing, path) = sample_project();
        existing.supervisor = SupervisorConfig::default();
        let input = update_input(
            &existing,
            UpdateOptions {
                supervisor: SupervisorUpdate::Enable,
                ..Default::default()
            },
        );
        let updated =
            Project::from_update(&existing, input, &localweb_core::GlobalConfig::default())
                .unwrap();
        assert!(updated.supervisor.enabled);
        assert_eq!(
            updated.supervisor.command,
            [
                "@php",
                "artisan",
                "queue:work",
                "--sleep=3",
                "--tries=3",
                "--timeout=90"
            ]
        );
        std::fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn update_flags_are_tri_state() {
        let cli = Cli::try_parse_from([
            "localweb",
            "project",
            "update",
            "app",
            "--no-mail",
            "--no-autostart",
            "--no-supervisor",
            "--clear-cron",
        ])
        .unwrap();
        match cli.command {
            CommandKind::Project {
                command:
                    ProjectCommand::Update {
                        no_mail,
                        no_autostart,
                        no_supervisor,
                        clear_cron,
                        ..
                    },
            } => {
                assert!(no_mail);
                assert!(no_autostart);
                assert!(no_supervisor);
                assert!(clear_cron);
            }
            _ => panic!("expected project update"),
        }
        assert!(
            Cli::try_parse_from([
                "localweb",
                "project",
                "update",
                "app",
                "--mail",
                "--no-mail"
            ])
            .is_err()
        );
        assert!(
            Cli::try_parse_from([
                "localweb",
                "project",
                "update",
                "app",
                "--cron-preset",
                "--clear-cron"
            ])
            .is_err()
        );
    }

    #[test]
    fn directory_size_ignores_symlinks() {
        let root = std::env::temp_dir().join(format!("localweb-cli-size-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("nested")).unwrap();
        std::fs::write(root.join("a"), b"1234").unwrap();
        std::fs::write(root.join("nested/b"), b"12").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("a"), root.join("link")).unwrap();
        assert_eq!(directory_size(&root), 6);
        std::fs::remove_dir_all(&root).unwrap();
    }
}

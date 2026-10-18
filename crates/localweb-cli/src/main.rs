use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use localweb_core::{
    Cache, Database, ExecutionSpec, NewProject, Paths, Request, Response, SupervisorConfig,
    WebServer, certificate::CaPaths, send_request,
};
use std::{ffi::OsString, path::PathBuf, process::Command};

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
#[expect(
    clippy::large_enum_variant,
    reason = "project creation carries every option by design"
)]
enum ProjectCommand {
    List {
        #[arg(long)]
        json: bool,
    },
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
    Info {
        name: String,
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
    Remove {
        name: String,
    },
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
        Response::Status(status) => println!(
            "Daemon: running\nGateway: {}\nProjects: {} total, {} active",
            if status.gateway_running {
                "running"
            } else {
                "stopped"
            },
            status.project_count,
            status.active_projects
        ),
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
        Response::Project(item) => println!(
            "{} ({})\nPath: {}\nDocument root: {}\nPHP: {}\nWeb: {:?}\nWeb URL: {}\nDatabase: {:?}\nDatabase URL: {}\nCache: {:?}\nSupervisor: {}\nState: {}",
            item.project.name,
            item.project.hostname,
            item.project.path.display(),
            item.project.document_root.display(),
            item.project.php,
            item.project.web_server,
            item.project
                .web_port
                .map_or_else(|| "-".to_owned(), |port| format!("http://127.0.0.1:{port}")),
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
        ),
        Response::Message(message) => println!("{message}"),
        Response::Ca(status) => println!(
            "CA: {}\nSystem trust copy: {}\nCertificate: {}",
            if status.exists { "present" } else { "missing" },
            if status.trusted {
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
    let mut path = OsString::from(spec.shim_dir.as_os_str());
    path.push(":");
    path.push(std::env::var_os("PATH").unwrap_or_default());
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
    let installed = PathBuf::from("/usr/lib/localweb/localweb-helper");
    if installed.is_file() {
        return Ok(installed);
    }
    let development = std::env::current_exe()?.with_file_name("localweb-helper");
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

fn install_ca(paths: &Paths) -> Result<()> {
    let ca = CaPaths::new(paths);
    ca.ensure_ca()?;
    run_helper("install-ca", &std::fs::read(&ca.cert)?)?;
    println!("LocalWeb Development CA installed in the system trust store");
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
    run_helper("sync-hosts", &serde_json::to_vec(&names)?)?;
    println!("Synchronized {} exact LocalWeb hostnames", names.len());
    Ok(())
}

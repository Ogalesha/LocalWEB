//! Defines shared `LocalWeb` models, validation, paths, and daemon protocol.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
use thiserror::Error;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};
use uuid::Uuid;

pub mod certificate;
pub mod hosts;
pub mod platform;
pub mod runtime;

pub const PROTOCOL_VERSION: u32 = 1;
pub const CONFIG_SCHEMA: u32 = 1;
pub const PHP_VERSIONS: [&str; 6] = ["8.0", "8.1", "8.2", "8.3", "8.4", "8.5"];
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Error)]
pub enum LocalWebError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error("project not found: {0}")]
    ProjectNotFound(String),
    #[error("hostname already assigned: {0}")]
    DomainConflict(String),
    #[error("runtime is not installed: {0}")]
    RuntimeNotInstalled(String),
    #[error("service failed: {0}")]
    Service(String),
    #[error("protocol error: {0}")]
    Protocol(String),
}

pub type Result<T> = std::result::Result<T, LocalWebError>;

#[derive(Debug, Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub data: PathBuf,
    pub state: PathBuf,
    pub runtime: PathBuf,
}

impl Paths {
    /// Resolves the current user's XDG directories.
    ///
    /// # Errors
    ///
    /// Returns an error when `HOME` or `XDG_RUNTIME_DIR` is unavailable.
    pub fn discover() -> Result<Self> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| LocalWebError::Config("HOME is unset".into()))?;
        let xdg = |key: &str, fallback: &str| {
            std::env::var_os(key)
                .map_or_else(|| home.join(fallback), PathBuf::from)
                .join("localweb")
        };
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .ok_or_else(|| {
                LocalWebError::Config(
                    "XDG_RUNTIME_DIR is unset; a user systemd session is required".into(),
                )
            })?
            .join("localweb");
        Ok(Self {
            config: xdg("XDG_CONFIG_HOME", ".config"),
            data: xdg("XDG_DATA_HOME", ".local/share"),
            state: xdg("XDG_STATE_HOME", ".local/state"),
            runtime,
        })
    }

    /// Creates private application directories with user-only permissions.
    ///
    /// # Errors
    ///
    /// Returns an error when a directory cannot be created or secured.
    pub fn create(&self) -> Result<()> {
        for path in [
            &self.config,
            &self.config.join("projects"),
            &self.config.join("generated"),
            &self.data,
            &self.data.join("runtimes"),
            &self.data.join("certs"),
            &self.data.join("databases"),
            &self.state,
            &self.state.join("logs"),
            &self.runtime,
            &self.runtime.join("php"),
            &self.runtime.join("services"),
        ] {
            fs::create_dir_all(path)?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }

    /// Returns the daemon socket path.
    #[must_use]
    pub fn socket(&self) -> PathBuf {
        self.runtime.join("localweb.sock")
    }

    /// Returns the configuration file path for `id`.
    #[must_use]
    pub fn project_file(&self, id: Uuid) -> PathBuf {
        self.config.join("projects").join(format!("{id}.toml"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub schema_version: u32,
    pub general: GeneralConfig,
    pub defaults: Defaults,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    pub domain_suffix: String,
    pub https: bool,
    pub autostart: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Defaults {
    pub web_server: WebServer,
    pub php: String,
    pub database: Database,
    pub cache: Cache,
    pub composer: String,
    #[serde(default = "default_node_version")]
    pub node: String,
}

fn default_node_version() -> String {
    "24".into()
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA,
            general: GeneralConfig {
                domain_suffix: "dev".into(),
                https: true,
                autostart: false,
            },
            defaults: Defaults {
                web_server: WebServer::Nginx,
                php: "8.5".into(),
                database: Database::None,
                cache: Cache::None,
                composer: "2".into(),
                node: default_node_version(),
            },
        }
    }
}

impl GlobalConfig {
    /// Loads the global configuration or creates a secure default.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported schemas, invalid settings, or filesystem failures.
    pub fn load_or_create(paths: &Paths) -> Result<Self> {
        let file = paths.config.join("config.toml");
        if !file.exists() {
            let default = Self::default();
            atomic_write(
                &file,
                toml::to_string_pretty(&default)
                    .map_err(|e| LocalWebError::Config(e.to_string()))?
                    .as_bytes(),
                0o600,
            )?;
            return Ok(default);
        }
        let value: Self = toml::from_str(&fs::read_to_string(file)?)
            .map_err(|e| LocalWebError::Config(e.to_string()))?;
        if value.schema_version != CONFIG_SCHEMA {
            return Err(LocalWebError::Config(format!(
                "unsupported schema version {}",
                value.schema_version
            )));
        }
        validate_domain(&format!("project.{}", value.general.domain_suffix))?;
        Ok(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WebServer {
    Nginx,
    Apache,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Database {
    None,
    Mariadb,
    Mysql,
    Postgres,
    Sqlite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Cache {
    None,
    Redis,
    Memcached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    GenericPhp,
    Laravel,
    Symfony,
    Drupal,
    Wordpress,
    ProcessWire,
}

/// A project-scoped long-running worker managed by the systemd user manager.
///
/// The first command element is the executable. `@php` resolves to the same
/// PHP CLI selected by the project. Every following element is passed as one
/// literal argument; no shell parses this value.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupervisorConfig {
    pub enabled: bool,
    pub command: Vec<String>,
}

/// A project-scoped scheduled task executed by the daemon while the project runs.
///
/// `schedule` holds five fields (`minute hour day month weekday`) using `*`,
/// steps (`*/5`), ranges (`1-5`), and comma lists. Month and weekday names are
/// not accepted; use numbers (`0` and `7` both mean Sunday). `command` follows
/// the same rules as a supervisor command: the first element is the executable
/// (`@php` resolves to the project's PHP CLI) and the rest are literal
/// arguments, never parsed through a shell.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronEntry {
    pub enabled: bool,
    pub schedule: String,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub schema_version: u32,
    pub id: Uuid,
    pub name: String,
    pub path: PathBuf,
    pub hostname: String,
    pub document_root: PathBuf,
    pub profile: Profile,
    pub web_server: WebServer,
    pub php: String,
    pub composer: String,
    #[serde(default = "default_node_version")]
    pub node: String,
    pub database: Database,
    pub cache: Cache,
    #[serde(default)]
    pub supervisor: SupervisorConfig,
    #[serde(default)]
    pub cron: Vec<CronEntry>,
    #[serde(default)]
    pub mail: bool,
    pub autostart: bool,
    pub enabled: bool,
    pub environment: BTreeMap<String, String>,
    pub resources: ResourceSettings,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSettings {
    pub fpm_max_children: u16,
    pub fpm_idle_timeout_seconds: u16,
    pub fpm_max_requests: u32,
    pub cache_memory_mb: u32,
}

impl Default for ResourceSettings {
    fn default() -> Self {
        Self {
            fpm_max_children: 4,
            fpm_idle_timeout_seconds: 10,
            fpm_max_requests: 500,
            cache_memory_mb: 64,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewProject {
    pub path: PathBuf,
    pub name: Option<String>,
    pub hostname: Option<String>,
    pub document_root: Option<PathBuf>,
    pub profile: Option<Profile>,
    pub web_server: Option<WebServer>,
    pub php: Option<String>,
    pub composer: Option<String>,
    pub node: Option<String>,
    pub database: Option<Database>,
    pub cache: Option<Cache>,
    #[serde(default)]
    pub supervisor: SupervisorConfig,
    #[serde(default)]
    pub cron: Vec<CronEntry>,
    #[serde(default)]
    pub cron_preset: bool,
    #[serde(default)]
    pub mail: bool,
    pub autostart: bool,
}

impl Project {
    /// Validates input and creates a complete project configuration.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid names, paths, domains, document roots, or PHP versions.
    pub fn from_new(input: NewProject, global: &GlobalConfig) -> Result<Self> {
        let path = input.path.canonicalize()?;
        if !path.is_dir() {
            return Err(LocalWebError::Config(
                "project path must be a directory".into(),
            ));
        }
        let basename = path
            .file_name()
            .and_then(|x| x.to_str())
            .ok_or_else(|| LocalWebError::Config("project path has no valid basename".into()))?;
        let name = input.name.unwrap_or_else(|| sanitize_name(basename));
        validate_name(&name)?;
        let hostname = input
            .hostname
            .unwrap_or_else(|| format!("{}.{}", name, global.general.domain_suffix));
        validate_domain(&hostname)?;
        let profile = input.profile.unwrap_or_else(|| detect_profile(&path));
        let document_root = input.document_root.unwrap_or_else(|| match profile {
            Profile::Laravel | Profile::Symfony => PathBuf::from("public"),
            _ => PathBuf::from("."),
        });
        validate_document_root(&path, &document_root)?;
        let php = input.php.unwrap_or_else(|| global.defaults.php.clone());
        validate_php_version(&php)?;
        let node = input.node.unwrap_or_else(|| global.defaults.node.clone());
        validate_node_version(&node)?;
        let composer = input
            .composer
            .unwrap_or_else(|| global.defaults.composer.clone());
        if composer != "2" {
            return Err(LocalWebError::Config("only Composer 2 is supported".into()));
        }
        let mut supervisor = input.supervisor;
        if supervisor.enabled && supervisor.command.is_empty() {
            supervisor.command = default_supervisor_command(profile);
        }
        validate_supervisor(&supervisor)?;
        if input.cron.len() > 64 {
            return Err(LocalWebError::Config(
                "a project may define at most 64 cron entries".into(),
            ));
        }
        for entry in &input.cron {
            validate_cron_entry(entry)?;
        }
        let mut cron = input.cron;
        if cron.is_empty() && input.cron_preset {
            cron = default_cron_entries(profile, &hostname);
        }
        if input.cron_preset && cron.is_empty() {
            return Err(LocalWebError::Config(
                "this profile has no default schedule; describe at least one cron task".into(),
            ));
        }
        Ok(Self {
            schema_version: CONFIG_SCHEMA,
            id: Uuid::new_v4(),
            name,
            path,
            hostname,
            document_root,
            profile,
            web_server: input.web_server.unwrap_or(global.defaults.web_server),
            php,
            node,
            composer,
            database: input.database.unwrap_or(global.defaults.database),
            cache: input.cache.unwrap_or(global.defaults.cache),
            supervisor,
            cron,
            mail: input.mail,
            autostart: input.autostart,
            enabled: true,
            environment: BTreeMap::new(),
            resources: ResourceSettings::default(),
            created_at: Utc::now(),
        })
    }

    /// Resolves the validated absolute document root.
    ///
    /// # Errors
    ///
    /// Returns an error when the root is absent or escapes the project directory.
    pub fn absolute_document_root(&self) -> Result<PathBuf> {
        validate_document_root(&self.path, &self.document_root)
    }

    /// Validates the project worker definition loaded from configuration.
    ///
    /// # Errors
    ///
    /// Returns an error when the supervisor command is absent or contains
    /// values that cannot be passed safely as literal process arguments.
    pub fn validate_supervisor(&self) -> Result<()> {
        validate_supervisor(&self.supervisor)
    }
}

fn validate_supervisor(supervisor: &SupervisorConfig) -> Result<()> {
    if !supervisor.enabled {
        return Ok(());
    }
    if supervisor.command.is_empty() {
        return Err(LocalWebError::Config(
            "an enabled supervisor requires a command".into(),
        ));
    }
    if supervisor.command.len() > 64 {
        return Err(LocalWebError::Config(
            "a supervisor command may contain at most 64 arguments".into(),
        ));
    }
    if supervisor.command.iter().any(|argument| {
        argument.is_empty()
            || argument.len() > 4_096
            || argument.contains('\0')
            || argument.contains('\n')
            || argument.contains('\r')
    }) {
        return Err(LocalWebError::Config(
            "supervisor arguments must be non-empty single-line values".into(),
        ));
    }
    Ok(())
}

/// Validates one scheduled task: parseable schedule plus a safe command.
fn validate_cron_entry(entry: &CronEntry) -> Result<()> {
    validate_cron_schedule(&entry.schedule)?;
    if entry.command.is_empty() {
        return Err(LocalWebError::Config(
            "a cron entry requires a command".into(),
        ));
    }
    if entry.command.len() > 64 {
        return Err(LocalWebError::Config(
            "a cron command may contain at most 64 arguments".into(),
        ));
    }
    if entry.command.iter().any(|argument| {
        argument.is_empty()
            || argument.len() > 4_096
            || argument.contains('\0')
            || argument.contains('\n')
            || argument.contains('\r')
    }) {
        return Err(LocalWebError::Config(
            "cron arguments must be non-empty single-line values".into(),
        ));
    }
    Ok(())
}

/// Validates a five-field cron schedule: numbers, `*`, steps, ranges, lists.
///
/// Bounds are minute `0-59`, hour `0-23`, day `1-31`, month `1-12`,
/// weekday `0-7` (`0` and `7` both mean Sunday). Month and weekday names,
/// `@reboot`-style shorthands, and seconds are rejected.
///
/// # Errors
///
/// Returns an error for malformed schedules or out-of-range values.
pub fn validate_cron_schedule(schedule: &str) -> Result<()> {
    let fields: Vec<&str> = schedule.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(LocalWebError::Config(format!(
            "cron schedule needs exactly five fields (minute hour day month weekday): {schedule}"
        )));
    }
    let bounds = [(0, 59), (0, 23), (1, 31), (1, 12), (0, 7)];
    for (field, (lo, hi)) in fields.iter().zip(bounds) {
        validate_cron_field(field, lo, hi)?;
    }
    Ok(())
}

fn validate_cron_field(field: &str, lo: u32, hi: u32) -> Result<()> {
    let (base, step) = match field.split_once('/') {
        Some((base, step)) => {
            let amount: u32 = step
                .parse()
                .map_err(|_| LocalWebError::Config(format!("invalid cron step: {field}")))?;
            if amount == 0 || amount > hi {
                return Err(LocalWebError::Config(format!("invalid cron step: {field}")));
            }
            (base, true)
        }
        None => (field, false),
    };
    for part in base.split(',') {
        if part == "*" {
            continue;
        }
        if step && !part.contains('-') {
            return Err(LocalWebError::Config(format!(
                "a cron step needs `*` or a range: {field}"
            )));
        }
        match part.split_once('-') {
            Some((from, to)) => {
                let start = parse_cron_number(from, lo, hi, field)?;
                let end = parse_cron_number(to, lo, hi, field)?;
                if start > end {
                    return Err(LocalWebError::Config(format!(
                        "reversed cron range: {field}"
                    )));
                }
            }
            None => {
                parse_cron_number(part, lo, hi, field)?;
            }
        }
    }
    Ok(())
}

fn parse_cron_number(text: &str, lo: u32, hi: u32, field: &str) -> Result<u32> {
    if text.is_empty() {
        return Err(LocalWebError::Config(format!("empty cron value: {field}")));
    }
    let value: u32 = text
        .parse()
        .map_err(|_| LocalWebError::Config(format!("invalid cron value `{text}` in: {field}")))?;
    if value < lo || value > hi {
        return Err(LocalWebError::Config(format!(
            "cron value {value} out of range {lo}-{hi}: {field}"
        )));
    }
    Ok(value)
}

/// Parses one cron line: five schedule fields, then executable and arguments.
///
/// ```text
/// */5 * * * * @php artisan schedule:run
/// ```
///
/// # Errors
///
/// Returns an error for malformed schedules or missing commands.
pub fn parse_cron_line(line: &str) -> Result<CronEntry> {
    let mut parts = line.split_whitespace();
    let mut schedule = Vec::with_capacity(5);
    for _ in 0..5 {
        schedule.push(parts.next().ok_or_else(|| {
            LocalWebError::Config(format!("cron line needs a five-field schedule: {line}"))
        })?);
    }
    let entry = CronEntry {
        enabled: true,
        schedule: schedule.join(" "),
        command: parts.map(str::to_owned).collect(),
    };
    validate_cron_entry(&entry)?;
    Ok(entry)
}

/// Returns true when `schedule` fires at `moment`, to minute precision.
///
/// Out-of-range callers should validate first; an unparsable schedule
/// simply never matches.
#[must_use]
pub fn cron_matches(schedule: &str, moment: &chrono::DateTime<chrono::Local>) -> bool {
    use chrono::{Datelike, Timelike};
    let fields: Vec<&str> = schedule.split_whitespace().collect();
    if fields.len() != 5 {
        return false;
    }
    let values = [
        moment.minute(),
        moment.hour(),
        moment.day(),
        moment.month(),
        moment.weekday().number_from_monday() % 7,
    ];
    fields
        .iter()
        .zip(values)
        .all(|(field, value)| cron_field_matches(field, value))
}

fn cron_field_matches(field: &str, value: u32) -> bool {
    let (base, step) = match field.split_once('/') {
        Some((base, step)) => match step.parse::<u32>() {
            Ok(amount) if amount > 0 => (base, amount),
            _ => return false,
        },
        None => (field, 0),
    };
    base.split(',')
        .any(|part| cron_part_matches(part, value, step))
}

fn cron_part_matches(part: &str, value: u32, step: u32) -> bool {
    if part == "*" {
        return step == 0 || value.is_multiple_of(step);
    }
    if let Some((from, to)) = part.split_once('-') {
        let (Ok(start), Ok(end)) = (from.parse::<u32>(), to.parse::<u32>()) else {
            return false;
        };
        if value < start || value > end {
            return false;
        }
        return step == 0 || (value - start).is_multiple_of(step);
    }
    if step != 0 {
        return false;
    }
    match part.parse::<u32>() {
        // Sunday is both 0 and 7.
        Ok(number) => number == value || (value == 0 && number == 7),
        Err(_) => false,
    }
}

/// Converts a folder name into a conservative project identifier.
#[must_use]
pub fn sanitize_name(input: &str) -> String {
    let mut result = String::new();
    for c in input.to_ascii_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c);
        } else if (c == '-' || c == '_' || c == ' ') && !result.ends_with('-') {
            result.push('-');
        }
    }
    result.trim_matches('-').to_owned()
}

/// Validates a project or DNS-label identifier.
///
/// # Errors
///
/// Returns an error when the name violates `LocalWeb`'s lowercase DNS-label rules.
pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 63
        || !name
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        || !name
            .bytes()
            .last()
            .is_some_and(|b| b.is_ascii_alphanumeric())
        || !name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(LocalWebError::Config(format!(
            "invalid project name: {name}"
        )));
    }
    Ok(())
}

/// Validates a complete project hostname.
///
/// # Errors
///
/// Returns an error for malformed, oversized, or unsafe hostnames.
pub fn validate_domain(domain: &str) -> Result<()> {
    if domain.len() > 253
        || domain.len() < 3
        || !domain.contains('.')
        || domain.ends_with('.')
        || domain.split('.').any(|part| validate_name(part).is_err())
    {
        return Err(LocalWebError::Config(format!("invalid hostname: {domain}")));
    }
    Ok(())
}

/// Validates a PHP version understood by the project schema.
///
/// # Errors
///
/// Returns an error when the version is outside the declared PHP 8.x range.
pub fn validate_php_version(version: &str) -> Result<()> {
    if !PHP_VERSIONS.contains(&version) {
        return Err(LocalWebError::Config(format!(
            "unsupported PHP version: {version}"
        )));
    }
    Ok(())
}

/// Validates a Node.js version understood by the project schema.
///
/// The compatibility lines are 20, 22, and 24. Node 24 is bundled; the
/// older lines resolve against a system Node when present.
///
/// # Errors
///
/// Returns an error when the version is not 20, 22, or 24.
pub fn validate_node_version(version: &str) -> Result<()> {
    if !matches!(version, "20" | "22" | "24") {
        return Err(LocalWebError::Config(format!(
            "unsupported Node.js version: {version} (expected 20, 22, or 24)"
        )));
    }
    Ok(())
}

/// Resolves a document root that remains inside its project.
///
/// # Errors
///
/// Returns an error for absolute, missing, non-directory, or escaping paths.
pub fn validate_document_root(project: &Path, relative: &Path) -> Result<PathBuf> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(LocalWebError::Config(
            "document root must be relative and inside the project".into(),
        ));
    }
    let absolute = project.join(relative).canonicalize()?;
    if !absolute.starts_with(project) || !absolute.is_dir() {
        return Err(LocalWebError::Config(
            "document root escapes project or is not a directory".into(),
        ));
    }
    Ok(absolute)
}

/// Detects a supported PHP application profile from conventional files.
#[must_use]
pub fn detect_profile(path: &Path) -> Profile {
    if path.join("artisan").exists() {
        Profile::Laravel
    } else if path.join("bin/console").exists() {
        Profile::Symfony
    } else if path.join("core/lib/Drupal.php").is_file() {
        Profile::Drupal
    } else if path.join("wire/core/ProcessWire.php").is_file() {
        Profile::ProcessWire
    } else if path.join("wp-config.php").exists() || path.join("wp-includes").is_dir() {
        Profile::Wordpress
    } else {
        Profile::GenericPhp
    }
}

/// Returns the conventional supervisor command for a framework profile.
///
/// Laravel runs a queue worker and Symfony consumes Messenger transports.
/// Every other profile returns an empty command: enabling a supervisor
/// without one is rejected during validation.
#[must_use]
pub fn default_supervisor_command(profile: Profile) -> Vec<String> {
    match profile {
        Profile::Laravel => [
            "@php",
            "artisan",
            "queue:work",
            "--sleep=3",
            "--tries=3",
            "--timeout=90",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        Profile::Symfony => [
            "@php",
            "bin/console",
            "messenger:consume",
            "async",
            "--time-limit=3600",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        Profile::GenericPhp | Profile::Drupal | Profile::Wordpress | Profile::ProcessWire => {
            Vec::new()
        }
    }
}

/// Returns the conventional cron entries for a framework profile.
///
/// Laravel needs its scheduler kicked every minute, `WordPress` needs its
/// pseudo-cron kicked every few minutes, Drupal runs `drush cron`
/// periodically. Other applications provide their own schedule and command.
/// Profiles without a convention return no entries.
#[must_use]
pub fn default_cron_entries(profile: Profile, hostname: &str) -> Vec<CronEntry> {
    let entry = |schedule: &str, command: Vec<String>| CronEntry {
        enabled: true,
        schedule: schedule.to_owned(),
        command,
    };
    match profile {
        Profile::Laravel => vec![entry(
            "* * * * *",
            ["@php", "artisan", "schedule:run"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        )],
        Profile::Wordpress => vec![entry(
            "*/5 * * * *",
            vec!["@php".to_owned(), "wp-cron.php".to_owned()],
        )],
        Profile::Drupal => vec![entry(
            "*/15 * * * *",
            vec![
                "@php".to_owned(),
                "vendor/bin/drush".to_owned(),
                "cron".to_owned(),
                format!("--uri=https://{hostname}"),
            ],
        )],
        Profile::GenericPhp | Profile::Symfony | Profile::ProcessWire => Vec::new(),
    }
}

/// Atomically replaces a file using the requested Unix permission mode.
///
/// # Errors
///
/// Returns an error when the temporary file cannot be written, synced, or renamed.
pub fn atomic_write(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| LocalWebError::Config("path has no parent".into()))?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!(".localweb-{}.tmp", Uuid::new_v4()));
    let result = (|| -> Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
#[allow(
    clippy::large_enum_variant,
    reason = "creation carries the full project configuration by design"
)]
pub enum Request {
    Status,
    Shutdown,
    ProjectList,
    ProjectGet { name: String },
    ProjectAdd(NewProject),
    ProjectRemove { name: String },
    ProjectStart { name: String },
    ProjectStop { name: String },
    ProjectRestart { name: String },
    ProjectEnvironment { name: String },
    RuntimeList,
    ResolveExecution { name: String },
    CaStatus,
    Doctor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub version: u32,
    pub payload: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Response {
    Status(Status),
    Projects(Vec<ProjectStatus>),
    Project(ProjectStatus),
    Message(String),
    Diagnostics(Vec<Diagnostic>),
    Execution(ExecutionSpec),
    Environment(BTreeMap<String, String>),
    Ca(CaStatus),
    Error { code: String, message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionSpec {
    pub project_path: PathBuf,
    pub php: PathBuf,
    pub composer_phar: Option<PathBuf>,
    #[serde(default)]
    pub node: Option<PathBuf>,
    #[serde(default)]
    pub npm_cli: Option<PathBuf>,
    pub shim_dir: PathBuf,
    pub library_path: Option<String>,
    pub php_ini: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaStatus {
    pub exists: bool,
    pub trusted: bool,
    pub certificate: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub daemon_running: bool,
    pub gateway_running: bool,
    pub project_count: usize,
    pub active_projects: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStatus {
    pub project: Project,
    pub running: bool,
    #[serde(default)]
    pub mail_web_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayRoute {
    pub hostname: String,
    pub upstream_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub level: String,
    pub check: String,
    pub detail: String,
    pub fix: Option<String>,
}

/// Sends one versioned request to the per-user daemon.
///
/// # Errors
///
/// Returns an error for socket failures, oversized messages, malformed JSON, or version mismatch.
pub async fn send_request(paths: &Paths, request: Request) -> Result<Response> {
    let mut socket = UnixStream::connect(paths.socket()).await?;
    let mut data = serde_json::to_vec(&Envelope {
        version: PROTOCOL_VERSION,
        payload: request,
    })
    .map_err(|e| LocalWebError::Protocol(e.to_string()))?;
    data.push(b'\n');
    socket.write_all(&data).await?;
    let mut reader = BufReader::new(socket);
    let mut response = Vec::new();
    reader.read_until(b'\n', &mut response).await?;
    if response.len() > MAX_MESSAGE_BYTES {
        return Err(LocalWebError::Protocol("response too large".into()));
    }
    let envelope: Envelope<Response> =
        serde_json::from_slice(&response).map_err(|e| LocalWebError::Protocol(e.to_string()))?;
    if envelope.version != PROTOCOL_VERSION {
        return Err(LocalWebError::Protocol(
            "incompatible daemon protocol version".into(),
        ));
    }
    Ok(envelope.payload)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_and_domains() {
        assert_eq!(sanitize_name("My Cool_Project!"), "my-cool-project");
        assert!(validate_domain("acme.dev").is_ok());
        assert!(validate_domain("bad..dev").is_err());
        assert!(validate_domain("x.dev\n127.0.0.1 evil.dev").is_err());
    }

    #[test]
    fn bundled_node_lts_is_the_new_project_default() {
        assert_eq!(GlobalConfig::default().defaults.node, "24");
    }

    #[test]
    fn php_build_sources_match_the_stable_selection() {
        let sources = include_str!("../../../packaging/php-sources.txt");
        let versions = sources
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| line.split_whitespace().next())
            .collect::<Vec<_>>();
        assert_eq!(versions, PHP_VERSIONS);
        assert!(validate_php_version("8.6").is_err());
    }
    #[test]
    fn protocol_roundtrip() {
        let e = Envelope {
            version: PROTOCOL_VERSION,
            payload: Request::ProjectStart {
                name: "demo".into(),
            },
        };
        let text = serde_json::to_string(&e).unwrap();
        let _: Envelope<Request> = serde_json::from_str(&text).unwrap();
    }

    #[test]
    fn supervisor_rejects_shell_like_multiline_arguments() {
        let supervisor = SupervisorConfig {
            enabled: true,
            command: vec!["@php".into(), "artisan\nqueue:work".into()],
        };
        assert!(validate_supervisor(&supervisor).is_err());
    }

    #[test]
    fn laravel_supervisor_gets_safe_queue_worker_defaults() {
        let path = std::env::temp_dir().join(format!(
            "localweb-supervisor-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(path.join("public")).unwrap();
        let project = Project::from_new(
            NewProject {
                path: path.clone(),
                name: Some("worker-test".into()),
                hostname: Some("worker-test.dev".into()),
                document_root: Some(PathBuf::from("public")),
                profile: Some(Profile::Laravel),
                web_server: None,
                php: None,
                composer: None,
                database: None,
                cache: None,
                supervisor: SupervisorConfig {
                    enabled: true,
                    command: Vec::new(),
                },
                cron: Vec::new(),
                cron_preset: false,
                mail: false,
                node: None,
                autostart: false,
            },
            &GlobalConfig::default(),
        )
        .unwrap();
        assert_eq!(
            project.supervisor.command,
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
    fn cron_schedules_validate_and_match() {
        assert!(validate_cron_schedule("* * * * *").is_ok());
        assert!(validate_cron_schedule("*/5 * * * *").is_ok());
        assert!(validate_cron_schedule("0 3 * * 0").is_ok());
        assert!(validate_cron_schedule("1-30/2 9-17 1,15 1-12 1-5").is_ok());
        assert!(validate_cron_schedule("* * * *").is_err());
        assert!(validate_cron_schedule("60 * * * *").is_err());
        assert!(validate_cron_schedule("*/0 * * * *").is_err());
        assert!(validate_cron_schedule("5/10 * * * *").is_err());
        assert!(validate_cron_schedule("mon * * * *").is_err());
        assert!(parse_cron_line("*/5 * * * * @php artisan schedule:run").is_ok());
        assert!(parse_cron_line("* * * * *").is_err());
        // 2026-10-11 is a Sunday.
        let sunday = chrono::NaiveDate::from_ymd_opt(2026, 10, 11)
            .and_then(|date| date.and_hms_opt(15, 30, 0))
            .and_then(|naive| naive.and_local_timezone(chrono::Local).single())
            .unwrap();
        assert!(cron_matches("30 15 * * 0", &sunday));
        assert!(cron_matches("30 15 * * 7", &sunday));
        assert!(!cron_matches("30 15 * * 1", &sunday));
        assert!(cron_matches("*/5 * * * *", &sunday));
        assert!(!cron_matches("31 15 * * *", &sunday));
    }

    #[test]
    fn sqlite_database_serializes_to_lowercase() {
        assert_eq!(
            serde_json::to_string(&Database::Sqlite).unwrap(),
            "\"sqlite\""
        );
    }

    #[test]
    fn project_status_without_mail_url_still_parses() {
        let path = std::env::temp_dir().join(format!(
            "localweb-mailurl-{}-{}",
            std::process::id(),
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(path.join("public")).unwrap();
        let project = Project::from_new(
            NewProject {
                path: path.clone(),
                name: Some("mail-test".into()),
                hostname: Some("mail-test.dev".into()),
                document_root: Some(PathBuf::from("public")),
                profile: None,
                web_server: None,
                php: None,
                composer: None,
                node: None,
                database: None,
                cache: None,
                supervisor: SupervisorConfig::default(),
                cron: Vec::new(),
                cron_preset: false,
                mail: true,
                autostart: false,
            },
            &GlobalConfig::default(),
        )
        .unwrap();
        let status = ProjectStatus {
            project,
            running: false,
            mail_web_url: None,
        };
        let mut value = serde_json::to_value(&status).unwrap();
        value.as_object_mut().unwrap().remove("mail_web_url");
        let parsed: ProjectStatus = serde_json::from_value(value).unwrap();
        assert_eq!(parsed.mail_web_url, None);
        std::fs::remove_dir_all(path).unwrap();
    }
}

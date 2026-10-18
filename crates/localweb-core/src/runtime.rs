//! Resolves the private runtime payload inside a native `LocalWeb` package.

use crate::{LocalWebError, Result, validate_php_version};
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::process::Command;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Default installation root for the self-contained runtime payload.
pub const INSTALLED_RUNTIME_ROOT: &str = "/usr/lib/localweb/runtime";

#[derive(Debug, Clone)]
/// Resolves executable and library paths inside one runtime root.
pub struct RuntimeLayout {
    root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Describes provenance for a complete bundled runtime payload.
pub struct RuntimeManifest {
    /// Manifest schema version.
    pub schema_version: u32,
    /// Supported operating system and architecture.
    pub target: String,
    /// Reproducible build timestamp.
    pub built_at: String,
    /// Bundled component provenance records.
    pub components: Vec<RuntimeComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Describes one bundled native runtime component.
pub struct RuntimeComponent {
    /// Component name.
    pub name: String,
    /// Component version.
    pub version: String,
    /// Provider that produced the installed artifact.
    pub provider: String,
    /// Upstream package or download source.
    pub source: String,
    /// SHA-256 digest of the primary artifact.
    pub sha256: String,
}

impl RuntimeLayout {
    /// Locates the installed runtime or the explicit development override.
    #[must_use]
    pub fn discover() -> Self {
        #[cfg(windows)]
        let default = std::env::var_os("ProgramFiles")
            .map_or_else(|| PathBuf::from(r"C:\Program Files"), PathBuf::from)
            .join("LocalWeb/runtime");
        #[cfg(not(windows))]
        let default = PathBuf::from(INSTALLED_RUNTIME_ROOT);
        Self::from_root(std::env::var_os("LOCALWEB_RUNTIME_ROOT").map_or(default, PathBuf::from))
    }

    /// Creates a layout rooted at `root`.
    #[must_use]
    pub fn from_root(root: PathBuf) -> Self {
        Self { root }
    }
    /// Returns the runtime root.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
    /// Returns the provenance manifest path.
    #[must_use]
    pub fn manifest_path(&self) -> PathBuf {
        self.root.join("runtime-manifest.json")
    }

    /// Reads and validates the bundled runtime manifest.
    ///
    /// # Errors
    ///
    /// Returns an error when the manifest is absent, invalid, or targets another platform.
    pub fn manifest(&self) -> Result<RuntimeManifest> {
        let bytes = fs::read(self.manifest_path()).map_err(|error| {
            LocalWebError::RuntimeNotInstalled(format!("bundled runtime manifest: {error}"))
        })?;
        let manifest: RuntimeManifest = serde_json::from_slice(&bytes).map_err(|error| {
            LocalWebError::Config(format!("invalid bundled runtime manifest: {error}"))
        })?;
        let arch = match std::env::consts::ARCH {
            "x86_64" => "amd64",
            "aarch64" => "arm64",
            other => other,
        };
        let os = match crate::platform::TrustStore::detect()? {
            crate::platform::TrustStore::Ubuntu => "ubuntu",
            crate::platform::TrustStore::Arch => "arch",
            #[cfg(target_os = "macos")]
            crate::platform::TrustStore::Macos => "macos",
            #[cfg(windows)]
            crate::platform::TrustStore::Windows => "windows",
        };
        if manifest.schema_version != 1
            || !manifest.target.starts_with(&format!("{os}-"))
            || !manifest.target.ends_with(&format!("-{arch}"))
        {
            return Err(LocalWebError::Config(format!(
                "unsupported bundled runtime target: {}",
                manifest.target
            )));
        }
        Ok(manifest)
    }

    /// Resolves matching PHP CLI and FPM executables.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported versions or an incomplete runtime payload.
    pub fn php(&self, version: &str) -> Result<(PathBuf, PathBuf)> {
        validate_php_version(version)?;
        #[cfg(windows)]
        {
            for directory in windows_php_directories(version) {
                let php = directory.join("php.exe");
                let cgi = directory.join("php-cgi.exe");
                if php.is_file() && cgi.is_file() && windows_php_matches(&php, version) {
                    return Ok((php, cgi));
                }
            }
            Err(LocalWebError::RuntimeNotInstalled(format!(
                "PHP {version} with php-cgi.exe was not found in Scoop, Chocolatey or PATH"
            )))
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            {
                for formula in [format!("php@{version}"), "php".to_owned()] {
                    if let Some(directory) = macos_opt_dir(&formula) {
                        let php = directory.join("bin/php");
                        let fpm = directory.join("sbin/php-fpm");
                        if php.is_file() && fpm.is_file() {
                            return Ok((php, fpm));
                        }
                    }
                }
                let php = PathBuf::from("/usr/bin/php");
                let fpm = PathBuf::from("/usr/sbin/php-fpm");
                if php.is_file() && fpm.is_file() {
                    return Ok((php, fpm));
                }
            }
            let base = self.root.join("php").join(version);
            let php = base.join("bin/php");
            let fpm = base.join("sbin/php-fpm");
            require_pair(&php, &fpm, &format!("bundled PHP {version}"))?;
            Ok((php, fpm))
        }
    }

    /// Resolves the bundled Composer PHAR.
    ///
    /// # Errors
    ///
    /// Returns an error for unsupported versions or a missing PHAR.
    pub fn composer(&self, major: &str) -> Result<PathBuf> {
        if major != "2" {
            return Err(LocalWebError::Config(format!(
                "unsupported Composer version: {major}"
            )));
        }
        #[cfg(windows)]
        {
            let mut candidates = Vec::new();
            if let Some(directory) = scoop_app("composer") {
                candidates.push(directory.join("composer.phar"));
            }
            candidates.push(choco_root().join("lib/composer/tools/composer.phar"));
            candidates
                .into_iter()
                .find(|path| path.is_file())
                .ok_or_else(|| {
                    LocalWebError::RuntimeNotInstalled(
                        "Composer 2 is not installed via Scoop or Chocolatey".into(),
                    )
                })
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            if let Some(phar) = macos_composer_phar() {
                return Ok(phar);
            }
            require_file(
                self.root.join("composer/2/composer.phar"),
                "bundled Composer 2",
            )
        }
    }

    /// Resolves the bundled nginx executable.
    ///
    /// # Errors
    ///
    /// Returns an error when the executable is missing.
    pub fn nginx(&self) -> Result<PathBuf> {
        #[cfg(windows)]
        {
            scoop_app("nginx")
                .map(|directory| directory.join("nginx.exe"))
                .filter(|path| path.is_file())
                .or_else(|| which("nginx.exe"))
                .ok_or_else(|| {
                    LocalWebError::RuntimeNotInstalled(
                        "nginx is not installed via Scoop or Chocolatey".into(),
                    )
                })
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            if let Some(path) = macos_opt_file("nginx", "bin/nginx") {
                return Ok(path);
            }
            require_file(self.root.join("nginx/sbin/nginx"), "bundled nginx")
        }
    }

    /// Returns the nginx configuration prefix passed as `-p`.
    #[must_use]
    pub fn nginx_prefix(&self) -> PathBuf {
        #[cfg(windows)]
        {
            self.nginx()
                .ok()
                .and_then(|nginx| nginx.parent().map(Path::to_path_buf))
                .unwrap_or_else(|| self.root.join("nginx"))
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            {
                macos_prefix_file("etc/nginx/mime.types")
                    .and_then(|path| path.parent().map(Path::to_path_buf))
                    .unwrap_or_else(|| PathBuf::from("/opt/homebrew/etc/nginx"))
            }
            #[cfg(not(target_os = "macos"))]
            {
                self.root.join("nginx")
            }
        }
    }

    /// Returns the nginx MIME type database installed by the host.
    #[must_use]
    pub fn nginx_mime_types(&self) -> PathBuf {
        #[cfg(windows)]
        {
            self.nginx_prefix().join("conf/mime.types")
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            {
                for root in homebrew_roots() {
                    for candidate in [
                        root.join("etc/nginx/mime.types"),
                        root.join("opt/nginx/conf/mime.types"),
                        root.join("share/nginx/mime.types"),
                    ] {
                        if candidate.is_file() {
                            return candidate;
                        }
                    }
                }
            }
            self.root.join("nginx/conf/mime.types")
        }
    }

    /// Resolves the bundled Apache executable.
    ///
    /// # Errors
    ///
    /// Returns an error when the executable is missing.
    pub fn apache(&self) -> Result<PathBuf> {
        #[cfg(windows)]
        {
            if let Some(directory) = scoop_app("apache") {
                let path = directory.join("bin/httpd.exe");
                if path.is_file() {
                    return Ok(path);
                }
            }
            return which("httpd.exe").ok_or_else(|| {
                LocalWebError::RuntimeNotInstalled(
                    "Apache httpd is not installed via Scoop or Chocolatey".into(),
                )
            });
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            {
                if let Some(path) = macos_opt_file("httpd", "bin/httpd") {
                    return Ok(path);
                }
                if let Some(path) = macos_prefix_file("bin/httpd") {
                    return Ok(path);
                }
            }
            require_file(self.root.join("apache/bin/apache2"), "bundled Apache")
        }
    }

    /// Returns the Apache `ServerRoot` directory.
    #[must_use]
    pub fn apache_server_root(&self) -> PathBuf {
        #[cfg(windows)]
        {
            return self
                .apache()
                .ok()
                .and_then(|httpd| httpd.parent().map(Path::to_path_buf))
                .and_then(|bin| {
                    let root = bin.parent()?;
                    root.join("conf").is_dir().then(|| root.to_path_buf())
                })
                .unwrap_or_else(|| self.root.join("apache"));
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            {
                macos_prefix_file("etc/httpd/mime.types")
                    .and_then(|path| path.parent().map(Path::to_path_buf))
                    .unwrap_or_else(|| PathBuf::from("/opt/homebrew/etc/httpd"))
            }
            #[cfg(not(target_os = "macos"))]
            {
                self.root.join("apache")
            }
        }
    }

    /// Returns the Apache module directory.
    #[must_use]
    pub fn apache_modules_dir(&self) -> PathBuf {
        #[cfg(windows)]
        return self.apache_server_root().join("modules");
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            {
                for root in homebrew_roots() {
                    for candidate in [
                        root.join("opt/httpd/lib/httpd/modules"),
                        root.join("lib/httpd/modules"),
                        root.join("libexec/apache2"),
                    ] {
                        if candidate.is_dir() {
                            return candidate;
                        }
                    }
                }
            }
            self.apache_server_root().join("modules")
        }
    }

    /// Returns the Apache MIME type database installed by the host.
    #[must_use]
    pub fn apache_mime_types(&self) -> PathBuf {
        #[cfg(windows)]
        return self.apache_server_root().join("conf/mime.types");
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            {
                for root in homebrew_roots() {
                    let candidate = root.join("etc/httpd/mime.types");
                    if candidate.is_file() {
                        return candidate;
                    }
                }
            }
            self.apache_server_root().join("conf/mime.types")
        }
    }

    /// Resolves the bundled Node.js executable.
    ///
    /// # Errors
    ///
    /// Returns an error when the executable is missing.
    pub fn node(&self) -> Result<PathBuf> {
        #[cfg(windows)]
        {
            return scoop_app("nodejs")
                .map(|directory| directory.join("node.exe"))
                .filter(|path| path.is_file())
                .or_else(|| which("node.exe"))
                .ok_or_else(|| {
                    LocalWebError::RuntimeNotInstalled(
                        "Node.js is not installed via Scoop or Chocolatey".into(),
                    )
                });
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            {
                if let Some(path) = macos_opt_file("node", "bin/node") {
                    return Ok(path);
                }
                if let Some(path) = macos_prefix_file("bin/node") {
                    return Ok(path);
                }
            }
            require_file(self.root.join("node/bin/node"), "bundled Node.js")
        }
    }

    /// Resolves the bundled npm CLI entry point.
    ///
    /// # Errors
    ///
    /// Returns an error when the npm payload is missing.
    pub fn npm_cli(&self) -> Result<PathBuf> {
        #[cfg(windows)]
        {
            if let Ok(node) = self.node()
                && let Some(directory) = node.parent()
            {
                let path = directory.join("node_modules/npm/bin/npm-cli.js");
                if path.is_file() {
                    return Ok(path);
                }
            }
            return Err(LocalWebError::RuntimeNotInstalled(
                "npm-cli.js is missing from the Node.js installation".into(),
            ));
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            {
                if let Some(path) = macos_prefix_file("lib/node_modules/npm/bin/npm-cli.js") {
                    return Ok(path);
                }
                if let Some(path) = macos_opt_file("node", "lib/node_modules/npm/bin/npm-cli.js") {
                    return Ok(path);
                }
            }
            require_file(self.root.join("node/npm/bin/npm-cli.js"), "bundled npm")
        }
    }

    /// Resolves the bundled Mailpit executable.
    ///
    /// # Errors
    ///
    /// Returns an error when the executable is missing.
    pub fn mailpit(&self) -> Result<PathBuf> {
        #[cfg(windows)]
        {
            return scoop_app("mailpit")
                .map(|directory| directory.join("mailpit.exe"))
                .filter(|path| path.is_file())
                .or_else(|| which("mailpit.exe"))
                .ok_or_else(|| {
                    LocalWebError::RuntimeNotInstalled(
                        "Mailpit is not installed via Scoop or Chocolatey".into(),
                    )
                });
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            if let Some(path) = macos_opt_file("mailpit", "bin/mailpit") {
                return Ok(path);
            }
            require_file(self.root.join("mailpit/bin/mailpit"), "bundled Mailpit")
        }
    }

    /// Resolves MariaDB server, initializer, and client executables.
    ///
    /// # Errors
    ///
    /// Returns an error when any required executable is missing.
    pub fn mariadb(&self) -> Result<(PathBuf, PathBuf, PathBuf)> {
        #[cfg(windows)]
        {
            let base = scoop_app("mariadb")
                .or_else(|| {
                    which("mariadbd.exe").and_then(|server| {
                        server
                            .parent()
                            .and_then(Path::parent)
                            .map(Path::to_path_buf)
                    })
                })
                .ok_or_else(|| {
                    LocalWebError::RuntimeNotInstalled(
                        "MariaDB is not installed via Scoop or Chocolatey".into(),
                    )
                })?;
            let bin = base.join("bin");
            return Ok((
                require_file(bin.join("mariadbd.exe"), "MariaDB server")?,
                require_file(bin.join("mariadb-install-db.exe"), "MariaDB initializer")?,
                require_file(bin.join("mariadb.exe"), "MariaDB client")?,
            ));
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            {
                if let Some(directory) = macos_opt_dir("mariadb") {
                    let server = directory.join("bin/mariadbd");
                    let install = directory.join("bin/mariadb-install-db");
                    let client = directory.join("bin/mariadb");
                    if server.is_file() && install.is_file() && client.is_file() {
                        return Ok((server, install, client));
                    }
                }
            }
            let base = self.root.join("mariadb");
            let server = require_file(base.join("bin/mariadbd"), "bundled MariaDB server")?;
            let install = require_file(
                base.join("bin/mariadb-install-db"),
                "bundled MariaDB initializer",
            )?;
            let client = require_file(base.join("bin/mariadb"), "bundled MariaDB client")?;
            Ok((server, install, client))
        }
    }

    /// Resolves MySQL server and client executables.
    ///
    /// # Errors
    ///
    /// Returns an error when any required executable is missing.
    pub fn mysql(&self) -> Result<(PathBuf, PathBuf)> {
        #[cfg(windows)]
        {
            let base = scoop_app("mysql")
                .or_else(|| {
                    which("mysqld.exe").and_then(|server| {
                        server
                            .parent()
                            .and_then(Path::parent)
                            .map(Path::to_path_buf)
                    })
                })
                .ok_or_else(|| {
                    LocalWebError::RuntimeNotInstalled(
                        "MySQL is not installed via Scoop or Chocolatey".into(),
                    )
                })?;
            let bin = base.join("bin");
            return Ok((
                require_file(bin.join("mysqld.exe"), "MySQL server")?,
                require_file(bin.join("mysql.exe"), "MySQL client")?,
            ));
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            {
                if let Some(directory) = macos_opt_dir("mysql") {
                    let server = directory.join("bin/mysqld");
                    let client = directory.join("bin/mysql");
                    if server.is_file() && client.is_file() {
                        return Ok((server, client));
                    }
                }
            }
            let base = self.root.join("mysql");
            let server = require_file(base.join("bin/mysqld"), "bundled MySQL server")?;
            let client = require_file(base.join("bin/mysql"), "bundled MySQL client")?;
            Ok((server, client))
        }
    }

    /// Resolves the PostgreSQL server, initializer, and client.
    ///
    /// # Errors
    /// Returns an error when the PostgreSQL payload is incomplete.
    pub fn postgres(&self) -> Result<(PathBuf, PathBuf, PathBuf)> {
        #[cfg(windows)]
        {
            let base = scoop_app("postgresql")
                .or_else(|| {
                    which("postgres.exe").and_then(|server| {
                        server
                            .parent()
                            .and_then(Path::parent)
                            .map(Path::to_path_buf)
                    })
                })
                .ok_or_else(|| {
                    LocalWebError::RuntimeNotInstalled(
                        "PostgreSQL is not installed via Scoop or Chocolatey".into(),
                    )
                })?;
            let bin = base.join("bin");
            return Ok((
                require_file(bin.join("postgres.exe"), "PostgreSQL server")?,
                require_file(bin.join("initdb.exe"), "PostgreSQL initializer")?,
                require_file(bin.join("psql.exe"), "PostgreSQL client")?,
            ));
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            {
                if let Some(directory) = macos_postgres_dir() {
                    let bin = directory.join("bin");
                    return Ok((
                        require_file(bin.join("postgres"), "PostgreSQL server")?,
                        require_file(bin.join("initdb"), "PostgreSQL initializer")?,
                        require_file(bin.join("psql"), "PostgreSQL client")?,
                    ));
                }
            }
            let bin = self.root.join("postgres/bin");
            Ok((
                require_file(bin.join("postgres"), "bundled PostgreSQL server")?,
                require_file(bin.join("initdb"), "bundled PostgreSQL initializer")?,
                require_file(bin.join("psql"), "bundled PostgreSQL client")?,
            ))
        }
    }

    /// Resolves Redis server and client executables.
    ///
    /// # Errors
    ///
    /// Returns an error when either executable is missing.
    pub fn redis(&self) -> Result<(PathBuf, PathBuf)> {
        #[cfg(windows)]
        {
            let base = scoop_app("redis")
                .or_else(|| {
                    which("redis-server.exe")
                        .and_then(|server| server.parent().map(Path::to_path_buf))
                })
                .ok_or_else(|| {
                    LocalWebError::RuntimeNotInstalled(
                        "Redis is not installed via Scoop or Chocolatey".into(),
                    )
                })?;
            return Ok((
                require_file(base.join("redis-server.exe"), "Redis server")?,
                require_file(base.join("redis-cli.exe"), "Redis client")?,
            ));
        }
        #[cfg(not(windows))]
        {
            let base = self.root.join("redis/bin");
            Ok((
                require_file(base.join("redis-server"), "bundled Redis server")?,
                require_file(base.join("redis-cli"), "bundled Redis client")?,
            ))
        }
    }

    /// Returns the MariaDB installation base directory.
    #[must_use]
    pub fn mariadb_basedir(&self) -> PathBuf {
        #[cfg(windows)]
        {
            return windows_tool_base("mariadbd.exe", "mariadb")
                .unwrap_or_else(|| self.root.join("mariadb"));
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            if let Some(directory) = macos_opt_dir("mariadb") {
                return directory;
            }
            self.root.join("mariadb")
        }
    }

    /// Returns the MariaDB plugin directory.
    #[must_use]
    pub fn mariadb_plugin_dir(&self) -> PathBuf {
        #[cfg(windows)]
        {
            return self.mariadb_basedir().join("lib/plugin");
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            if let Some(directory) = macos_opt_dir("mariadb") {
                return directory.join("lib/plugin");
            }
            self.root.join("mariadb/lib/plugin")
        }
    }

    /// Returns the MariaDB localization directory.
    #[must_use]
    pub fn mariadb_messages_dir(&self) -> PathBuf {
        #[cfg(windows)]
        {
            return self.mariadb_basedir().join("share/mariadb");
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            if let Some(directory) = macos_opt_dir("mariadb") {
                return directory.join("share/mariadb");
            }
            self.root.join("mariadb/share/mariadb")
        }
    }

    /// Returns the MySQL localization directory.
    #[must_use]
    pub fn mysql_messages_dir(&self) -> PathBuf {
        #[cfg(windows)]
        {
            return windows_tool_base("mysqld.exe", "mysql")
                .map_or_else(|| self.root.join("mysql/share"), |base| base.join("share"));
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            if let Some(directory) = macos_opt_dir("mysql") {
                return directory.join("share/mysql");
            }
            self.root.join("mysql/share/mysql")
        }
    }

    /// Returns the PostgreSQL data directory templates (`initdb -L`).
    #[must_use]
    pub fn postgres_share(&self) -> PathBuf {
        #[cfg(windows)]
        {
            let base = windows_tool_base("postgres.exe", "postgresql")
                .unwrap_or_else(|| self.root.join("postgres"));
            let candidates = [base.join("share/postgresql"), base.join("share")];
            return candidates
                .into_iter()
                .find(|path| path.join("postgres.bki").is_file())
                .unwrap_or_else(|| base.join("share"));
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            if let Some(directory) = macos_postgres_dir() {
                return macos_postgres_share(&directory);
            }
            self.root.join("postgres/share")
        }
    }

    /// Returns the PostgreSQL extension library directory.
    #[must_use]
    pub fn postgres_lib(&self) -> PathBuf {
        #[cfg(windows)]
        {
            return windows_tool_base("postgres.exe", "postgresql")
                .unwrap_or_else(|| self.root.join("postgres"))
                .join("lib");
        }
        #[cfg(not(windows))]
        {
            #[cfg(target_os = "macos")]
            if let Some(directory) = macos_postgres_dir() {
                return directory.join("lib/postgresql");
            }
            self.root.join("postgres/lib")
        }
    }

    /// Builds the private native-library search path for managed processes.
    #[must_use]
    pub fn library_path(&self) -> String {
        #[cfg(windows)]
        {
            // Windows resolves DLLs from the executable directory and PATH, so
            // no extra library path is required for Scoop/Chocolatey installs.
            String::new()
        }
        #[cfg(not(windows))]
        {
            [
                "lib",
                "php/8.5/lib",
                "nginx/lib",
                "mariadb/lib",
                "redis/lib",
                "mysql/lib",
                "postgres/lib",
            ]
            .into_iter()
            .map(|path| self.root.join(path).display().to_string())
            .collect::<Vec<_>>()
            .join(":")
        }
    }
}

#[cfg(windows)]
fn windows_home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

#[cfg(windows)]
fn scoop_app(name: &str) -> Option<PathBuf> {
    let directory = windows_home()?
        .join("scoop")
        .join("apps")
        .join(name)
        .join("current");
    directory.is_dir().then_some(directory)
}

#[cfg(windows)]
fn choco_root() -> PathBuf {
    std::env::var_os("ChocolateyInstall").map_or_else(
        || PathBuf::from(r"C:\ProgramData\chocolatey"),
        PathBuf::from,
    )
}

#[cfg(windows)]
fn which(name: &str) -> Option<PathBuf> {
    let mut directories: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|value| std::env::split_paths(&value).collect())
        .unwrap_or_default();
    if let Some(home) = windows_home() {
        directories.push(home.join("scoop/shims"));
    }
    directories.push(choco_root().join("bin"));
    directories
        .into_iter()
        .map(|directory| directory.join(name))
        .find(|path| path.is_file())
}

#[cfg(windows)]
fn windows_tool_base(executable: &str, scoop_name: &str) -> Option<PathBuf> {
    scoop_app(scoop_name).or_else(|| {
        which(executable)
            .and_then(|path| path.parent().and_then(Path::parent).map(Path::to_path_buf))
    })
}

#[cfg(windows)]
fn windows_php_directories(version: &str) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    for name in [format!("php{}", version.replace('.', "")), "php".to_owned()] {
        if let Some(directory) = scoop_app(&name) {
            directories.push(directory);
        }
    }
    let tools = PathBuf::from(r"C:\tools");
    directories.push(tools.join(format!("php{version}")));
    directories.push(tools.join("php"));
    if let Some(php) = which("php.exe")
        && let Some(directory) = php.parent()
    {
        directories.push(directory.to_path_buf());
    }
    directories
}

#[cfg(windows)]
fn windows_php_matches(php: &Path, version: &str) -> bool {
    Command::new(php)
        .args(["-n", "-r", "echo PHP_MAJOR_VERSION.'.'.PHP_MINOR_VERSION;"])
        .output()
        .is_ok_and(|output| {
            output.status.success() && String::from_utf8_lossy(&output.stdout).trim() == version
        })
}

#[cfg(target_os = "macos")]
fn homebrew_roots() -> [PathBuf; 2] {
    [PathBuf::from("/opt/homebrew"), PathBuf::from("/usr/local")]
}

#[cfg(target_os = "macos")]
fn macos_opt_dir(formula: &str) -> Option<PathBuf> {
    homebrew_roots()
        .into_iter()
        .map(|root| root.join("opt").join(formula))
        .find(|path| path.is_dir())
}

#[cfg(target_os = "macos")]
fn macos_opt_file(formula: &str, relative: &str) -> Option<PathBuf> {
    macos_opt_dir(formula)
        .map(|directory| directory.join(relative))
        .filter(|path| path.is_file())
}

#[cfg(target_os = "macos")]
fn macos_prefix_file(relative: &str) -> Option<PathBuf> {
    homebrew_roots()
        .into_iter()
        .map(|root| root.join(relative))
        .find(|path| path.is_file())
}

#[cfg(target_os = "macos")]
fn macos_composer_phar() -> Option<PathBuf> {
    for root in homebrew_roots() {
        for candidate in [
            root.join("opt/composer/libexec/composer.phar"),
            root.join("share/composer/composer.phar"),
            root.join("libexec/composer.phar"),
        ] {
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        let cellar = root.join("Cellar/composer");
        let Ok(entries) = fs::read_dir(cellar) else {
            continue;
        };
        let mut versions: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
        versions.sort();
        for version in versions {
            for candidate in [
                version.join("libexec/composer.phar"),
                version.join("composer.phar"),
            ] {
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn macos_postgres_dir() -> Option<PathBuf> {
    [
        "postgresql@17",
        "postgresql@16",
        "postgresql@15",
        "postgresql@14",
        "postgresql",
    ]
    .into_iter()
    .find_map(macos_opt_dir)
}

#[cfg(target_os = "macos")]
fn macos_postgres_share(directory: &Path) -> PathBuf {
    let share = directory.join("share");
    let mut candidates = vec![share.join("postgresql")];
    if let Ok(entries) = fs::read_dir(&share) {
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with("postgresql")
            {
                candidates.push(entry.path());
            }
        }
    }
    candidates
        .into_iter()
        .find(|path| path.is_dir())
        .unwrap_or(share)
}

fn require_file(path: PathBuf, label: &str) -> Result<PathBuf> {
    if path.is_file() {
        Ok(path)
    } else {
        Err(LocalWebError::RuntimeNotInstalled(format!(
            "{label} ({})",
            path.display()
        )))
    }
}

#[cfg(not(windows))]
fn require_pair(first: &Path, second: &Path, label: &str) -> Result<()> {
    if first.is_file() && second.is_file() {
        Ok(())
    } else {
        Err(LocalWebError::RuntimeNotInstalled(format!(
            "{label} CLI and FPM under {}",
            first.parent().unwrap_or(first).display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bundled_layout_is_deterministic() {
        let layout = RuntimeLayout::from_root(PathBuf::from("/opt/localweb-runtime"));
        assert_eq!(
            layout.manifest_path(),
            PathBuf::from("/opt/localweb-runtime/runtime-manifest.json")
        );
        assert!(
            layout
                .library_path()
                .starts_with("/opt/localweb-runtime/lib:")
        );
    }
}

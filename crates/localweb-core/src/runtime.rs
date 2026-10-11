//! Resolves the private runtime payload inside a native `LocalWeb` package.

use crate::{LocalWebError, Result, validate_php_version};
use serde::{Deserialize, Serialize};
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
        let base = self.root.join("php").join(version);
        let php = base.join("bin/php");
        let fpm = base.join("sbin/php-fpm");
        require_pair(&php, &fpm, &format!("bundled PHP {version}"))?;
        Ok((php, fpm))
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
        require_file(
            self.root.join("composer/2/composer.phar"),
            "bundled Composer 2",
        )
    }

    /// Resolves the bundled nginx executable.
    ///
    /// # Errors
    ///
    /// Returns an error when the executable is missing.
    pub fn nginx(&self) -> Result<PathBuf> {
        require_file(self.root.join("nginx/sbin/nginx"), "bundled nginx")
    }

    /// Resolves the bundled Apache executable.
    ///
    /// # Errors
    ///
    /// Returns an error when the executable is missing.
    pub fn apache(&self) -> Result<PathBuf> {
        require_file(self.root.join("apache/bin/apache2"), "bundled Apache")
    }

    /// Resolves the bundled Node.js executable.
    ///
    /// # Errors
    ///
    /// Returns an error when the executable is missing.
    pub fn node(&self) -> Result<PathBuf> {
        require_file(self.root.join("node/bin/node"), "bundled Node.js")
    }

    /// Resolves the bundled npm CLI entry point.
    ///
    /// # Errors
    ///
    /// Returns an error when the npm payload is missing.
    pub fn npm_cli(&self) -> Result<PathBuf> {
        require_file(self.root.join("node/npm/bin/npm-cli.js"), "bundled npm")
    }

    /// Resolves the bundled Mailpit executable.
    ///
    /// # Errors
    ///
    /// Returns an error when the executable is missing.
    pub fn mailpit(&self) -> Result<PathBuf> {
        require_file(self.root.join("mailpit/bin/mailpit"), "bundled Mailpit")
    }

    /// Resolves MariaDB server, initializer, and client executables.
    ///
    /// # Errors
    ///
    /// Returns an error when any required executable is missing.
    pub fn mariadb(&self) -> Result<(PathBuf, PathBuf, PathBuf)> {
        let base = self.root.join("mariadb");
        let server = require_file(base.join("bin/mariadbd"), "bundled MariaDB server")?;
        let install = require_file(
            base.join("bin/mariadb-install-db"),
            "bundled MariaDB initializer",
        )?;
        let client = require_file(base.join("bin/mariadb"), "bundled MariaDB client")?;
        Ok((server, install, client))
    }

    /// Resolves MySQL server and client executables.
    ///
    /// # Errors
    ///
    /// Returns an error when any required executable is missing.
    pub fn mysql(&self) -> Result<(PathBuf, PathBuf)> {
        let base = self.root.join("mysql");
        let server = require_file(base.join("bin/mysqld"), "bundled MySQL server")?;
        let client = require_file(base.join("bin/mysql"), "bundled MySQL client")?;
        Ok((server, client))
    }

    /// Resolves the PostgreSQL server, initializer, and client.
    ///
    /// # Errors
    /// Returns an error when the PostgreSQL payload is incomplete.
    pub fn postgres(&self) -> Result<(PathBuf, PathBuf, PathBuf)> {
        let bin = self.root.join("postgres/bin");
        Ok((
            require_file(bin.join("postgres"), "bundled PostgreSQL server")?,
            require_file(bin.join("initdb"), "bundled PostgreSQL initializer")?,
            require_file(bin.join("psql"), "bundled PostgreSQL client")?,
        ))
    }

    /// Resolves Redis server and client executables.
    ///
    /// # Errors
    ///
    /// Returns an error when either executable is missing.
    pub fn redis(&self) -> Result<(PathBuf, PathBuf)> {
        let base = self.root.join("redis/bin");
        Ok((
            require_file(base.join("redis-server"), "bundled Redis server")?,
            require_file(base.join("redis-cli"), "bundled Redis client")?,
        ))
    }

    /// Builds the private native-library search path for managed processes.
    #[must_use]
    pub fn library_path(&self) -> String {
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

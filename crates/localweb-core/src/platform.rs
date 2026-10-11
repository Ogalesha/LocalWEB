//! Native certificate-store integration for supported Linux distributions.

use crate::{LocalWebError, Result};
use std::{fs, path::Path, process::Command};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustStore {
    Ubuntu,
    Arch,
}

impl TrustStore {
    /// Detects the native certificate store from the host OS.
    ///
    /// # Errors
    /// Returns an error for an unreadable or unsupported OS definition.
    pub fn detect() -> Result<Self> {
        Self::from_os_release(&fs::read_to_string("/etc/os-release")?)
    }

    /// Detects the trust-store family from an OS release definition.
    ///
    /// # Errors
    /// Returns an error for an unsupported distribution.
    pub fn from_os_release(contents: &str) -> Result<Self> {
        let values = contents
            .lines()
            .filter_map(|line| line.split_once('='))
            .collect::<std::collections::BTreeMap<_, _>>();
        let id = values
            .get("ID")
            .copied()
            .unwrap_or_default()
            .trim_matches('"');
        let like = values
            .get("ID_LIKE")
            .copied()
            .unwrap_or_default()
            .trim_matches('"');
        let ids = format!("{id} {like}");
        if ids.split_whitespace().any(|value| value == "ubuntu") {
            Ok(Self::Ubuntu)
        } else if ids
            .split_whitespace()
            .any(|value| matches!(value, "arch" | "omarchy"))
        {
            Ok(Self::Arch)
        } else {
            Err(LocalWebError::Config(format!(
                "unsupported certificate store on {id}"
            )))
        }
    }

    #[must_use]
    pub fn certificate_path(self) -> &'static Path {
        Path::new(match self {
            Self::Ubuntu => "/usr/local/share/ca-certificates/localweb-development-ca.crt",
            Self::Arch => "/etc/ca-certificates/trust-source/anchors/localweb-development-ca.crt",
        })
    }

    /// Updates the native trust database after installing the certificate.
    ///
    /// # Errors
    /// Returns an error if the native trust command fails.
    pub fn refresh(self) -> Result<()> {
        let (program, args): (&str, Vec<&str>) = match self {
            Self::Ubuntu => ("/usr/sbin/update-ca-certificates", vec![]),
            Self::Arch => ("/usr/bin/trust", vec!["extract-compat"]),
        };
        if !Command::new(program).args(args).status()?.success() {
            return Err(LocalWebError::Service(format!(
                "{program} failed to update certificate trust"
            )));
        }
        Ok(())
    }

    #[must_use]
    pub fn trusted(self, certificate: &Path) -> bool {
        fs::read(certificate).ok().filter(|bytes| !bytes.is_empty())
            == fs::read(self.certificate_path()).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_supported_linux_trust_stores_and_derivatives() {
        for (text, expected) in [
            ("ID=ubuntu\nID_LIKE=debian", TrustStore::Ubuntu),
            ("ID_LIKE=ubuntu", TrustStore::Ubuntu),
            ("ID=arch", TrustStore::Arch),
            ("ID=omarchy\nID_LIKE=arch", TrustStore::Arch),
        ] {
            assert_eq!(TrustStore::from_os_release(text).unwrap(), expected);
        }
        assert!(TrustStore::from_os_release("ID=unknown").is_err());
    }
}

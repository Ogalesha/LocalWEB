//! Native certificate-store integration for supported operating systems.

use crate::{LocalWebError, Result};
use std::{fs, path::Path, path::PathBuf, process::Command};

/// Common name embedded in every generated `LocalWeb` development CA.
pub const CA_COMMON_NAME: &str = "LocalWeb Development CA";

#[cfg(target_os = "macos")]
const MACOS_CERTIFICATE_PATH: &str =
    "/Library/Application Support/LocalWeb/localweb-development-ca.crt";
#[cfg(target_os = "macos")]
const MACOS_KEYCHAIN: &str = "/Library/Keychains/System.keychain";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustStore {
    Ubuntu,
    Arch,
    #[cfg(target_os = "macos")]
    Macos,
    #[cfg(windows)]
    Windows,
}

impl TrustStore {
    /// Detects the native certificate store from the host OS.
    ///
    /// # Errors
    /// Returns an error for an unreadable or unsupported OS definition.
    pub fn detect() -> Result<Self> {
        #[cfg(target_os = "macos")]
        {
            Ok(Self::Macos)
        }
        #[cfg(windows)]
        {
            Ok(Self::Windows)
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            Self::from_os_release(&fs::read_to_string("/etc/os-release")?)
        }
    }

    /// Detects the trust-store family from an OS release definition.
    ///
    /// # Errors
    /// Returns an error for an unsupported distribution.
    #[cfg(unix)]
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
    pub fn certificate_path(self) -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            let _ = self;
            PathBuf::from(MACOS_CERTIFICATE_PATH)
        }
        #[cfg(windows)]
        {
            let _ = self;
            let base = std::env::var_os("LOCALAPPDATA")
                .map_or_else(std::env::temp_dir, PathBuf::from)
                .join("LocalWeb")
                .join("data")
                .join("certs");
            base.join("localweb-development-ca.crt")
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            PathBuf::from(match self {
                Self::Ubuntu => "/usr/local/share/ca-certificates/localweb-development-ca.crt",
                Self::Arch => {
                    "/etc/ca-certificates/trust-source/anchors/localweb-development-ca.crt"
                }
            })
        }
    }

    /// Adds the installed certificate to the native trust database.
    ///
    /// # Errors
    /// Returns an error if the native trust command fails.
    pub fn trust_certificate(self) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            let _ = self;
            macos_trust()
        }
        #[cfg(windows)]
        {
            let _ = self;
            windows_certutil(&[
                "-user",
                "-addstore",
                "-f",
                "Root",
                &self.certificate_path().to_string_lossy(),
            ])
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
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
    }

    /// Removes `LocalWeb`'s certificate from the native trust database.
    ///
    /// # Errors
    /// Returns an error if the native trust command fails.
    pub fn untrust_certificate(self) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            let _ = self;
            macos_untrust()
        }
        #[cfg(windows)]
        {
            let _ = self;
            windows_certutil(&["-user", "-delstore", "Root", CA_COMMON_NAME])
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            self.trust_certificate()
        }
    }

    #[must_use]
    pub fn trusted(self, certificate: &Path) -> bool {
        #[cfg(target_os = "macos")]
        {
            let Ok(bytes) = fs::read(certificate) else {
                return false;
            };
            if bytes.is_empty() || fs::read(self.certificate_path()).ok() != Some(bytes) {
                return false;
            }
            macos_keychain_contains(certificate)
        }
        #[cfg(windows)]
        {
            let _ = self;
            let Ok(bytes) = fs::read(certificate) else {
                return false;
            };
            if bytes.is_empty() || fs::read(self.certificate_path()).ok() != Some(bytes) {
                return false;
            }
            windows_store_contains(certificate)
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            fs::read(certificate).ok().filter(|bytes| !bytes.is_empty())
                == fs::read(self.certificate_path()).ok()
        }
    }
}

#[cfg(target_os = "macos")]
fn macos_trust() -> Result<()> {
    let certificate = TrustStore::Macos.certificate_path();
    if macos_keychain_contains(&certificate) {
        return Ok(());
    }
    let status = Command::new("/usr/bin/security")
        .args(["add-trusted-cert", "-d", "-r", "trustRoot", "-k"])
        .arg(MACOS_KEYCHAIN)
        .arg(&certificate)
        .status()?;
    if !status.success() {
        return Err(LocalWebError::Service(
            "security add-trusted-cert failed to trust the LocalWeb CA".into(),
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_untrust() -> Result<()> {
    let status = Command::new("/usr/bin/security")
        .args(["delete-certificate", "-c", CA_COMMON_NAME, MACOS_KEYCHAIN])
        .status()?;
    if !status.success() {
        // `security` exits non-zero when the certificate is already absent.
        return Ok(());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn macos_keychain_contains(certificate: &Path) -> bool {
    let Some(fingerprint) = sha1_fingerprint(certificate) else {
        return false;
    };
    let Ok(output) = Command::new("/usr/bin/security")
        .args(["find-certificate", "-Z", "-a", "-c", CA_COMMON_NAME])
        .arg(MACOS_KEYCHAIN)
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once(':'))
        .any(|(_, value)| normalize_fingerprint(value) == fingerprint)
}

#[cfg(target_os = "macos")]
fn sha1_fingerprint(certificate: &Path) -> Option<String> {
    let output = Command::new("/usr/bin/openssl")
        .args(["x509", "-noout", "-fingerprint", "-sha1", "-in"])
        .arg(certificate)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let (_, value) = text.split_once('=')?;
    Some(normalize_fingerprint(value))
}

#[cfg(windows)]
fn windows_certutil(args: &[&str]) -> Result<()> {
    let output = Command::new("certutil").args(args).output()?;
    if !output.status.success() {
        return Err(LocalWebError::Service(format!(
            "certutil {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}

#[cfg(windows)]
fn windows_store_contains(certificate: &Path) -> bool {
    let Some(fingerprint) = windows_sha1_fingerprint(certificate) else {
        return false;
    };
    let Ok(output) = Command::new("certutil")
        .args(["-user", "-store", "Root"])
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once(':'))
        .any(|(label, value)| {
            label.trim().eq_ignore_ascii_case("Cert Hash(sha1)")
                && normalize_fingerprint(value) == fingerprint
        })
}

#[cfg(windows)]
fn windows_sha1_fingerprint(certificate: &Path) -> Option<String> {
    let output = Command::new("certutil")
        .arg("-hashfile")
        .arg(certificate)
        .arg("SHA1")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| {
            let cleaned = normalize_fingerprint(line);
            (cleaned.len() == 40).then_some(cleaned)
        })
}

#[cfg(any(target_os = "macos", windows))]
fn normalize_fingerprint(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_hexdigit)
        .collect::<String>()
        .to_uppercase()
}

#[cfg(all(test, unix))]
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
        TrustStore::from_os_release("ID=unknown").unwrap_err();
    }
}

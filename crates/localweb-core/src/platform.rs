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

/// Installs the `LocalWeb` CA into the per-user NSS database used by
/// Chromium-family browsers on Linux.
///
/// # Errors
/// Returns an error when the user home cannot be resolved or `certutil` fails.
pub fn install_browser_ca(certificate: &Path) -> Result<()> {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        install_linux_browser_ca(certificate, &browser_nss_database()?)
    }
    #[cfg(any(not(unix), target_os = "macos"))]
    {
        let _ = certificate;
        Ok(())
    }
}

/// Removes the `LocalWeb` CA from the per-user Linux browser NSS database.
///
/// # Errors
/// Returns an error when `certutil` cannot remove an existing entry.
pub fn remove_browser_ca() -> Result<()> {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        remove_linux_browser_ca(&browser_nss_database()?)
    }
    #[cfg(any(not(unix), target_os = "macos"))]
    {
        Ok(())
    }
}

/// Reports whether Chromium-family browsers trust the exact `LocalWeb` CA.
#[must_use]
pub fn browser_ca_trusted(certificate: &Path) -> bool {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        browser_nss_database()
            .is_ok_and(|database| linux_browser_ca_trusted(certificate, &database))
    }
    #[cfg(any(not(unix), target_os = "macos"))]
    {
        let _ = certificate;
        true
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
const NSS_CERTUTIL: &str = "/usr/bin/certutil";

#[cfg(all(unix, not(target_os = "macos")))]
fn browser_nss_database() -> Result<PathBuf> {
    let user_home = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| LocalWebError::Config("HOME is required for browser trust setup".into()))?;
    Ok(nss_database_for_home(&user_home))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn nss_database_for_home(user_home: &Path) -> PathBuf {
    user_home.join(".pki/nssdb")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn nss_database_argument(database: &Path) -> std::ffi::OsString {
    let mut argument = std::ffi::OsString::from("sql:");
    argument.push(database.as_os_str());
    argument
}

#[cfg(all(unix, not(target_os = "macos")))]
fn install_linux_browser_ca(certificate: &Path, database: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    use std::process::Stdio;

    if !Path::new(NSS_CERTUTIL).is_file() {
        return Err(LocalWebError::RuntimeNotInstalled(
            "NSS certutil (/usr/bin/certutil); reinstall the LocalWeb package".into(),
        ));
    }
    fs::create_dir_all(database)?;
    fs::set_permissions(database, fs::Permissions::from_mode(0o700))?;
    let database_argument = nss_database_argument(database);
    if !database.join("cert9.db").is_file() {
        let output = Command::new(NSS_CERTUTIL)
            .args(["-N", "--empty-password", "-d"])
            .arg(&database_argument)
            .stdin(Stdio::null())
            .output()?;
        require_certutil_success(&output, "initialize the browser certificate database")?;
    }
    if linux_browser_ca_trusted(certificate, database) {
        return Ok(());
    }
    if linux_browser_nickname_exists(database) {
        let output = Command::new(NSS_CERTUTIL)
            .args(["-D", "-d"])
            .arg(&database_argument)
            .args(["-n", CA_COMMON_NAME])
            .stdin(Stdio::null())
            .output()?;
        require_certutil_success(&output, "replace the browser CA")?;
    }
    let output = Command::new(NSS_CERTUTIL)
        .args(["-A", "-d"])
        .arg(&database_argument)
        .args(["-n", CA_COMMON_NAME, "-t", "C,,", "-a", "-i"])
        .arg(certificate)
        .stdin(Stdio::null())
        .output()?;
    require_certutil_success(&output, "install the browser CA")?;
    if !linux_browser_ca_trusted(certificate, database) {
        return Err(LocalWebError::Service(
            "certutil completed but Brave/Chromium does not trust the LocalWeb CA".into(),
        ));
    }
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn remove_linux_browser_ca(database: &Path) -> Result<()> {
    if !database.join("cert9.db").is_file() || !linux_browser_nickname_exists(database) {
        return Ok(());
    }
    let output = Command::new(NSS_CERTUTIL)
        .args(["-D", "-d"])
        .arg(nss_database_argument(database))
        .args(["-n", CA_COMMON_NAME])
        .stdin(std::process::Stdio::null())
        .output()?;
    require_certutil_success(&output, "remove the browser CA")
}

#[cfg(all(unix, not(target_os = "macos")))]
fn linux_browser_nickname_exists(database: &Path) -> bool {
    Command::new(NSS_CERTUTIL)
        .args(["-L", "-d"])
        .arg(nss_database_argument(database))
        .args(["-n", CA_COMMON_NAME])
        .stdin(std::process::Stdio::null())
        .output()
        .is_ok_and(|output| output.status.success())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn linux_browser_ca_trusted(certificate: &Path, database: &Path) -> bool {
    let Ok(expected) = fs::read(certificate) else {
        return false;
    };
    let Ok(output) = Command::new(NSS_CERTUTIL)
        .args(["-L", "-a", "-d"])
        .arg(nss_database_argument(database))
        .args(["-n", CA_COMMON_NAME])
        .stdin(std::process::Stdio::null())
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    certificate_der(&expected).is_some_and(|expected| {
        certificate_der(&output.stdout).is_some_and(|actual| actual == expected)
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn certificate_der(bytes: &[u8]) -> Option<Vec<u8>> {
    x509_parser::pem::parse_x509_pem(bytes)
        .ok()
        .map(|(_, pem)| pem.contents)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn require_certutil_success(output: &std::process::Output, action: &str) -> Result<()> {
    if output.status.success() {
        return Ok(());
    }
    Err(LocalWebError::Service(format!(
        "certutil could not {action}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )))
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

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn nss_database_path_and_argument_are_user_scoped() {
        let database = nss_database_for_home(Path::new("/home/developer"));
        assert_eq!(database, Path::new("/home/developer/.pki/nssdb"));
        assert_eq!(
            nss_database_argument(&database),
            std::ffi::OsString::from("sql:/home/developer/.pki/nssdb")
        );
    }
}

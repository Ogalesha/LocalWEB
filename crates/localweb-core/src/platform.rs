//! Native certificate-store integration for supported operating systems.

use crate::{LocalWebError, Result};
use std::{fs, path::Path, path::PathBuf, process::Command};

/// Common name embedded in every generated `LocalWeb` development CA.
pub const CA_COMMON_NAME: &str = "LocalWeb Development CA";

/// Resolves the native Windows system directory without trusting PATH,
/// the current directory or environment overrides.
///
/// # Errors
/// Returns an error if Windows cannot provide a complete absolute path.
#[cfg(windows)]
pub fn windows_system_directory() -> std::io::Result<PathBuf> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt as _};
    let mut buffer = vec![0u16; 32_768];
    // SAFETY: buffer is writable and has the stated capacity in UTF-16 units.
    let length = unsafe {
        windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW(
            buffer.as_mut_ptr(),
            32_768,
        )
    };
    if length == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let length = usize::try_from(length).map_err(std::io::Error::other)?;
    if length >= buffer.len() {
        return Err(std::io::Error::other(
            "Windows system directory is too long",
        ));
    }
    let directory = PathBuf::from(OsString::from_wide(&buffer[..length]));
    if !directory.is_absolute() {
        return Err(std::io::Error::other(
            "Windows system directory is not absolute",
        ));
    }
    Ok(directory)
}

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
            install_current_user_ca(&self.certificate_path())
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
            remove_current_user_ca(&self.certificate_path())
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
            windows_certificate_present(certificate).unwrap_or(false)
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
        for database in browser_nss_databases()? {
            install_linux_browser_ca(certificate, &database)?;
        }
        Ok(())
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
        for database in browser_nss_databases()? {
            remove_linux_browser_ca(&database)?;
        }
        Ok(())
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
        browser_nss_databases().is_ok_and(|databases| {
            databases
                .iter()
                .all(|database| linux_browser_ca_trusted(certificate, database))
        })
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
fn browser_nss_databases() -> Result<Vec<PathBuf>> {
    let user_home = std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| LocalWebError::Config("HOME is required for browser trust setup".into()))?;
    Ok(nss_databases_for_home(&user_home))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn nss_databases_for_home(user_home: &Path) -> Vec<PathBuf> {
    let legacy = user_home.join(".pki/nssdb");
    let current = user_home.join(".local/share/pki/nssdb");
    let mut databases = Vec::new();
    if legacy.is_dir() {
        databases.push(legacy);
    }
    // Chromium 146+ defaults to the new location, but falls back to an
    // existing legacy database. A running browser can still have the newer
    // database open when an older installer creates the legacy directory.
    if current.is_dir() || databases.is_empty() {
        databases.push(current);
    }
    databases
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
    let matches = certificate_der(&expected).is_some_and(|expected| {
        certificate_der(&output.stdout).is_some_and(|actual| actual == expected)
    });
    if !matches {
        return false;
    }
    let Ok(output) = Command::new(NSS_CERTUTIL)
        .args(["-L", "-d"])
        .arg(nss_database_argument(database))
        .stdin(std::process::Stdio::null())
        .output()
    else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout).lines().any(|line| {
            let Some((nickname, flags)) = line.trim_end().rsplit_once(char::is_whitespace) else {
                return false;
            };
            nickname.trim_end() == CA_COMMON_NAME
                && flags
                    .split(',')
                    .next()
                    .is_some_and(|ssl| ssl.contains('C') && !ssl.contains('p'))
        })
}

#[cfg(any(windows, all(unix, not(target_os = "macos"))))]
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

/// Trusts the CA in the caller's Windows profile, without changing accounts
/// through UAC. Both the desktop CLI and MCP use this operation.
///
/// # Errors
/// Returns an error if import fails or the exact certificate cannot be found.
#[cfg(windows)]
pub fn install_current_user_ca(certificate: &Path) -> Result<()> {
    if windows_certificate_present(certificate)? {
        return Ok(());
    }
    let output = windows_certutil_command()?
        .args(["-user", "-addstore", "-f", "Root"])
        .arg(certificate)
        .output()?;
    if !output.status.success() {
        return Err(windows_certificate_error(&output));
    }
    if !windows_certificate_present(certificate)? {
        return Err(LocalWebError::Service(
            "Windows did not trust the HTTPS certificate for this account".into(),
        ));
    }
    Ok(())
}

/// Revokes only the exact CA in the caller's Windows certificate store.
///
/// # Errors
/// Returns an error if querying, deleting or verifying removal fails.
#[cfg(windows)]
pub fn remove_current_user_ca(certificate: &Path) -> Result<()> {
    if !windows_certificate_present(certificate)? {
        return Ok(());
    }
    let fingerprint = windows_sha1_fingerprint(certificate)?;
    let output = windows_certutil_command()?
        .args(["-user", "-silent", "-delstore", "Root", &fingerprint])
        .output()?;
    if !output.status.success() {
        return Err(windows_certificate_error(&output));
    }
    if windows_certificate_present(certificate)? {
        return Err(LocalWebError::Service(
            "Windows still trusts the HTTPS certificate for this account".into(),
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn windows_certutil_command() -> Result<Command> {
    use std::os::windows::process::CommandExt as _;
    let mut command = Command::new(windows_system_directory()?.join("certutil.exe"));
    command
        .creation_flags(0x0800_0000)
        .stdin(std::process::Stdio::null());
    Ok(command)
}

#[cfg(windows)]
fn windows_certificate_error(output: &std::process::Output) -> LocalWebError {
    let details = if output.stderr.is_empty() {
        &output.stdout
    } else {
        &output.stderr
    };
    LocalWebError::Service(format!(
        "Windows certificate command failed ({}): {}",
        output.status,
        String::from_utf8_lossy(details)
            .trim()
            .chars()
            .take(2000)
            .collect::<String>()
    ))
}

#[cfg(windows)]
fn windows_certificate_present(certificate: &Path) -> Result<bool> {
    use windows_sys::Win32::Foundation::{CRYPT_E_NOT_FOUND, NTE_NOT_FOUND};

    let fingerprint = windows_sha1_fingerprint(certificate)?;
    let output = windows_certutil_command()?
        .args(["-user", "-silent", "-store", "Root", &fingerprint])
        .output()?;
    if output.status.success() {
        return Ok(true);
    }
    if matches!(
        output.status.code(),
        Some(CRYPT_E_NOT_FOUND | NTE_NOT_FOUND)
    ) {
        return Ok(false);
    }
    Err(windows_certificate_error(&output))
}

#[cfg(windows)]
fn windows_sha1_fingerprint(certificate: &Path) -> Result<String> {
    use std::fmt::Write as _;
    let der = certificate_der(&fs::read(certificate)?)
        .ok_or_else(|| LocalWebError::Config("invalid PEM certificate".into()))?;
    // SHA-1 is the Windows store's legacy certificate identifier. Hash the
    // DER certificate, not the surrounding PEM text or a signature input.
    let digest = ring::digest::digest(&ring::digest::SHA1_FOR_LEGACY_USE_ONLY, &der);
    let mut fingerprint = String::with_capacity(40);
    for byte in digest.as_ref() {
        write!(fingerprint, "{byte:02X}").unwrap();
    }
    Ok(fingerprint)
}

#[cfg(target_os = "macos")]
fn normalize_fingerprint(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_hexdigit)
        .collect::<String>()
        .to_uppercase()
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn system_tools_resolve_inside_the_native_system_directory() {
        let directory = super::windows_system_directory().unwrap();
        assert!(directory.is_absolute());
        assert!(directory.join("cmd.exe").is_file());
    }

    #[test]
    #[ignore = "temporarily modifies the current user's Root store; run explicitly on an isolated Windows runner"]
    fn windows_current_user_ca_install_query_and_remove() {
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = remove_current_user_ca(&self.0);
            }
        }
        let base = std::env::temp_dir().join(format!("lw-user-ca-{}", uuid::Uuid::new_v4()));
        let paths = crate::Paths {
            config: base.join("c"),
            data: base.join("d"),
            state: base.join("s"),
            runtime: base.join("r"),
        };
        paths.create().unwrap();
        let ca = crate::certificate::CaPaths::new(&paths);
        ca.ensure_ca().unwrap();
        let cleanup = Cleanup(ca.cert.clone());
        println!(
            "LocalWeb test CA thumbprint: {}",
            windows_sha1_fingerprint(&ca.cert).unwrap()
        );
        assert!(!TrustStore::Windows.trusted(&ca.cert));
        println!("Importing the test CA; Windows may request confirmation");
        install_current_user_ca(&ca.cert).unwrap();
        assert!(TrustStore::Windows.trusted(&ca.cert));
        install_current_user_ca(&ca.cert).unwrap();
        let alternate = base.join("same-ca-crlf.pem");
        fs::write(
            &alternate,
            fs::read_to_string(&ca.cert).unwrap().replace('\n', "\r\n"),
        )
        .unwrap();
        assert!(
            TrustStore::Windows.trusted(&alternate),
            "trust follows DER identity, not PEM file bytes"
        );
        println!("Removing the test CA");
        remove_current_user_ca(&alternate).unwrap();
        assert!(!TrustStore::Windows.trusted(&ca.cert));
        remove_current_user_ca(&ca.cert).unwrap();
        drop(cleanup);
        fs::remove_dir_all(base).unwrap();
    }
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
    fn nss_database_selection_handles_modern_legacy_and_existing_browsers() {
        let home = std::env::temp_dir().join(format!("localweb nss {}", uuid::Uuid::new_v4()));
        let current = home.join(".local/share/pki/nssdb");
        let legacy = home.join(".pki/nssdb");
        assert_eq!(nss_databases_for_home(&home), vec![current.clone()]);
        fs::create_dir_all(&legacy).unwrap();
        assert_eq!(nss_databases_for_home(&home), vec![legacy.clone()]);
        fs::create_dir_all(&current).unwrap();
        assert_eq!(nss_databases_for_home(&home), vec![legacy, current.clone()]);
        let mut expected = std::ffi::OsString::from("sql:");
        expected.push(&current);
        assert_eq!(nss_database_argument(&current), expected);
        fs::remove_dir_all(home).unwrap();
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn browser_trust_repairs_missing_ssl_flags_and_preserves_other_entries() {
        if !Path::new(NSS_CERTUTIL).is_file() {
            eprintln!("NSS regression needs libnss3-tools or nss");
            return;
        }
        let base = std::env::temp_dir().join(format!("localweb nss {}", uuid::Uuid::new_v4()));
        let paths = crate::Paths {
            config: base.join("config"),
            data: base.join("data"),
            state: base.join("state"),
            runtime: base.join("runtime"),
        };
        let ca = crate::certificate::CaPaths::new(&paths);
        ca.ensure_ca().unwrap();
        let database = base.join("nssdb");
        install_linux_browser_ca(&ca.cert, &database).unwrap();
        assert!(linux_browser_ca_trusted(&ca.cert, &database));
        let argument = nss_database_argument(&database);
        let result = Command::new(NSS_CERTUTIL)
            .args(["-M", "-d"])
            .arg(&argument)
            .args(["-n", CA_COMMON_NAME, "-t", ",,"])
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        assert!(!linux_browser_ca_trusted(&ca.cert, &database));
        install_linux_browser_ca(&ca.cert, &database).unwrap();
        assert!(linux_browser_ca_trusted(&ca.cert, &database));
        let other_ca = crate::certificate::CaPaths::new(&crate::Paths {
            data: base.join("other"),
            ..paths
        });
        other_ca.ensure_ca().unwrap();
        assert!(!linux_browser_ca_trusted(&other_ca.cert, &database));
        let result = Command::new(NSS_CERTUTIL)
            .args(["-A", "-d"])
            .arg(&argument)
            .args(["-n", "Unrelated CA", "-t", "C,,", "-a", "-i"])
            .arg(&other_ca.cert)
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        remove_linux_browser_ca(&database).unwrap();
        assert!(!linux_browser_ca_trusted(&ca.cert, &database));
        let result = Command::new(NSS_CERTUTIL)
            .args(["-L", "-d"])
            .arg(&argument)
            .args(["-n", "Unrelated CA"])
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        fs::remove_dir_all(base).unwrap();
    }
}

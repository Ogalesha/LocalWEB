use anyhow::{Context, Result, bail};
use localweb_core::{
    atomic_write,
    hosts::{remove_managed_block, transform_hosts},
    platform::TrustStore,
};
#[cfg(unix)]
use serde::Deserialize;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(unix)]
use std::process::Command;
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const MAX_INPUT: u64 = 64 * 1024;
#[cfg(all(unix, not(target_os = "macos")))]
const GATEWAY: &str = "/usr/lib/localweb/localweb-gateway";
#[cfg(target_os = "macos")]
const USER_REGISTRY: &str = "/usr/local/var/lib/localweb/users";
#[cfg(all(unix, not(target_os = "macos")))]
const USER_REGISTRY: &str = "/var/lib/localweb/users";

#[derive(Debug, Deserialize)]
#[cfg(unix)]
struct UserPaths {
    config: PathBuf,
    data: PathBuf,
    state: PathBuf,
    runtime: PathBuf,
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let operation = args.next().context("missing helper operation")?;
    let input_file = match (args.next(), args.next()) {
        (None, None) => None,
        (Some(flag), Some(path)) if flag == "--input" => Some(PathBuf::from(path)),
        _ => bail!("unexpected helper arguments"),
    };
    if args.next().is_some() {
        bail!("unexpected helper arguments");
    }
    #[cfg(unix)]
    {
        let uid = Command::new("/usr/bin/id").arg("-u").output()?;
        if !uid.status.success() || String::from_utf8_lossy(&uid.stdout).trim() != "0" {
            bail!("localweb-helper requires administrator authorization");
        }
    }
    #[cfg(windows)]
    require_administrator()?;
    let package_removal = operation == "package-remove"
        && std::env::var("DPKG_MAINTSCRIPT_PACKAGE").as_deref() == Ok("localweb")
        && std::env::var("DPKG_MAINTSCRIPT_NAME").as_deref() == Ok("prerm");
    if !package_removal {
        #[cfg(unix)]
        invoking_uid().context("localweb-helper requires administrator authorization")?;
    }
    match operation.as_str() {
        "sync-hosts" => sync_hosts(input_file.as_deref()),
        "install-ca" => install_ca(input_file.as_deref()),
        "remove-ca" => remove_ca(input_file.as_deref()),
        "set-gateway-cap" => set_gateway_cap(),
        "register-paths" => register_paths(input_file.as_deref()),
        "remove-integration" => remove_system_integration(),
        "package-remove" if package_removal => remove_system_integration(),
        _ => bail!("unknown privileged operation"),
    }
}

#[cfg(unix)]
fn invoking_uid() -> Result<u32> {
    let key = if cfg!(target_os = "macos") {
        "LOCALWEB_INVOKING_UID"
    } else {
        "PKEXEC_UID"
    };
    let raw = std::env::var(key).with_context(|| format!("{key} is required"))?;
    raw.parse::<u32>().context("invalid invoking user id")
}

/// Elevation on Windows is granted by the UAC launcher; verify the token can
/// write the hosts file before performing privileged work.
#[cfg(windows)]
fn require_administrator() -> Result<()> {
    if fs::OpenOptions::new()
        .append(true)
        .open(hosts_path())
        .is_err()
    {
        bail!("localweb-helper requires administrator authorization");
    }
    Ok(())
}

/// Returns the system hosts file for the current platform.
fn hosts_path() -> PathBuf {
    #[cfg(windows)]
    {
        let root = std::env::var_os("SystemRoot")
            .map_or_else(|| PathBuf::from(r"C:\Windows"), PathBuf::from);
        root.join("System32")
            .join("drivers")
            .join("etc")
            .join("hosts")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/etc/hosts")
    }
}

#[cfg(windows)]
fn register_paths(_input: Option<&Path>) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn register_paths(input: Option<&Path>) -> Result<()> {
    let uid = invoking_uid()?;
    if uid == 0 {
        bail!("LocalWeb cannot register root as an application user");
    }
    let paths: UserPaths =
        serde_json::from_slice(&read_input(input)?).context("invalid path registration")?;
    let home = home_for_uid(uid)?;
    #[cfg(target_os = "macos")]
    let runtime_parent = runtime_parent(uid, &paths.runtime)?;
    #[cfg(not(target_os = "macos"))]
    let runtime_parent = runtime_parent(uid, &paths.runtime);
    let registered = [
        validate_user_directory(&paths.config, &home, uid)?,
        validate_user_directory(&paths.data, &home, uid)?,
        validate_user_directory(&paths.state, &home, uid)?,
        validate_user_directory(&paths.runtime, &runtime_parent, uid)?,
    ];
    let contents = registered
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::create_dir_all(USER_REGISTRY)?;
    atomic_write(
        &Path::new(USER_REGISTRY).join(format!("{uid}.paths")),
        contents.as_bytes(),
        0o600,
    )?;
    Ok(())
}

#[cfg(unix)]
fn validate_user_directory(path: &Path, parent: &Path, uid: u32) -> Result<PathBuf> {
    if path
        .as_os_str()
        .to_string_lossy()
        .chars()
        .any(char::is_control)
    {
        bail!("registered paths cannot contain control characters");
    }
    let canonical = path
        .canonicalize()
        .with_context(|| format!("cannot resolve {}", path.display()))?;
    let parent = parent
        .canonicalize()
        .with_context(|| format!("cannot resolve {}", parent.display()))?;
    let metadata = fs::symlink_metadata(&canonical)?;
    if !canonical.starts_with(&parent)
        || canonical == parent
        || !metadata.is_dir()
        || metadata.uid() != uid
    {
        bail!(
            "{} is not an owned directory below {}",
            canonical.display(),
            parent.display()
        );
    }
    Ok(canonical)
}

#[cfg(all(unix, not(target_os = "macos")))]
fn runtime_parent(uid: u32, _runtime: &Path) -> PathBuf {
    PathBuf::from(format!("/run/user/{uid}"))
}

#[cfg(target_os = "macos")]
fn runtime_parent(uid: u32, runtime: &Path) -> Result<PathBuf> {
    let reported =
        std::env::var_os("LOCALWEB_RUNTIME_DIR").context("LOCALWEB_RUNTIME_DIR is required")?;
    if PathBuf::from(reported) != runtime {
        bail!("runtime directory does not match the registration request");
    }
    let parent = runtime
        .parent()
        .context("runtime directory has no parent")?;
    let metadata = fs::symlink_metadata(parent)?;
    if metadata.uid() != uid {
        bail!("runtime directory is not owned by the invoking user");
    }
    Ok(parent.to_path_buf())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn home_for_uid(uid: u32) -> Result<PathBuf> {
    let passwd = fs::read_to_string("/etc/passwd")?;
    passwd
        .lines()
        .find_map(|line| {
            let mut fields = line.split(':');
            let _name = fields.next()?;
            let _password = fields.next()?;
            let parsed_uid = fields.next()?.parse::<u32>().ok()?;
            let _gid = fields.next()?;
            let _gecos = fields.next()?;
            let home = fields.next()?;
            (parsed_uid == uid).then(|| PathBuf::from(home))
        })
        .context("the invoking user is absent from /etc/passwd")
}

#[cfg(target_os = "macos")]
fn home_for_uid(uid: u32) -> Result<PathBuf> {
    let user = Command::new("/usr/bin/id")
        .arg("-un")
        .arg(uid.to_string())
        .output()?;
    if !user.status.success() {
        bail!("cannot resolve the invoking user name");
    }
    let name = String::from_utf8_lossy(&user.stdout).trim().to_owned();
    if name.is_empty() {
        bail!("cannot resolve the invoking user name");
    }
    let record = Command::new("/usr/bin/dscl")
        .args([".", "-read"])
        .arg(format!("/Users/{name}"))
        .arg("NFSHomeDirectory")
        .output()?;
    if !record.status.success() {
        bail!("cannot resolve the invoking user home directory");
    }
    String::from_utf8_lossy(&record.stdout)
        .lines()
        .find_map(|line| line.split_once(": "))
        .map(|(_, home)| PathBuf::from(home.trim()))
        .context("invoking user record has no home directory")
}

fn remove_system_integration() -> Result<()> {
    let hosts = hosts_path();
    ensure_regular_root_owned(&hosts)?;
    let updated = remove_managed_block(&fs::read_to_string(&hosts)?)?;
    atomic_write(&hosts, updated.as_bytes(), 0o644)?;

    let store = TrustStore::detect()?;
    let trust = store.certificate_path();
    if trust.exists() {
        ensure_regular_root_owned(&trust)?;
        fs::remove_file(&trust)?;
        store.untrust_certificate()?;
    }
    Ok(())
}

fn read_input(input: Option<&Path>) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    match input {
        Some(path) => {
            let metadata = fs::symlink_metadata(path)?;
            if !metadata.file_type().is_file() || metadata.len() > MAX_INPUT {
                bail!("input file must be a small regular file");
            }
            fs::File::open(path)?
                .take(MAX_INPUT + 1)
                .read_to_end(&mut data)?;
        }
        None => {
            io::stdin().take(MAX_INPUT + 1).read_to_end(&mut data)?;
        }
    }
    if data.len() as u64 > MAX_INPUT {
        bail!("input exceeds 64 KiB");
    }
    Ok(data)
}

fn sync_hosts(input: Option<&Path>) -> Result<()> {
    let domains: Vec<String> =
        serde_json::from_slice(&read_input(input)?).context("expected JSON hostname array")?;
    if domains.len() > 1000 {
        bail!("too many hostnames");
    }
    let path = hosts_path();
    ensure_regular_root_owned(&path)?;
    let original = fs::read_to_string(&path)?;
    let updated = transform_hosts(&original, &domains)?;
    atomic_write(&path, updated.as_bytes(), 0o644)?;
    Ok(())
}

fn install_ca(input: Option<&Path>) -> Result<()> {
    let pem_bytes = read_input(input)?;
    let text = std::str::from_utf8(&pem_bytes).context("CA certificate must be UTF-8 PEM")?;
    if text.matches("-----BEGIN CERTIFICATE-----").count() != 1 || text.contains("PRIVATE KEY") {
        bail!("exactly one PEM certificate and no private key required");
    }
    let parsed = pem::parse(text).context("invalid PEM")?;
    if parsed.tag() != "CERTIFICATE" {
        bail!("not a certificate");
    }
    let (_, cert) = x509_parser::parse_x509_certificate(parsed.contents())
        .map_err(|error| anyhow::anyhow!("invalid X.509 certificate: {error}"))?;
    let constraints = cert
        .basic_constraints()
        .context("invalid CA basic constraints")?;
    if !constraints.is_some_and(|bc| bc.value.ca) {
        bail!("certificate is not a CA");
    }
    let common_name = cert
        .subject()
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok());
    if common_name != Some("LocalWeb Development CA") {
        bail!("certificate subject is not LocalWeb Development CA");
    }
    let store = TrustStore::detect()?;
    let trust = store.certificate_path();
    if trust.exists() {
        ensure_regular_root_owned(&trust)?;
        if fs::read(&trust)? != pem_bytes {
            bail!(
                "a different LocalWeb CA is already trusted; refusing to overwrite it. Run `localweb ca remove` from the account that installed it after confirming no other account still relies on it"
            );
        }
        store.trust_certificate()?;
        return Ok(());
    }
    atomic_write(&trust, &pem_bytes, 0o644)?;
    store.trust_certificate()?;
    Ok(())
}

fn remove_ca(input: Option<&Path>) -> Result<()> {
    let pem_bytes = read_input(input)?;
    let text = std::str::from_utf8(&pem_bytes).context("CA certificate must be UTF-8 PEM")?;
    if text.matches("-----BEGIN CERTIFICATE-----").count() != 1 || text.contains("PRIVATE KEY") {
        bail!("exactly one PEM certificate and no private key required");
    }
    let store = TrustStore::detect()?;
    let trust = store.certificate_path();
    if !trust.exists() {
        return Ok(());
    }
    ensure_regular_root_owned(&trust)?;
    if fs::read(&trust)? != pem_bytes {
        bail!("the trusted LocalWeb CA belongs to a different account; refusing to remove it");
    }
    fs::remove_file(&trust)?;
    store.untrust_certificate()?;
    Ok(())
}

#[cfg(any(target_os = "macos", windows))]
fn set_gateway_cap() -> Result<()> {
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn set_gateway_cap() -> Result<()> {
    ensure_regular_root_owned(Path::new(GATEWAY))?;
    let tool = if Path::new("/usr/sbin/setcap").is_file() {
        "/usr/sbin/setcap"
    } else {
        "/usr/bin/setcap"
    };
    let status = Command::new(tool)
        .arg("cap_net_bind_service=+ep")
        .arg(GATEWAY)
        .status()?;
    if !status.success() {
        bail!("setcap failed for dedicated gateway binary");
    }
    Ok(())
}

#[cfg(unix)]
fn ensure_regular_root_owned(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.file_type().is_file() || meta.uid() != 0 || meta.nlink() != 1 {
        bail!(
            "{} must be a regular root-owned file with one link",
            path.display()
        );
    }
    Ok(())
}

#[cfg(windows)]
fn ensure_regular_root_owned(path: &Path) -> Result<()> {
    if !fs::symlink_metadata(path)?.file_type().is_file() {
        bail!("{} must be a regular file", path.display());
    }
    Ok(())
}

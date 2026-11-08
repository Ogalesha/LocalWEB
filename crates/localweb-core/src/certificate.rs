//! Creates and preserves the private `LocalWeb` certificate authority.

use crate::{LocalWebError, Paths, Result, atomic_write, validate_domain};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose,
};
use std::{fs, path::PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MaterialState {
    Missing,
    Complete,
    Incomplete,
}

fn material_state(cert: &std::path::Path, key: &std::path::Path) -> Result<MaterialState> {
    let present = |path: &std::path::Path| -> Result<bool> {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_file() => Ok(true),
            Ok(_) => Err(LocalWebError::Config(format!(
                "certificate material must be a regular file: {}",
                path.display()
            ))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    };
    Ok(match (present(cert)?, present(key)?) {
        (false, false) => MaterialState::Missing,
        (true, true) => MaterialState::Complete,
        _ => MaterialState::Incomplete,
    })
}

#[derive(Debug, Clone)]
/// Locates the certificate authority and issued leaf certificates.
pub struct CaPaths {
    /// Public CA certificate path.
    pub cert: PathBuf,
    /// Private CA key path.
    pub key: PathBuf,
    /// Directory containing issued leaf certificates.
    pub leaves: PathBuf,
}

impl CaPaths {
    /// Creates certificate paths below `LocalWeb`'s private data directory.
    #[must_use]
    pub fn new(paths: &Paths) -> Self {
        let base = paths.data.join("certs");
        Self {
            cert: base.join("localweb-ca.crt"),
            key: base.join("localweb-ca.key"),
            leaves: base.join("sites"),
        }
    }

    /// Creates the development certificate authority when absent.
    ///
    /// Existing CA material is preserved. Partial material is reported instead of regenerated.
    ///
    /// # Errors
    ///
    /// Returns an error for incomplete CA state, generation failures, or filesystem failures.
    pub fn ensure_ca(&self) -> Result<()> {
        match material_state(&self.cert, &self.key)? {
            MaterialState::Complete => return validate_ca(&fs::read(&self.cert)?, &fs::read_to_string(&self.key)?),
            MaterialState::Incomplete => return Err(LocalWebError::Config("incomplete CA state; restore the missing file rather than silently replacing the CA".into())),
            MaterialState::Missing => {}
        }
        let mut params = CertificateParams::new(Vec::<String>::new()).map_err(cert_error)?;
        params.distinguished_name = ca_name();
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
        let key = KeyPair::generate().map_err(cert_error)?;
        let cert = params.self_signed(&key).map_err(cert_error)?;
        atomic_write(&self.key, key.serialize_pem().as_bytes(), 0o600)?;
        if let Err(err) = atomic_write(&self.cert, cert.pem().as_bytes(), 0o644) {
            let _ = fs::remove_file(&self.key);
            return Err(err);
        }
        Ok(())
    }

    /// Creates a CA-signed certificate for one validated DNS name.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid domains, incomplete files, cryptographic failures, or I/O.
    pub fn ensure_leaf(&self, domain: &str) -> Result<(PathBuf, PathBuf)> {
        validate_domain(domain)?;
        self.ensure_ca()?;
        let cert_path = self.leaves.join(format!("{domain}.crt"));
        let key_path = self.leaves.join(format!("{domain}.key"));
        let previous_key = match material_state(&cert_path, &key_path)? {
            MaterialState::Complete => {
                let key = fs::read(&key_path)?;
                if leaf_matches(&fs::read(&cert_path)?, &key, &fs::read(&self.cert)?, domain) {
                    return Ok((cert_path, key_path));
                }
                Some(key)
            }
            MaterialState::Incomplete => {
                return Err(LocalWebError::Config(format!(
                    "incomplete certificate state for {domain}"
                )));
            }
            MaterialState::Missing => None,
        };
        let ca_key = KeyPair::from_pem(&fs::read_to_string(&self.key)?).map_err(cert_error)?;
        let ca_params = CertificateParams::from_ca_cert_pem(&fs::read_to_string(&self.cert)?)
            .map_err(cert_error)?;
        let ca_cert = ca_params.self_signed(&ca_key).map_err(cert_error)?;
        let mut params = CertificateParams::new(vec![domain.to_owned()]).map_err(cert_error)?;
        params.distinguished_name.push(DnType::CommonName, domain);
        params.is_ca = IsCa::ExplicitNoCa;
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        params.use_authority_key_identifier_extension = true;
        let key = KeyPair::generate().map_err(cert_error)?;
        let cert = params
            .signed_by(&key, &ca_cert, &ca_key)
            .map_err(cert_error)?;
        fs::create_dir_all(&self.leaves)?;
        atomic_write(&key_path, key.serialize_pem().as_bytes(), 0o600)?;
        if let Err(err) = atomic_write(&cert_path, cert.pem().as_bytes(), 0o644) {
            if let Some(previous) = previous_key {
                atomic_write(&key_path, &previous, 0o600).map_err(|rollback| {
                    LocalWebError::Config(format!(
                        "certificate write failed: {err}; restoring its key also failed: {rollback}"
                    ))
                })?;
            } else {
                let _ = fs::remove_file(&key_path);
            }
            return Err(err);
        }
        Ok((cert_path, key_path))
    }
}

fn validate_ca(pem: &[u8], key: &str) -> Result<()> {
    let key = KeyPair::from_pem(key).map_err(cert_error)?;
    let (_, pem) = x509_parser::pem::parse_x509_pem(pem).map_err(cert_error)?;
    let (_, cert) = x509_parser::parse_x509_certificate(&pem.contents).map_err(cert_error)?;
    if !cert.is_ca()
        || !cert.validity().is_valid()
        || cert.public_key().raw != key.public_key_der()
        || cert.subject() != cert.issuer()
        || cert.verify_signature(None).is_err()
    {
        return Err(LocalWebError::Config(
            "invalid CA certificate or mismatched private key; restore the matching CA files"
                .into(),
        ));
    }
    Ok(())
}

fn leaf_matches(pem: &[u8], key: &[u8], ca: &[u8], domain: &str) -> bool {
    let Some(key) = std::str::from_utf8(key)
        .ok()
        .and_then(|key| KeyPair::from_pem(key).ok())
    else {
        return false;
    };
    let Ok((_, pem)) = x509_parser::pem::parse_x509_pem(pem) else {
        return false;
    };
    let Ok((_, ca)) = x509_parser::pem::parse_x509_pem(ca) else {
        return false;
    };
    let Ok((_, cert)) = x509_parser::parse_x509_certificate(&pem.contents) else {
        return false;
    };
    let Ok((_, ca)) = x509_parser::parse_x509_certificate(&ca.contents) else {
        return false;
    };
    !cert.is_ca() && cert.validity().is_valid() && cert.public_key().raw == key.public_key_der()
        && cert.extensions().iter().any(|extension| matches!(extension.parsed_extension(), x509_parser::extensions::ParsedExtension::AuthorityKeyIdentifier(_)))
        && cert.issuer() == ca.subject() && cert.verify_signature(Some(ca.public_key())).is_ok()
        && cert.subject_alternative_name().ok().flatten().is_some_and(|names| names.value.general_names.iter().any(|name| matches!(name, x509_parser::extensions::GeneralName::DNSName(name) if *name == domain)))
        && cert.extended_key_usage().ok().flatten().is_some_and(|usage| usage.value.server_auth)
}

fn ca_name() -> DistinguishedName {
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, "LocalWeb Development CA");
    name
}

fn cert_error(error: impl std::fmt::Display) -> LocalWebError {
    LocalWebError::Config(format!("certificate error: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (PathBuf, CaPaths) {
        let base = std::env::temp_dir().join(format!("localweb-ca-state-{}", uuid::Uuid::new_v4()));
        let paths = Paths {
            config: base.join("c"),
            data: base.join("d"),
            state: base.join("s"),
            runtime: base.join("r"),
        };
        paths.create().unwrap();
        (base, CaPaths::new(&paths))
    }

    #[test]
    fn mismatched_or_partial_ca_is_preserved_and_rejected() {
        let (base, ca) = fixture();
        ca.ensure_ca().unwrap();
        let certificate = fs::read(&ca.cert).unwrap();
        fs::write(&ca.key, KeyPair::generate().unwrap().serialize_pem()).unwrap();
        assert!(ca.ensure_ca().is_err());
        assert_eq!(fs::read(&ca.cert).unwrap(), certificate);
        fs::remove_file(&ca.key).unwrap();
        assert!(ca.ensure_ca().is_err());
        assert!(!ca.key.exists());
        assert_eq!(fs::read(&ca.cert).unwrap(), certificate);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn cached_leaf_from_another_ca_or_domain_is_reissued() {
        let (base, ca) = fixture();
        let (other_base, other_ca) = fixture();
        let (other_cert, other_key) = other_ca.ensure_leaf("acme.dev").unwrap();
        let (cert, key) = ca.ensure_leaf("acme.dev").unwrap();
        fs::copy(other_cert, &cert).unwrap();
        fs::copy(other_key, &key).unwrap();
        let stale = fs::read(&cert).unwrap();
        ca.ensure_leaf("acme.dev").unwrap();
        assert_ne!(fs::read(&cert).unwrap(), stale);
        assert!(leaf_matches(
            &fs::read(&cert).unwrap(),
            &fs::read(&key).unwrap(),
            &fs::read(&ca.cert).unwrap(),
            "acme.dev"
        ));
        let (different_cert, different_key) = ca.ensure_leaf("different.dev").unwrap();
        fs::copy(different_cert, &cert).unwrap();
        fs::copy(different_key, &key).unwrap();
        ca.ensure_leaf("acme.dev").unwrap();
        assert!(leaf_matches(
            &fs::read(&cert).unwrap(),
            &fs::read(&key).unwrap(),
            &fs::read(&ca.cert).unwrap(),
            "acme.dev"
        ));
        fs::remove_dir_all(base).unwrap();
        fs::remove_dir_all(other_base).unwrap();
    }
    #[test]
    fn ca_is_stable_and_leaf_has_san() {
        let base = std::env::temp_dir().join(format!("localweb-ca-test-{}", uuid::Uuid::new_v4()));
        let paths = Paths {
            config: base.join("config"),
            data: base.join("data"),
            state: base.join("state"),
            runtime: base.join("runtime"),
        };
        paths.create().unwrap();
        let ca = CaPaths::new(&paths);
        ca.ensure_ca().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(&ca.key).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert_eq!(
                fs::metadata(&ca.cert).unwrap().permissions().mode() & 0o777,
                0o644
            );
        };
        let original = fs::read(&ca.cert).unwrap();
        ca.ensure_ca().unwrap();
        assert_eq!(original, fs::read(&ca.cert).unwrap());
        let (cert, key) = ca.ensure_leaf("acme.dev").unwrap();
        let bytes = fs::read(&cert).unwrap();
        let (_, pem) = x509_parser::pem::parse_x509_pem(&bytes).unwrap();
        let (_, leaf) = x509_parser::parse_x509_certificate(&pem.contents).unwrap();
        assert!(leaf.extensions().iter().any(|extension| matches!(
            extension.parsed_extension(),
            x509_parser::extensions::ParsedExtension::AuthorityKeyIdentifier(_)
        )));
        assert!(
            fs::read_to_string(cert)
                .unwrap()
                .contains("BEGIN CERTIFICATE")
        );
        assert!(fs::read_to_string(&key).unwrap().contains("PRIVATE KEY"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(&key).unwrap().permissions().mode() & 0o777,
                0o600
            );
        };
        fs::remove_dir_all(base).unwrap();
    }
}

//! Creates and preserves the private `LocalWeb` certificate authority.

use crate::{LocalWebError, Paths, Result, atomic_write, validate_domain};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa,
    KeyPair, KeyUsagePurpose,
};
use std::{fs, path::PathBuf};

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
        if self.cert.exists() && self.key.exists() {
            return Ok(());
        }
        if self.cert.exists() || self.key.exists() {
            return Err(LocalWebError::Config("incomplete CA state; restore the missing file rather than silently replacing the CA".into()));
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
        let cert_path = self.leaves.join(format!("{domain}.crt"));
        let key_path = self.leaves.join(format!("{domain}.key"));
        if cert_path.exists() && key_path.exists() {
            let bytes = fs::read(&cert_path)?;
            if let Ok((_, pem)) = x509_parser::pem::parse_x509_pem(&bytes)
                && let Ok((_, cert)) = x509_parser::parse_x509_certificate(&pem.contents)
                && cert.extensions().iter().any(|extension| {
                    matches!(
                        extension.parsed_extension(),
                        x509_parser::extensions::ParsedExtension::AuthorityKeyIdentifier(_)
                    )
                })
            {
                return Ok((cert_path, key_path));
            }
        }
        if cert_path.exists() != key_path.exists() {
            return Err(LocalWebError::Config(format!(
                "incomplete certificate state for {domain}"
            )));
        }
        self.ensure_ca()?;
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
            let _ = fs::remove_file(&key_path);
            return Err(err);
        }
        Ok((cert_path, key_path))
    }
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
        assert!(fs::read_to_string(key).unwrap().contains("PRIVATE KEY"));
        fs::remove_dir_all(base).unwrap();
    }
}

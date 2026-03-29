//! TLS support for the mhost reverse proxy.
//!
//! Provides:
//! - [`generate_self_signed_cert`] — generate a self-signed cert with `rcgen`.
//! - [`build_server_config`]       — build a `rustls::ServerConfig` from a cert/key pair.
//! - [`cache_cert_path`]           — canonical path for caching a cert on disk.
//! - [`AcmeCertManager`]           — ACME stub that falls back to self-signed certs.

use std::path::PathBuf;
use std::sync::Arc;

use rcgen::{generate_simple_self_signed, CertifiedKey};
use rustls::ServerConfig;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

use mhost_core::MhostPaths;

// ---------------------------------------------------------------------------
// Self-signed certificate generation
// ---------------------------------------------------------------------------

/// Generate a self-signed certificate covering the given hostnames.
///
/// Returns a DER-encoded certificate chain and a PKCS#8 private key,
/// both with `'static` lifetime (owned data).
pub fn generate_self_signed_cert(
    hostnames: &[&str],
) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), String> {
    let subject_alt_names: Vec<String> = hostnames.iter().map(|h| h.to_string()).collect();

    let CertifiedKey { cert, key_pair } = generate_simple_self_signed(subject_alt_names)
        .map_err(|e| format!("Failed to generate cert: {e}"))?;

    let cert_der = CertificateDer::from(cert.der().to_vec());
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key_pair.serialize_der()));

    Ok((vec![cert_der], key_der))
}

// ---------------------------------------------------------------------------
// ServerConfig builder
// ---------------------------------------------------------------------------

/// Build a `rustls` [`ServerConfig`] from a DER cert chain and a private key.
pub fn build_server_config(
    certs: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<Arc<ServerConfig>, String> {
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| format!("TLS config error: {e}"))?;
    Ok(Arc::new(config))
}

// ---------------------------------------------------------------------------
// Certificate cache path
// ---------------------------------------------------------------------------

/// Return the filesystem path where a certificate for `hostname` should be
/// cached under the mhost data directory (`~/.mhost/certs/<hostname>.pem`).
pub fn cache_cert_path(paths: &MhostPaths, hostname: &str) -> PathBuf {
    paths.root().join("certs").join(format!("{hostname}.pem"))
}

// ---------------------------------------------------------------------------
// ACME stub
// ---------------------------------------------------------------------------

/// Manages ACME (Let's Encrypt) certificate acquisition.
///
/// This is a **stub implementation** — full ACME support would require
/// integrating `instant-acme` and handling DNS/HTTP-01 challenges.
/// For now every call falls back to [`generate_self_signed_cert`].
pub struct AcmeCertManager {
    pub email: String,
    pub cache_dir: PathBuf,
}

impl AcmeCertManager {
    /// Create a new manager for the given contact `email` and cert cache
    /// directory.
    pub fn new(email: &str, cache_dir: PathBuf) -> Self {
        Self {
            email: email.to_string(),
            cache_dir,
        }
    }

    /// Return a certificate for `hostname`.
    ///
    /// **Stub**: In production this would use `instant-acme` to obtain a
    /// Let's Encrypt certificate.  Until that integration is complete the
    /// method falls back to a self-signed certificate so callers can proceed.
    pub fn get_or_create_cert(
        &self,
        hostname: &str,
    ) -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), String> {
        tracing::info!(hostname, "ACME cert requested (using self-signed fallback)");
        generate_self_signed_cert(&[hostname])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // --- generate_self_signed_cert ---

    #[test]
    fn generate_self_signed_cert_succeeds_for_localhost() {
        let result = generate_self_signed_cert(&["localhost"]);
        assert!(result.is_ok(), "expected Ok, got {result:?}");
        let (certs, _key) = result.unwrap();
        assert!(!certs.is_empty(), "cert chain must not be empty");
    }

    #[test]
    fn generate_self_signed_cert_succeeds_for_multiple_hostnames() {
        let result = generate_self_signed_cert(&["example.com", "www.example.com"]);
        assert!(result.is_ok());
        let (certs, _key) = result.unwrap();
        assert_eq!(certs.len(), 1);
    }

    // --- build_server_config ---

    #[test]
    fn build_server_config_succeeds_with_valid_cert_and_key() {
        // Install the default rustls crypto provider (ring) so that
        // ServerConfig::builder() can resolve a provider.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let (certs, key) = generate_self_signed_cert(&["localhost"]).unwrap();
        let result = build_server_config(certs, key);
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    // --- cache_cert_path ---

    #[test]
    fn cache_cert_path_returns_expected_path() {
        let paths = MhostPaths::with_root(PathBuf::from("/tmp/mhost-test"));
        let path = cache_cert_path(&paths, "example.com");
        assert_eq!(
            path,
            PathBuf::from("/tmp/mhost-test/certs/example.com.pem")
        );
    }

    // --- AcmeCertManager ---

    #[test]
    fn acme_manager_get_or_create_cert_returns_valid_cert() {
        let manager = AcmeCertManager::new(
            "admin@example.com",
            PathBuf::from("/tmp/mhost-test/certs"),
        );
        let result = manager.get_or_create_cert("example.com");
        assert!(result.is_ok(), "ACME stub must succeed: {result:?}");
        let (certs, _key) = result.unwrap();
        assert!(!certs.is_empty());
    }

    #[test]
    fn acme_manager_stores_email_and_cache_dir() {
        let cache = PathBuf::from("/tmp/certs");
        let manager = AcmeCertManager::new("admin@test.com", cache.clone());
        assert_eq!(manager.email, "admin@test.com");
        assert_eq!(manager.cache_dir, cache);
    }
}

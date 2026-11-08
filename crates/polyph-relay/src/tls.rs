//! TLS for the relay: loading PEM files, generating a self-signed
//! certificate for LAN mode, and a client verifier that pins a certificate
//! by its SHA-256 fingerprint (bound into the join PAKE transcript).

use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, ring};
use rustls::{ClientConfig, DigitallySignedStruct, ServerConfig, SignatureScheme};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use sha2::{Digest, Sha256};

fn provider() -> Arc<CryptoProvider> {
    Arc::new(ring::default_provider())
}

pub fn fingerprint(cert_der: &[u8]) -> [u8; 32] {
    Sha256::digest(cert_der).into()
}

/// A self-signed certificate for an embedded LAN relay.
#[derive(Debug, Clone)]
pub struct SelfSigned {
    pub config: Arc<ServerConfig>,
    pub fingerprint: [u8; 32],
}

pub fn self_signed(names: &[String]) -> anyhow::Result<SelfSigned> {
    let mut all = names.to_vec();
    if all.is_empty() {
        all.push("polyph.local".into());
    }
    let key = rcgen::KeyPair::generate()?;
    let cert = rcgen::CertificateParams::new(all)?.self_signed(&key)?;
    let der = cert.der().clone();
    let fp = fingerprint(&der);
    let key_der = PrivateKeyDer::try_from(key.serialize_der()).map_err(|e| anyhow::anyhow!("key encoding: {e}"))?;
    let config = ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(vec![der], key_der)?;
    Ok(SelfSigned { config: Arc::new(config), fingerprint: fp })
}

pub fn load_pem(cert_path: &std::path::Path, key_path: &std::path::Path) -> anyhow::Result<Arc<ServerConfig>> {
    use rustls_pki_types::pem::PemObject;
    let certs: Vec<CertificateDer<'static>> = CertificateDer::pem_file_iter(cert_path)?.collect::<Result<_, _>>()?;
    let key = PrivateKeyDer::from_pem_file(key_path)?;
    let config = ServerConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    Ok(Arc::new(config))
}

/// Accepts exactly one certificate, identified by fingerprint, or (when
/// `fp` is `None`) any certificate while recording its fingerprint so the
/// caller can bind it into the join PAKE. Signatures are still verified so
/// the server must hold the matching private key.
#[derive(Debug)]
struct Pinned {
    fp: Option<[u8; 32]>,
    seen: Arc<std::sync::Mutex<Option<[u8; 32]>>>,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        use subtle_eq::ct_eq;
        let got = fingerprint(end_entity);
        if let Ok(mut s) = self.seen.lock() {
            *s = Some(got);
        }
        match self.fp {
            None => Ok(ServerCertVerified::assertion()),
            Some(fp) if ct_eq(&got, &fp) => Ok(ServerCertVerified::assertion()),
            Some(_) => Err(rustls::Error::General("relay certificate does not match the pinned fingerprint".into())),
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}

mod subtle_eq {
    pub fn ct_eq(a: &[u8; 32], b: &[u8; 32]) -> bool {
        a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }
}

pub fn pinned_client(fp: [u8; 32]) -> anyhow::Result<Arc<ClientConfig>> {
    verifier_client(Some(fp), Arc::new(std::sync::Mutex::new(None)))
}

/// Accepts any certificate and records its fingerprint in `seen`. Only for
/// joining a LAN room, where the fingerprint is then bound into the PAKE
/// transcript so a substituted certificate makes the join fail.
pub fn recording_client(seen: Seen) -> anyhow::Result<Arc<ClientConfig>> {
    verifier_client(None, seen)
}

pub type Seen = Arc<std::sync::Mutex<Option<[u8; 32]>>>;

fn verifier_client(fp: Option<[u8; 32]>, seen: Seen) -> anyhow::Result<Arc<ClientConfig>> {
    let p = provider();
    let cfg = ClientConfig::builder_with_provider(p.clone())
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pinned { fp, seen, provider: p }))
        .with_no_client_auth();
    Ok(Arc::new(cfg))
}

/// Trust roots for public relays: the operating system store, certificates in
/// the file named by `SSL_CERT_FILE`, and the bundled webpki roots as a
/// fallback. The system store matters behind a TLS-inspecting proxy, whose
/// private CA is installed there and not in the webpki bundle.
pub fn public_root_store() -> rustls::RootCertStore {
    let file = std::env::var_os("SSL_CERT_FILE").filter(|v| !v.is_empty()).map(std::path::PathBuf::from);
    build_root_store(file.as_deref())
}

pub fn build_root_store(ssl_cert_file: Option<&std::path::Path>) -> rustls::RootCertStore {
    use rustls_pki_types::pem::PemObject;
    let mut roots = rustls::RootCertStore::empty();
    let native = rustls_native_certs::load_native_certs();
    for e in &native.errors {
        tracing::debug!(error = %e, "could not read part of the system trust store");
    }
    let (added, _) = roots.add_parsable_certificates(native.certs);
    tracing::debug!(added, "system trust store loaded");
    if let Some(path) = ssl_cert_file {
        match CertificateDer::pem_file_iter(path) {
            Ok(it) => {
                let certs: Vec<CertificateDer<'static>> = it.filter_map(Result::ok).collect();
                roots.add_parsable_certificates(certs);
            }
            Err(e) => tracing::warn!(file = %path.display(), error = %e, "could not read SSL_CERT_FILE"),
        }
    }
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    roots
}

pub fn public_client() -> anyhow::Result<Arc<ClientConfig>> {
    let cfg = ClientConfig::builder_with_provider(provider())
        .with_safe_default_protocol_versions()?
        .with_root_certificates(public_root_store())
        .with_no_client_auth();
    Ok(Arc::new(cfg))
}

#[cfg(test)]
mod root_tests {
    use super::*;

    const CN: &str = "polyph-test-proxy-ca";

    fn has_subject(r: &rustls::RootCertStore) -> bool {
        r.roots.iter().any(|a| a.subject.as_ref().windows(CN.len()).any(|w| w == CN.as_bytes()))
    }

    #[test]
    fn ssl_cert_file_adds_its_certificates() {
        let mut params = rcgen::CertificateParams::new(vec!["proxy-ca.test".to_string()]).unwrap();
        params.distinguished_name.push(rcgen::DnType::CommonName, CN);
        params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        let key = rcgen::KeyPair::generate().unwrap();
        let ca = params.self_signed(&key).unwrap();
        let dir = std::env::temp_dir().join(format!("polyph-roots-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("ca.pem");
        std::fs::write(&file, ca.pem()).unwrap();
        let without = build_root_store(None);
        let with = build_root_store(Some(&file));
        assert!(!without.roots.is_empty(), "webpki fallback must be present");
        assert!(!has_subject(&without));
        assert!(has_subject(&with));
        let _ = std::fs::remove_dir_all(dir);
    }
}

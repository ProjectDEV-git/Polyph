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

/// Accepts exactly one certificate, identified by fingerprint. Signatures
/// are still verified so the server must hold the matching private key.
#[derive(Debug)]
struct Pinned {
    fp: [u8; 32],
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
        if ct_eq(&fingerprint(end_entity), &self.fp) {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General("relay certificate does not match the pinned fingerprint".into()))
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
    let p = provider();
    let cfg = ClientConfig::builder_with_provider(p.clone())
        .with_safe_default_protocol_versions()?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pinned { fp, provider: p }))
        .with_no_client_auth();
    Ok(Arc::new(cfg))
}

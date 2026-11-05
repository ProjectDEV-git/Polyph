//! Device identity and the local storage key.
//!
//! One 32-byte storage key protects everything the daemon writes (identity,
//! rooms). It lives in the OS keychain (macOS Keychain, Windows Credential
//! Manager, Secret Service on Linux). Where no keychain service is reachable
//! (headless or minimal Linux, or `POLYPH_KEYSTORE=file`), it is kept in
//! `storage.key` with owner-only permissions, like an SSH private key.

use std::sync::Arc;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use polyph_proto::identity::DeviceIdentity;
use tracing::{info, warn};
use zeroize::Zeroizing;

use crate::paths::{Paths, write_private};

const SERVICE: &str = "dev.polyph";

#[derive(Clone)]
pub struct StorageKey(Arc<Zeroizing<[u8; 32]>>);

impl std::fmt::Debug for StorageKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StorageKey(..)")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyStore {
    Keychain,
    File,
}

impl StorageKey {
    pub fn seal(&self, aad: &[u8], data: &[u8]) -> anyhow::Result<Vec<u8>> {
        let c = XChaCha20Poly1305::new_from_slice(&self.0[..]).map_err(|_| anyhow::anyhow!("storage key"))?;
        let mut nonce = [0u8; 24];
        getrandom::getrandom(&mut nonce).map_err(|_| anyhow::anyhow!("no randomness"))?;
        let ct = c
            .encrypt(XNonce::from_slice(&nonce), Payload { msg: data, aad })
            .map_err(|_| anyhow::anyhow!("encryption failed"))?;
        let mut out = b"PYS1".to_vec();
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);
        Ok(out)
    }

    pub fn open(&self, aad: &[u8], data: &[u8]) -> anyhow::Result<Zeroizing<Vec<u8>>> {
        if data.len() < 28 || &data[..4] != b"PYS1" {
            anyhow::bail!("not a Polyph storage file");
        }
        let c = XChaCha20Poly1305::new_from_slice(&self.0[..]).map_err(|_| anyhow::anyhow!("storage key"))?;
        let pt = c
            .decrypt(XNonce::from_slice(&data[4..28]), Payload { msg: &data[28..], aad })
            .map_err(|_| anyhow::anyhow!("could not decrypt local data; the storage key does not match"))?;
        Ok(Zeroizing::new(pt))
    }
}

fn account_for(paths: &Paths) -> String {
    // One keychain entry per Polyph home, so separate homes never collide.
    let h = blake3::hash(paths.home.to_string_lossy().as_bytes());
    format!("storage-{}", &h.to_hex()[..16])
}

fn random_key() -> anyhow::Result<Zeroizing<[u8; 32]>> {
    let mut k = Zeroizing::new([0u8; 32]);
    getrandom::getrandom(k.as_mut()).map_err(|_| anyhow::anyhow!("no randomness"))?;
    Ok(k)
}

fn from_file(paths: &Paths) -> anyhow::Result<StorageKey> {
    let p = paths.storage_key();
    if let Ok(b) = std::fs::read(&p) {
        let k: [u8; 32] = b.as_slice().try_into().map_err(|_| anyhow::anyhow!("{} is corrupt", p.display()))?;
        return Ok(StorageKey(Arc::new(Zeroizing::new(k))));
    }
    let k = random_key()?;
    write_private(&p, k.as_ref())?;
    Ok(StorageKey(Arc::new(k)))
}

fn from_keychain(paths: &Paths) -> anyhow::Result<StorageKey> {
    let entry = keyring::Entry::new(SERVICE, &account_for(paths))?;
    match entry.get_secret() {
        Ok(b) => {
            let k: [u8; 32] = b.as_slice().try_into().map_err(|_| anyhow::anyhow!("keychain entry is corrupt"))?;
            Ok(StorageKey(Arc::new(Zeroizing::new(k))))
        }
        Err(keyring::Error::NoEntry) => {
            let k = random_key()?;
            entry.set_secret(k.as_ref())?;
            // Read back: some backends accept writes they cannot persist.
            let back = entry.get_secret()?;
            if back.as_slice() != k.as_ref() {
                anyhow::bail!("keychain did not keep the secret");
            }
            Ok(StorageKey(Arc::new(k)))
        }
        Err(e) => Err(e.into()),
    }
}

/// Loads the storage key, preferring the keychain. If a `storage.key` file
/// exists it is used (an earlier run had no keychain), so data stays readable.
pub fn storage_key(paths: &Paths, force_file: bool) -> anyhow::Result<(StorageKey, KeyStore)> {
    let forced_file = force_file || std::env::var("POLYPH_KEYSTORE").is_ok_and(|v| v == "file");
    if forced_file || paths.storage_key().exists() {
        return Ok((from_file(paths)?, KeyStore::File));
    }
    // Keychain backends block on their own runtime (secret-service over
    // D-Bus), which panics inside an async runtime; ask from a plain thread.
    let p2 = paths.clone();
    let got = std::thread::spawn(move || from_keychain(&p2))
        .join()
        .unwrap_or_else(|_| Err(anyhow::anyhow!("keychain lookup panicked")));
    match got {
        Ok(k) => Ok((k, KeyStore::Keychain)),
        Err(e) => {
            warn!(error = %e, "no keychain available; storing the local key in a private file");
            Ok((from_file(paths)?, KeyStore::File))
        }
    }
}

/// Removes this home's storage key from the keychain, if it is there
/// (`polyph uninstall --purge`). The data it protected is gone with it.
pub fn forget_keychain(paths: &Paths) {
    let p = paths.clone();
    // A plain thread, as in `storage_key`: some backends need their own runtime.
    let _ = std::thread::spawn(move || {
        if let Ok(e) = keyring::Entry::new(SERVICE, &account_for(&p)) {
            let _ = e.delete_credential();
        }
    })
    .join();
}

/// Loads or creates this device's identity, encrypted with the storage key.
pub fn identity(paths: &Paths, key: &StorageKey) -> anyhow::Result<Arc<DeviceIdentity>> {
    let p = paths.identity();
    if let Ok(b) = std::fs::read(&p) {
        let pt = key.open(b"identity", &b)?;
        let arr: [u8; 64] = pt.as_slice().try_into().map_err(|_| anyhow::anyhow!("identity file is corrupt"))?;
        return Ok(Arc::new(DeviceIdentity::from_secret_bytes(&arr)));
    }
    let id = DeviceIdentity::generate()?;
    write_private(&p, &key.seal(b"identity", id.to_secret_bytes().as_ref())?)?;
    info!(device = %id.device_id().short(), "created device identity");
    Ok(Arc::new(id))
}

/// The local API token, created on first use with owner-only permissions.
pub fn api_token(paths: &Paths) -> anyhow::Result<String> {
    if let Ok(t) = std::fs::read_to_string(paths.token()) {
        let t = t.trim().to_string();
        if t.len() >= 32 {
            return Ok(t);
        }
    }
    let k = random_key()?;
    let t = hex::encode(k.as_ref());
    write_private(&paths.token(), t.as_bytes())?;
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_store_round_trip_and_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::at(dir.path());
        paths.ensure().unwrap();
        let k = from_file(&paths).unwrap();
        let id = identity(&paths, &k).unwrap();
        let id2 = identity(&paths, &from_file(&paths).unwrap()).unwrap();
        assert_eq!(id.device_id(), id2.device_id());
        let t = api_token(&paths).unwrap();
        assert_eq!(t, api_token(&paths).unwrap());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for f in [paths.token(), paths.storage_key(), paths.identity()] {
                assert_eq!(std::fs::metadata(&f).unwrap().permissions().mode() & 0o777, 0o600, "{}", f.display());
            }
        }
        let sealed = k.seal(b"a", b"secret").unwrap();
        assert!(k.open(b"b", &sealed).is_err());
        assert_eq!(k.open(b"a", &sealed).unwrap().as_slice(), b"secret");
    }
}

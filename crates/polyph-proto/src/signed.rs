//! `Signed<T>`: a CBOR body plus the signer and a signature over the exact
//! body bytes. Verifiers check the signature before decoding, so field
//! ordering or encoder differences can never invalidate a signature.

use std::marker::PhantomData;

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::codec::{from_cbor, to_cbor};
use crate::error::Result;
use crate::identity::{DeviceIdentity, verify};
use crate::ids::{DeviceId, Sig};

#[derive(Serialize, Deserialize)]
#[serde(bound = "")]
pub struct Signed<T> {
    #[serde(with = "crate::bytes::vec")]
    pub body: Vec<u8>,
    pub signer: DeviceId,
    pub sig: Sig,
    #[serde(skip)]
    _t: PhantomData<fn() -> T>,
}

impl<T> Clone for Signed<T> {
    fn clone(&self) -> Self {
        Self { body: self.body.clone(), signer: self.signer, sig: self.sig, _t: PhantomData }
    }
}

impl<T> PartialEq for Signed<T> {
    fn eq(&self, o: &Self) -> bool {
        self.body == o.body && self.signer == o.signer && self.sig == o.sig
    }
}
impl<T> Eq for Signed<T> {}

impl<T> std::fmt::Debug for Signed<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Signed<{}>(by {}, {} bytes)", std::any::type_name::<T>(), self.signer.short(), self.body.len())
    }
}

/// Types that can be signed name their own signing context.
pub trait SignContext {
    const CONTEXT: &'static str;
}

impl<T: Serialize + DeserializeOwned + SignContext> Signed<T> {
    pub fn sign(identity: &DeviceIdentity, value: &T) -> Result<Self> {
        let body = to_cbor(value)?;
        let sig = identity.sign(T::CONTEXT, &body);
        Ok(Self { body, signer: identity.device_id(), sig, _t: PhantomData })
    }

    /// Verifies the signature against the embedded signer and decodes the body.
    /// Callers must still check that the signer is someone they trust.
    pub fn open(&self) -> Result<T> {
        verify(&self.signer, T::CONTEXT, &self.body, &self.sig)?;
        from_cbor(&self.body)
    }

    /// Verifies against an expected signer.
    pub fn open_from(&self, expected: &DeviceId) -> Result<T> {
        if &self.signer != expected {
            return Err(crate::error::ProtoError::BadSignature);
        }
        self.open()
    }
}

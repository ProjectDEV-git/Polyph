//! wasm-bindgen exports for browsers and the phone PWA. Structured values
//! cross the boundary as JSON strings (ids as hex, bytes as base64) and are
//! converted to the CBOR wire format here, so there is one implementation of
//! every encoding and every cryptographic operation.

use wasm_bindgen::prelude::*;

use crate::cert::CertChain;
use crate::codec::{from_cbor, to_cbor};
use crate::delivery::{EpochKeyDelivery, open_epoch_key};
use crate::envelope::Envelope;
use crate::group::{EpochKey, Keyring};
use crate::identity::DeviceIdentity;
use crate::ids::RoomId;
use crate::join::{JoinReply, JoinRequest};
use crate::joincode::JoinCode;
use crate::messages::RoomMessage;
use crate::pake::{PakeFinished, PakeRole, PakeStart, SecureChannel};
use crate::relay_wire::{ClientFrame, RelayFrame, auth_message};
use crate::signed::Signed;

fn err<E: std::fmt::Display>(e: E) -> JsError {
    JsError::new(&e.to_string())
}

#[wasm_bindgen]
#[derive(Debug)]
pub struct WasmIdentity(DeviceIdentity);

#[wasm_bindgen]
impl WasmIdentity {
    pub fn generate() -> Result<WasmIdentity, JsError> {
        DeviceIdentity::generate().map(WasmIdentity).map_err(err)
    }

    #[wasm_bindgen(js_name = fromSecret)]
    pub fn from_secret(bytes: &[u8]) -> Result<WasmIdentity, JsError> {
        let arr: [u8; 64] = bytes.try_into().map_err(|_| JsError::new("secret must be 64 bytes"))?;
        Ok(WasmIdentity(DeviceIdentity::from_secret_bytes(&arr)))
    }

    pub fn secret(&self) -> Vec<u8> {
        self.0.to_secret_bytes().to_vec()
    }

    #[wasm_bindgen(js_name = deviceId)]
    pub fn device_id(&self) -> String {
        self.0.device_id().to_hex()
    }

    #[wasm_bindgen(js_name = publicJson)]
    pub fn public_json(&self) -> Result<String, JsError> {
        serde_json::to_string(&self.0.public()).map_err(err)
    }

    pub fn sign(&self, context: &str, msg: &[u8]) -> Vec<u8> {
        self.0.sign(context, msg).0.to_vec()
    }

    /// Signature for the relay's challenge.
    #[wasm_bindgen(js_name = signRelayChallenge)]
    pub fn sign_relay_challenge(&self, nonce: &[u8]) -> Result<Vec<u8>, JsError> {
        let n: [u8; 32] = nonce.try_into().map_err(|_| JsError::new("nonce must be 32 bytes"))?;
        Ok(self.0.sign(crate::identity::ctx::RELAY_AUTH, &auth_message(&n, &self.0.device_id())).0.to_vec())
    }
}

#[wasm_bindgen(js_name = normalizeJoinCode)]
pub fn normalize_join_code(code: &str) -> Result<String, JsError> {
    JoinCode::parse(code).map(|c| c.to_string()).map_err(err)
}

#[wasm_bindgen(js_name = generateJoinCode)]
pub fn generate_join_code() -> Result<String, JsError> {
    JoinCode::generate().map(|c| c.to_string()).map_err(err)
}

#[wasm_bindgen(js_name = rendezvousId)]
pub fn rendezvous_id(code: &str) -> Result<String, JsError> {
    JoinCode::parse(code).map(|c| c.rendezvous().to_hex()).map_err(err)
}

#[wasm_bindgen]
#[derive(Debug)]
pub struct WasmPake {
    start: Option<PakeStart>,
    finished: Option<PakeFinished>,
}

#[wasm_bindgen]
impl WasmPake {
    #[wasm_bindgen(constructor)]
    pub fn new(code: &str, host: bool, binding: &[u8]) -> Result<WasmPake, JsError> {
        let c = JoinCode::parse(code).map_err(err)?;
        let role = if host { PakeRole::Host } else { PakeRole::Joiner };
        Ok(WasmPake { start: Some(PakeStart::new(&c, role, binding)), finished: None })
    }

    pub fn message(&self) -> Vec<u8> {
        self.start.as_ref().map(|s| s.message().to_vec()).unwrap_or_default()
    }

    /// Consumes the peer's PAKE message and returns our confirmation tag.
    pub fn finish(&mut self, peer: &[u8]) -> Result<Vec<u8>, JsError> {
        let s = self.start.take().ok_or_else(|| JsError::new("PAKE already finished"))?;
        let f = s.finish(peer).map_err(err)?;
        let tag = f.confirmation().to_vec();
        self.finished = Some(f);
        Ok(tag)
    }

    #[wasm_bindgen(js_name = verifyPeer)]
    pub fn verify_peer(&mut self, tag: &[u8]) -> Result<WasmChannel, JsError> {
        let f = self.finished.take().ok_or_else(|| JsError::new("PAKE not finished"))?;
        f.verify_peer(tag).map(WasmChannel).map_err(err)
    }
}

#[wasm_bindgen]
#[derive(Debug)]
pub struct WasmChannel(SecureChannel);

#[wasm_bindgen]
impl WasmChannel {
    pub fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, JsError> {
        self.0.seal(plaintext).map_err(err)
    }
    pub fn open(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>, JsError> {
        self.0.open(ciphertext).map_err(err)
    }
}

#[wasm_bindgen]
#[derive(Debug, Default)]
pub struct WasmKeyring(Keyring);

#[wasm_bindgen]
impl WasmKeyring {
    #[wasm_bindgen(constructor)]
    pub fn new() -> WasmKeyring {
        WasmKeyring::default()
    }

    /// Opens a signed epoch-key delivery (JSON) addressed to `me`; returns the epoch.
    #[wasm_bindgen(js_name = insertDelivery)]
    pub fn insert_delivery(&mut self, me: &WasmIdentity, delivery_json: &str) -> Result<u64, JsError> {
        let d: Signed<EpochKeyDelivery> = serde_json::from_str(delivery_json).map_err(err)?;
        let k = open_epoch_key(&me.0, &d).map_err(err)?;
        let e = k.epoch;
        self.0.insert(k);
        Ok(e)
    }

    /// Inserts a raw key, used by spectator links that carry a view key.
    #[wasm_bindgen(js_name = insertRaw)]
    pub fn insert_raw(&mut self, room_hex: &str, epoch: u64, key: &[u8]) -> Result<(), JsError> {
        let room = RoomId::from_hex(room_hex).map_err(err)?;
        let k: [u8; 32] = key.try_into().map_err(|_| JsError::new("key must be 32 bytes"))?;
        self.0.insert(EpochKey::from_bytes(room, epoch, k));
        Ok(())
    }

    #[wasm_bindgen(js_name = currentEpoch)]
    pub fn current_epoch(&self) -> Option<u64> {
        self.0.current
    }

    /// Encrypts a room message (JSON) into envelope bytes.
    pub fn seal(&self, me: &WasmIdentity, seq: u64, message_json: &str) -> Result<Vec<u8>, JsError> {
        let msg: RoomMessage = serde_json::from_str(message_json).map_err(err)?;
        let key = self.0.current_key().ok_or_else(|| JsError::new("no room key yet"))?;
        let env = key.seal(&me.0, seq, &msg.encode().map_err(err)?).map_err(err)?;
        Ok(env.encode())
    }

    /// Decrypts envelope bytes. Returns `{"sender": hex, "epoch": n, "message": {...}}`.
    pub fn open(&mut self, envelope: &[u8]) -> Result<String, JsError> {
        let env = Envelope::decode(envelope).map_err(err)?;
        let pt = self.0.open(&env).map_err(err)?;
        let msg = RoomMessage::decode(&pt).map_err(err)?;
        serde_json::to_string(&serde_json::json!({
            "sender": env.sender.to_hex(),
            "epoch": env.epoch,
            "seq": env.seq,
            "message": msg,
        }))
        .map_err(err)
    }
}

#[wasm_bindgen(js_name = encodeClientFrame)]
pub fn encode_client_frame(json: &str) -> Result<Vec<u8>, JsError> {
    let f: ClientFrame = serde_json::from_str(json).map_err(err)?;
    f.encode().map_err(err)
}

#[wasm_bindgen(js_name = decodeRelayFrame)]
pub fn decode_relay_frame(bytes: &[u8]) -> Result<String, JsError> {
    let f = RelayFrame::decode(bytes).map_err(err)?;
    serde_json::to_string(&f).map_err(err)
}

#[wasm_bindgen(js_name = encodeJoinRequest)]
pub fn encode_join_request(json: &str) -> Result<Vec<u8>, JsError> {
    let r: JoinRequest = serde_json::from_str(json).map_err(err)?;
    to_cbor(&r).map_err(err)
}

#[wasm_bindgen(js_name = decodeJoinReply)]
pub fn decode_join_reply(bytes: &[u8]) -> Result<String, JsError> {
    let r: JoinReply = from_cbor(bytes).map_err(err)?;
    serde_json::to_string(&r).map_err(err)
}

/// Verifies a certificate chain (JSON) and returns the verified member as JSON.
#[wasm_bindgen(js_name = verifyChain)]
pub fn verify_chain(chain_json: &str, now: i64) -> Result<String, JsError> {
    let c: CertChain = serde_json::from_str(chain_json).map_err(err)?;
    let v = c.verify(now, &|_| false).map_err(err)?;
    serde_json::to_string(&serde_json::json!({
        "room": v.room.to_hex(),
        "member": v.member.to_hex(),
        "device": v.device.sign.to_hex(),
        "role": v.role.as_str(),
    }))
    .map_err(err)
}

#[wasm_bindgen(js_name = protocolVersion)]
pub fn protocol_version() -> u16 {
    crate::version::PROTOCOL_VERSION
}

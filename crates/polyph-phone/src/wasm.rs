//! wasm-bindgen exports for the phone PWA (`apps/web/src/phone`). Structured
//! values cross as JSON strings and frames as byte arrays, like
//! `polyph-proto`'s own exports.

use std::sync::Arc;

use polyph_proto::identity::DeviceIdentity;
use polyph_proto::ids::MemberId;
use wasm_bindgen::prelude::*;

use crate::{Output, Phone, PhoneError};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(s: &str);
}

/// Sends Rust panics to the console (the release build aborts after it).
fn hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| std::panic::set_hook(Box::new(|info| console_error(&format!("polyph engine: {info}")))));
}

fn err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

fn identity(secret: &[u8]) -> Result<Arc<DeviceIdentity>, JsError> {
    hook();
    let arr: [u8; 64] = secret.try_into().map_err(|_| JsError::new("secret must be 64 bytes"))?;
    Ok(Arc::new(DeviceIdentity::from_secret_bytes(&arr)))
}

/// A new device secret (64 bytes) for a phone that has none yet.
#[wasm_bindgen(js_name = generateDeviceSecret)]
pub fn generate_device_secret() -> Result<Vec<u8>, JsError> {
    Ok(DeviceIdentity::generate().map_err(err)?.to_secret_bytes().to_vec())
}

/// What a call produced. Frames go to the WebSocket in order; `events` is a
/// JSON array of `{ room, event }`.
#[wasm_bindgen]
#[derive(Debug, Default)]
pub struct PhoneOutput {
    frames: Vec<Vec<u8>>,
    events: String,
    dirty: bool,
    result: Option<String>,
    error: Option<String>,
    code: i32,
}

#[wasm_bindgen]
impl PhoneOutput {
    #[wasm_bindgen(getter, js_name = frameCount)]
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
    pub fn frame(&self, i: usize) -> Vec<u8> {
        self.frames.get(i).cloned().unwrap_or_default()
    }
    #[wasm_bindgen(getter)]
    pub fn events(&self) -> String {
        self.events.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn dirty(&self) -> bool {
        self.dirty
    }
    /// The call's result as JSON, when it succeeded.
    #[wasm_bindgen(getter)]
    pub fn result(&self) -> Option<String> {
        self.result.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn error(&self) -> Option<String> {
        self.error.clone()
    }
    #[wasm_bindgen(getter)]
    pub fn code(&self) -> i32 {
        self.code
    }
}

impl From<Output> for PhoneOutput {
    fn from(o: Output) -> Self {
        Self {
            frames: o.frames,
            events: serde_json::to_string(&o.events).unwrap_or_else(|_| "[]".into()),
            dirty: o.dirty,
            ..Default::default()
        }
    }
}

#[wasm_bindgen]
#[derive(Debug)]
pub struct PhoneEngine(Phone);

#[wasm_bindgen]
impl PhoneEngine {
    /// A phone with no room yet. `member` (hex) pairs it with the person's
    /// other devices; without it the phone joins as a new member.
    #[wasm_bindgen(constructor)]
    pub fn new(secret: &[u8], member: Option<String>, name: &str, relay: &str) -> Result<PhoneEngine, JsError> {
        let member = match member.filter(|m| !m.is_empty()) {
            Some(m) => Some(MemberId::from_hex(&m).map_err(err)?),
            None => None,
        };
        Ok(Self(Phone::new(identity(secret)?, member, name, relay).map_err(err)?))
    }

    pub fn restore(secret: &[u8], saved: &[u8], now: f64) -> Result<PhoneEngine, JsError> {
        Ok(Self(Phone::restore(identity(secret)?, saved, now as i64).map_err(err)?))
    }

    pub fn save(&mut self) -> Result<Vec<u8>, JsError> {
        self.0.save().map_err(err)
    }

    #[wasm_bindgen(getter)]
    pub fn relay(&self) -> String {
        self.0.relay.clone()
    }

    #[wasm_bindgen(getter, js_name = hasRoom)]
    pub fn has_room(&self) -> bool {
        self.0.room.is_some()
    }

    pub fn connected(&mut self) -> PhoneOutput {
        self.0.connected().into()
    }

    pub fn disconnected(&mut self) -> PhoneOutput {
        self.0.disconnected().into()
    }

    pub fn frame(&mut self, bytes: &[u8], now: f64) -> PhoneOutput {
        self.0.frame(bytes, now as i64).into()
    }

    pub fn tick(&mut self, now: f64) -> PhoneOutput {
        self.0.tick(now as i64).into()
    }

    pub fn join(&mut self, code: &str, name: Option<String>) -> PhoneOutput {
        match self.0.start_join(code, name.as_deref()) {
            Ok(o) => o.into(),
            Err(e) => failed(e),
        }
    }

    /// One API call (`params` and the result are JSON).
    pub fn call(&mut self, method: &str, params: &str, now: f64) -> PhoneOutput {
        let params: serde_json::Value = match serde_json::from_str(params) {
            Ok(v) => v,
            Err(e) => return failed(PhoneError::Bad(e.to_string())),
        };
        match self.0.call(method, &params, now as i64) {
            Ok((v, o)) => {
                let mut out: PhoneOutput = o.into();
                out.result = Some(v.to_string());
                out
            }
            Err(e) => failed(e),
        }
    }
}

fn failed(e: PhoneError) -> PhoneOutput {
    PhoneOutput { code: e.code() as i32, error: Some(e.to_string()), events: "[]".into(), ..Default::default() }
}

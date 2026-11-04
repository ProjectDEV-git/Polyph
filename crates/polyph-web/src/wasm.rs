//! JavaScript bindings. The page owns the WebSocket, storage and the clock;
//! see `packages/ui/src/member/` for the other half.

use polyph_proto::identity::DeviceIdentity;
use serde_json::Value;
use wasm_bindgen::prelude::*;

use crate::member::Member;

/// A new device identity, as its 64 secret bytes (kept in browser storage).
#[wasm_bindgen(js_name = newIdentity)]
pub fn new_identity() -> Result<Vec<u8>, JsError> {
    Ok(DeviceIdentity::generate().map_err(|e| JsError::new(&e.to_string()))?.to_secret_bytes().to_vec())
}

fn identity(secret: &[u8]) -> Result<DeviceIdentity, JsError> {
    let b: [u8; 64] = secret.try_into().map_err(|_| JsError::new("identity must be 64 bytes"))?;
    Ok(DeviceIdentity::from_secret_bytes(&b))
}

#[wasm_bindgen]
#[derive(Debug)]
pub struct WebMember(Member);

#[wasm_bindgen]
impl WebMember {
    #[wasm_bindgen(constructor)]
    pub fn new(secret: &[u8], name: &str, relay: &str) -> Result<WebMember, JsError> {
        Ok(WebMember(Member::new(identity(secret)?, name, relay)))
    }

    /// Restores a room saved with `save()`.
    pub fn load(secret: &[u8], name: &str, saved: &[u8], now: f64) -> Result<WebMember, JsError> {
        Member::load(identity(secret)?, name, saved, now as i64).map(WebMember).map_err(|e| JsError::new(&e))
    }

    pub fn save(&mut self) -> Option<Vec<u8>> {
        self.0.save()
    }

    #[wasm_bindgen(getter)]
    pub fn relay(&self) -> String {
        self.0.relay().to_string()
    }

    #[wasm_bindgen(js_name = hasRoom)]
    pub fn has_room(&self) -> bool {
        self.0.room.is_some()
    }

    pub fn connected(&mut self) {
        self.0.connected()
    }

    pub fn disconnected(&mut self) {
        self.0.disconnected()
    }

    #[wasm_bindgen(js_name = onFrame)]
    pub fn on_frame(&mut self, bytes: &[u8], now: f64) {
        self.0.on_frame(bytes, now as i64)
    }

    pub fn tick(&mut self, now: f64) {
        self.0.tick(now as i64)
    }

    /// Calls an API method; params and result are JSON text.
    pub fn call(&mut self, method: &str, params: &str, now: f64) -> Result<String, JsError> {
        let p: Value = serde_json::from_str(params).unwrap_or(Value::Null);
        let v = self.0.call(method, &p, now as i64).map_err(|e| JsError::new(&e))?;
        Ok(v.to_string())
    }

    /// Frames to send (array of Uint8Array).
    #[wasm_bindgen(js_name = takeFrames)]
    pub fn take_frames(&mut self) -> js_sys::Array {
        let out = self.0.take_frames();
        out.iter().map(|f| js_sys::Uint8Array::from(f.as_slice())).collect()
    }

    /// Events for the app, as JSON text (an array of `{room, event}`).
    #[wasm_bindgen(js_name = takeEvents)]
    pub fn take_events(&mut self) -> String {
        Value::Array(self.0.take_events()).to_string()
    }

    /// True when the room changed since the last call and should be saved.
    #[wasm_bindgen(js_name = takeDirty)]
    pub fn take_dirty(&mut self) -> bool {
        self.0.take_dirty()
    }
}

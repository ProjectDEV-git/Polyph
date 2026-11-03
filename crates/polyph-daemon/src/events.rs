//! Events the daemon streams to local clients.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ApiEvent {
    /// Room id (hex), when the event belongs to a room.
    pub room: Option<String>,
    pub event: serde_json::Value,
}

impl ApiEvent {
    pub fn room<T: Serialize>(room: &polyph_proto::ids::RoomId, ev: &T) -> Self {
        Self { room: Some(room.to_hex()), event: serde_json::to_value(ev).unwrap_or(serde_json::Value::Null) }
    }

    pub fn global(kind: &str, data: serde_json::Value) -> Self {
        let mut v = serde_json::json!({ "type": kind });
        if let (Some(o), serde_json::Value::Object(d)) = (v.as_object_mut(), data) {
            o.extend(d);
        }
        Self { room: None, event: v }
    }

    pub fn room_kind(room: &polyph_proto::ids::RoomId, kind: &str, data: serde_json::Value) -> Self {
        let mut e = Self::global(kind, data);
        e.room = Some(room.to_hex());
        e
    }

    pub fn kind(&self) -> &str {
        self.event.get("type").and_then(|t| t.as_str()).unwrap_or("")
    }
}

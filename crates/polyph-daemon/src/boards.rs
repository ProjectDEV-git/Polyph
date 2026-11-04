//! Whiteboards: creating, finding and drawing on them (people through the
//! API, agents through their tools).

use polyph_core::Effect;
use polyph_core::docs;
use polyph_core::whiteboard::{self, INDEX_PREFIX, WhiteboardInfo};
use polyph_proto::ids::ObjectId;
use polyph_proto::messages::DocName;
use serde_json::json;

use crate::daemon::ApiError;
use crate::runtime::RoomCtx;

pub fn create(ctx: &mut RoomCtx, name: &str, now: i64) -> Result<WhiteboardInfo, ApiError> {
    let name = name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(ApiError::BadRequest("a whiteboard name must be 1 to 120 bytes".into()));
    }
    Ok(WhiteboardInfo {
        id: ObjectId::random().map_err(|e| ApiError::Failed(e.to_string()))?.to_hex(),
        name: polyph_core::redact::redact(name).into_owned(),
        created_by: ctx.core.my_member.to_hex(),
        created_at: now,
    })
}

/// Index entry plus the board's own name.
pub fn create_effects(ctx: &mut RoomCtx, info: &WhiteboardInfo) -> Result<Vec<Effect>, ApiError> {
    let mut fx = ctx.core.change_doc(DocName::Settings, |d| {
        docs::set_setting(d, &format!("{INDEX_PREFIX}{}", info.id), &json!(info));
    })?;
    fx.extend(ctx.core.change_doc(DocName::Whiteboard(info.id.clone()), |d| whiteboard::set_name(d, &info.name))?);
    Ok(fx)
}

/// A whiteboard by id prefix or exact name (case-insensitive).
pub fn find(ctx: &mut RoomCtx, q: &str) -> Result<WhiteboardInfo, ApiError> {
    let ql = q.trim().to_lowercase();
    let all = ctx.core.whiteboards();
    let hits: Vec<WhiteboardInfo> =
        all.into_iter().filter(|w| (!ql.is_empty() && w.id.starts_with(&ql)) || w.name.to_lowercase() == ql).collect();
    match hits.len() {
        1 => Ok(hits.into_iter().next().ok_or_else(|| ApiError::NotFound("no such whiteboard".into()))?),
        0 => Err(ApiError::NotFound(format!("no whiteboard \"{q}\""))),
        _ => Err(ApiError::BadRequest(format!("\"{q}\" matches several whiteboards; use its id"))),
    }
}

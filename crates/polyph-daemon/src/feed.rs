//! Writes daemon-side events into the room's activity feed (board changes
//! are logged by the room engine itself; see `polyph_core::activity`).

use polyph_proto::crdt::{Activity, ActivityEntry};
use polyph_proto::ids::ObjectId;
use polyph_proto::messages::DocName;
use serde_json::Value;

use crate::daemon::Shared;
use crate::runtime::{RoomCtx, RoomRuntime, apply};

/// Appends one entry while the room is locked.
pub async fn log_locked(
    rt: &RoomRuntime,
    ctx: &mut RoomCtx,
    shared: &Shared,
    kind: &str,
    agent: Option<&str>,
    args: Value,
) {
    let e = ActivityEntry {
        id: ObjectId::random().map(|o| o.to_hex()).unwrap_or_default(),
        at: shared.clock.now(),
        kind: kind.to_string(),
        actor: ctx.core.my_member.to_hex(),
        agent: agent.map(str::to_string),
        summary_key: format!("activity-{}", kind.replace('_', "-")),
        args,
        checkpoint: None,
    };
    if let Ok(fx) = ctx.core.change_doc(DocName::Activity, |d| {
        let _ = Activity::new(d).append(&e);
    }) {
        apply(rt, ctx, shared, fx).await;
    }
}

pub async fn log(rt: &RoomRuntime, shared: &Shared, kind: &str, agent: Option<&str>, args: Value) {
    let mut ctx = rt.ctx.lock().await;
    log_locked(rt, &mut ctx, shared, kind, agent, args).await;
}

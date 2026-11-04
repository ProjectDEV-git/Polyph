//! `org.polyph.Status1` on the D-Bus session bus (Linux): the status feed for
//! panels and widgets (Waybar, GNOME and KDE extensions) without the local
//! API token.
//!
//! Read-only on purpose: the session bus is reachable by every process of
//! the user, sandboxed agents included, so nothing here can approve, deny or
//! change anything. Approvals go through the token-protected local API.
//!
//! Object `/org/polyph/Status`:
//! - method `Snapshot() -> s` (status snapshot JSON);
//! - properties `RoomName` (s), `Connected` (b), `AgentsRunning` (u),
//!   `ApprovalsPending` (u), with change signals;
//! - signal `Event(s)`: each status feed line.

use std::sync::{Arc, Weak};

use polyph_proto::messages::AgentState;
use zbus::object_server::SignalContext;

use crate::daemon::Daemon;
use crate::status::StatusFeed;

pub const NAME: &str = "org.polyph.Status1";
pub const PATH: &str = "/org/polyph/Status";

struct Status {
    d: Weak<Daemon>,
}

impl Status {
    async fn snap(&self) -> Option<polyph_proto::status::StatusSnapshot> {
        Some(self.d.upgrade()?.status().await)
    }
}

#[zbus::interface(name = "org.polyph.Status1")]
impl Status {
    async fn snapshot(&self) -> String {
        self.snap().await.map(|s| serde_json::to_string(&s).unwrap_or_default()).unwrap_or_default()
    }

    #[zbus(property)]
    async fn room_name(&self) -> String {
        self.snap().await.and_then(|s| s.room_name).unwrap_or_default()
    }

    #[zbus(property)]
    async fn connected(&self) -> bool {
        self.snap().await.is_some_and(|s| s.connected)
    }

    #[zbus(property)]
    async fn agents_running(&self) -> u32 {
        self.snap()
            .await
            .map(|s| {
                s.agents.iter().filter(|a| matches!(a.state, AgentState::Running | AgentState::WaitingApproval)).count()
                    as u32
            })
            .unwrap_or(0)
    }

    #[zbus(property)]
    async fn approvals_pending(&self) -> u32 {
        self.snap().await.map(|s| s.approvals.len() as u32).unwrap_or(0)
    }

    #[zbus(signal)]
    async fn event(ctxt: &SignalContext<'_>, line: &str) -> zbus::Result<()>;
}

/// Serves the status object until the daemon goes away. `address` is a bus
/// address; None uses the session bus.
pub async fn serve(d: &Arc<Daemon>, address: Option<String>) -> zbus::Result<()> {
    let builder = match &address {
        Some(a) => zbus::connection::Builder::address(a.as_str())?,
        None => zbus::connection::Builder::session()?,
    };
    let conn = builder.name(NAME)?.serve_at(PATH, Status { d: Arc::downgrade(d) })?.build().await?;
    let mut rx = d.subscribe();
    let weak = Arc::downgrade(d);
    let d0_now = d.shared.clock.now();
    tokio::spawn(async move {
        let mut feed = StatusFeed::new(d0_now);
        loop {
            let ev = match rx.recv().await {
                Ok(e) => e,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return,
            };
            let Some(d) = weak.upgrade() else { return };
            let lines = feed.translate(&ev, &d).await;
            if lines.is_empty() {
                continue;
            }
            let Ok(iface) = conn.object_server().interface::<_, Status>(PATH).await else { return };
            let ctxt = iface.signal_context();
            for l in &lines {
                let _ = Status::event(ctxt, &l.to_json_line()).await;
            }
            let s = iface.get().await;
            let _ = s.connected_changed(ctxt).await;
            let _ = s.agents_running_changed(ctxt).await;
            let _ = s.approvals_pending_changed(ctxt).await;
            let _ = s.room_name_changed(ctxt).await;
        }
    });
    Ok(())
}

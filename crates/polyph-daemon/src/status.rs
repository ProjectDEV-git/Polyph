//! Turns daemon events into the versioned status feed (docs/status-feed.md).

use polyph_proto::status::{StatusEvent, StatusEventKind, StatusSnapshot};

use crate::daemon::Daemon;
use crate::events::ApiEvent;

#[derive(Debug, Default)]
pub struct StatusFeed {
    seq: u64,
}

impl StatusFeed {
    fn next(&mut self, kind: StatusEventKind) -> StatusEvent {
        self.seq += 1;
        StatusEvent::new(self.seq, polyph_proto::now(), kind)
    }

    pub fn snapshot(&mut self, s: StatusSnapshot) -> StatusEvent {
        self.next(StatusEventKind::Snapshot(s))
    }

    pub async fn translate(&mut self, ev: &ApiEvent, daemon: &Daemon) -> Vec<StatusEvent> {
        let mut out = vec![];
        match ev.kind() {
            "approval_pending" => {
                let snap = daemon.status().await;
                if let Some(id) = ev.event.pointer("/approval/id").and_then(|v| v.as_str())
                    && let Some(a) = snap.approvals.into_iter().find(|a| a.id == id)
                {
                    out.push(self.next(StatusEventKind::ApprovalPending { approval: a }));
                }
            }
            "approval_resolved" => {
                let id = ev.event.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                let approved = ev.event.pointer("/decision/t").and_then(|v| v.as_str()) == Some("approve");
                out.push(self.next(StatusEventKind::ApprovalResolved { id, approved }));
            }
            "claims_changed" => {
                let count = ev.event.get("count").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                out.push(self.next(StatusEventKind::ClaimsChanged { count }));
            }
            "connection" => {
                let connected = ev.event.get("connected").and_then(|v| v.as_bool()).unwrap_or(false);
                out.push(self.next(StatusEventKind::Connection { connected }));
            }
            "member_online" | "member_offline" | "member_joined" => {
                let snap = daemon.status().await;
                out.push(self.next(StatusEventKind::Snapshot(snap)));
            }
            _ => {}
        }
        out
    }
}

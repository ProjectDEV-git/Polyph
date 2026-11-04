//! Turns daemon events into the versioned status feed for the status pill,
//! Nublet, scripts and the D-Bus service. It carries states, names, counts
//! and colors only: never code, chat or terminal content.

use std::collections::HashMap;

use polyph_agents::usage::{BudgetScope, Limit};
use polyph_core::voice::{Step, Theme, voice_css};
use polyph_proto::crdt::{Activity, Board};
use polyph_proto::messages::{AgentEventKind, AgentState, DocName};
use polyph_proto::status::{StatusAgent, StatusBudget, StatusEvent, StatusEventKind, StatusSnapshot};

use crate::daemon::Daemon;
use crate::events::ApiEvent;
use crate::runtime::{RoomCtx, RoomRuntime};

#[derive(Debug, Default)]
pub struct StatusFeed {
    seq: u64,
    /// Activity entries already looked at, per room.
    activity_seen: HashMap<String, usize>,
    /// When the feed started: older activity is history, not news.
    started_at: i64,
}

fn theme(d: &Daemon) -> Theme {
    if d.shared.config.lock().theme == "light" { Theme::Light } else { Theme::Dark }
}

/// The voice color of a member's person (hue) or of their n-th agent.
fn hue_of(ctx: &RoomCtx, member: &str) -> u16 {
    if ctx.core.my_member.to_hex() == member {
        return ctx.core.my_profile.hue;
    }
    ctx.core
        .peers
        .values()
        .find(|p| p.member().is_some_and(|m| m.to_hex() == member))
        .and_then(|p| p.profile.as_ref().map(|x| x.hue))
        .unwrap_or(polyph_core::room::NO_HUE)
}

/// Agents in the room with their state, task and voice color.
pub fn agents(ctx: &mut RoomCtx, theme: Theme, local: &[polyph_proto::ids::ObjectId]) -> Vec<StatusAgent> {
    let cards = Board::new(ctx.core.doc(&DocName::Board)).all_cards();
    let names = crate::daemon::member_names(&ctx.core);
    let mut per_member: HashMap<String, u8> = HashMap::new();
    let mut logs: Vec<_> = ctx.transcripts.values().filter_map(|l| l.front().map(|f| (f.at, l))).collect();
    logs.sort_by_key(|(at, _)| *at);
    let mut out = vec![];
    for (_, log) in logs {
        let Some(first) = log.front() else { continue };
        let a = &first.agent;
        let member = a.member.to_hex();
        let mut state = AgentState::Starting;
        let mut task = None;
        for e in log {
            match &e.kind {
                AgentEventKind::Started { task: t, .. } => task = t.map(|t| t.to_hex()),
                AgentEventKind::State { state: s, .. } => state = *s,
                AgentEventKind::Stopped { .. } => state = AgentState::Stopped,
                _ => {}
            }
        }
        let n = per_member.entry(member.clone()).or_insert(0);
        *n = n.saturating_add(1);
        if state == AgentState::Stopped {
            continue;
        }
        let hue = hue_of(ctx, &member);
        let color =
            if hue == polyph_core::room::NO_HUE { String::new() } else { voice_css(hue, Step::Agent(*n - 1), theme) };
        out.push(StatusAgent {
            id: a.id.to_hex(),
            label: a.label.clone(),
            owner: names.get(&member).cloned().unwrap_or_default(),
            color,
            state,
            task: task.and_then(|t| cards.iter().find(|c| c.id == t).map(|c| c.title.clone())),
            mine: local.contains(&a.id),
            cloud: false,
        });
    }
    out
}

/// Budgets with what they have used.
pub async fn budgets(rt: &RoomRuntime, prices: &polyph_agents::usage::PriceTable) -> Vec<StatusBudget> {
    let (ledger, budgets) = crate::agents::ledger(rt, prices).await;
    budgets
        .iter()
        .map(|b| {
            let (limit, unit) = match b.limit {
                Limit::Tokens(n) => (n as f64, "tokens"),
                Limit::Dollars(x) => (x, "usd"),
            };
            StatusBudget {
                scope: match &b.scope {
                    BudgetScope::Room => "room".into(),
                    BudgetScope::Member(m) => format!("member:{m}"),
                },
                used: b.used(&ledger),
                limit,
                unit: unit.into(),
            }
        })
        .collect()
}

/// Fills in the parts of a snapshot that need the room's state.
pub async fn fill(d: &Daemon, rt: &RoomRuntime, s: &mut StatusSnapshot) {
    let local: Vec<_> = d.agents.map.lock().await.keys().copied().collect();
    s.agents = agents(&mut *rt.ctx.lock().await, theme(d), &local);
    s.budgets = budgets(rt, &Default::default()).await;
}

impl StatusFeed {
    /// A feed that reports what happens from `now` on.
    pub fn new(now: i64) -> Self {
        Self { started_at: now, ..Default::default() }
    }

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
            "agent" => {
                let e = &ev.event["event"];
                let id = e["agent"]["id"].as_str().unwrap_or_default().to_string();
                match e["kind"]["t"].as_str() {
                    Some("started") => {
                        let snap = daemon.status().await;
                        if let Some(a) = snap.agents.into_iter().find(|a| a.id == id) {
                            out.push(self.next(StatusEventKind::AgentStarted { agent: a }));
                        }
                    }
                    Some("state") => {
                        let state: Option<AgentState> = serde_json::from_value(e["kind"]["c"]["state"].clone()).ok();
                        if let Some(state) = state {
                            let snap = daemon.status().await;
                            let task = snap.agents.into_iter().find(|a| a.id == id).and_then(|a| a.task);
                            out.push(self.next(StatusEventKind::AgentState { id, state, task }));
                        }
                    }
                    Some("stopped") => out.push(self.next(StatusEventKind::AgentStopped { id })),
                    _ => {}
                }
            }
            "budget_warning" => {
                let b = &ev.event["budget"];
                let percent = ev.event["percent"].as_u64().unwrap_or(0) as u32;
                let (limit, unit) = match (b["limit"]["tokens"].as_f64(), b["limit"]["dollars"].as_f64()) {
                    (Some(t), _) => (t, "tokens"),
                    (_, Some(x)) => (x, "usd"),
                    _ => (0.0, "tokens"),
                };
                let scope = if b["scope"] == "room" {
                    "room".to_string()
                } else {
                    format!("member:{}", b["scope"]["member"].as_str().unwrap_or_default())
                };
                let used = limit * percent as f64 / 100.0;
                out.push(self.next(StatusEventKind::BudgetWarning {
                    budget: StatusBudget { scope, used, limit, unit: unit.into() },
                    percent,
                }));
            }
            "doc_changed" if ev.event["doc"] == "activity" || ev.event["doc"]["t"] == "activity" => {
                // Cards that moved, from the activity feed.
                let Some(room) = ev.room.clone() else { return out };
                let Ok(rt) = daemon.find(&room).await else { return out };
                let entries = Activity::new(rt.ctx.lock().await.core.doc(&DocName::Activity)).entries();
                let before_start = entries.iter().filter(|e| e.at < self.started_at).count();
                let seen = *self.activity_seen.entry(room.clone()).or_insert(before_start);
                self.activity_seen.insert(room, entries.len());
                for e in entries.iter().skip(seen).filter(|e| e.kind == "card_moved") {
                    out.push(self.next(StatusEventKind::TaskMoved {
                        task: e.args["card"].as_str().unwrap_or_default().to_string(),
                        title: e.args["title"].as_str().unwrap_or_default().to_string(),
                        column: e.args["to"].as_str().unwrap_or_default().to_string(),
                    }));
                }
            }
            _ => {}
        }
        out
    }
}

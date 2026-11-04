//! The status provider: the status feed over plain HTTP for widgets and
//! ambient displays (Nublet, dashboards, bar plugins), with no access to
//! anything else. See `docs/status-feed.md`.
//!
//! - `GET /status` — the current snapshot, one status feed line.
//! - `GET /status/events` — the live feed as Server-Sent Events, starting with
//!   a snapshot.
//! - `GET /status/widget` — a ready-to-draw summary in the person's language.
//!
//! Every route needs the read-only status token (`polyph status token`,
//! `?token=` or `Authorization: Bearer`). It opens nothing but these routes:
//! the feed carries states, names, counts and colors, never code, chat or
//! terminal content. Requests must name a loopback host.

use std::convert::Infallible;
use std::sync::Arc;

use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use polyph_i18n::Localizer;
use polyph_proto::messages::AgentState;
use polyph_proto::status::{StatusEvent, StatusEventKind, StatusSnapshot};
use serde_json::{Value, json};

use crate::daemon::Daemon;
use crate::paths::Paths;
use crate::status::StatusFeed;

/// Version of the widget document (`/status/widget`).
pub const WIDGET_VERSION: u32 = 1;

/// The read-only status token, created on first use.
pub fn token(paths: &Paths) -> anyhow::Result<String> {
    let p = paths.home.join("status.token");
    if let Ok(t) = std::fs::read_to_string(&p) {
        let t = t.trim().to_string();
        if t.len() >= 32 {
            return Ok(t);
        }
    }
    let mut b = [0u8; 24];
    getrandom::getrandom(&mut b).map_err(|e| anyhow::anyhow!("no system randomness: {e}"))?;
    let t = format!("st_{}", hex::encode(b));
    crate::paths::write_private(&p, t.as_bytes())?;
    Ok(t)
}

/// Makes a new status token; widgets using the old one stop working.
pub fn rotate(paths: &Paths) -> anyhow::Result<String> {
    let _ = std::fs::remove_file(paths.home.join("status.token"));
    token(paths)
}

/// Read-only data, meant for widgets in any origin: allow reading it from
/// pages (the token still gates it).
fn cors(mut r: Response) -> Response {
    let h = r.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

pub fn authorized(paths: &Paths, headers: &HeaderMap, query_token: Option<&str>) -> bool {
    let Ok(want) = token(paths) else { return false };
    let given = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .or(query_token);
    given.is_some_and(|g| crate::api::ct_eq(g.as_bytes(), want.as_bytes())) && crate::web::local_host(headers)
}

pub async fn snapshot(d: &Arc<Daemon>) -> Response {
    let mut f = StatusFeed::new(d.shared.clock.now());
    let line = f.snapshot(d.status().await);
    cors(axum::Json(serde_json::to_value(&line).unwrap_or(Value::Null)).into_response())
}

pub async fn events(d: Arc<Daemon>) -> Response {
    struct St {
        d: Arc<Daemon>,
        rx: tokio::sync::broadcast::Receiver<crate::events::ApiEvent>,
        feed: StatusFeed,
        queue: std::collections::VecDeque<StatusEvent>,
    }
    let rx = d.subscribe();
    let mut feed = StatusFeed::new(d.shared.clock.now());
    let first = feed.snapshot(d.status().await);
    let st = St { d, rx, feed, queue: [first].into() };
    // The snapshot first, then each status line as it happens.
    let stream = futures_util::stream::unfold(st, |mut st| async move {
        loop {
            if let Some(e) = st.queue.pop_front() {
                return Some((Ok::<_, Infallible>(line_event(&e)), st));
            }
            match st.rx.recv().await {
                Ok(ev) => {
                    let lines = st.feed.translate(&ev, &st.d).await;
                    st.queue.extend(lines);
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return None,
            }
        }
    });
    cors(Sse::new(stream).keep_alive(KeepAlive::default()).into_response())
}

fn line_event(e: &StatusEvent) -> Event {
    Event::default().event(kind_name(&e.kind)).data(e.to_json_line())
}

fn kind_name(k: &StatusEventKind) -> &'static str {
    match k {
        StatusEventKind::Snapshot(_) => "snapshot",
        StatusEventKind::AgentStarted { .. } => "agent_started",
        StatusEventKind::AgentState { .. } => "agent_state",
        StatusEventKind::AgentStopped { .. } => "agent_stopped",
        StatusEventKind::ApprovalPending { .. } => "approval_pending",
        StatusEventKind::ApprovalResolved { .. } => "approval_resolved",
        StatusEventKind::ClaimsChanged { .. } => "claims_changed",
        StatusEventKind::TaskMoved { .. } => "task_moved",
        StatusEventKind::BudgetWarning { .. } => "budget_warning",
        StatusEventKind::Connection { .. } => "connection",
    }
}

fn state_word(s: AgentState) -> (&'static str, &'static str) {
    // (state, symbol): the same symbols as the TUI, so a monochrome display still reads.
    match s {
        AgentState::Running | AgentState::Starting => ("running", "⠿"),
        AgentState::WaitingApproval => ("waiting", "!"),
        AgentState::Blocked => ("blocked", "■"),
        AgentState::Error => ("error", "✕"),
        AgentState::Done => ("done", "✓"),
        AgentState::Stopped => ("stopped", "·"),
        _ => ("idle", "○"),
    }
}

/// A summary any display can draw: a title, one line of state, a badge and
/// one item per agent, in the person's language.
pub fn widget(s: &StatusSnapshot, l: &Localizer, now: i64) -> Value {
    let mine: Vec<_> = s.approvals.iter().filter(|a| a.mine).collect();
    let running = s.agents.iter().filter(|a| matches!(a.state, AgentState::Running | AgentState::Starting)).count();
    let n = |v: usize| polyph_i18n::FluentValue::from(v as i64);
    let (state, subtitle) = if !s.connected && s.room.is_some() {
        ("offline", l.t("status-offline"))
    } else if let Some(a) = mine.first() {
        (
            "waiting",
            l.t_args("status-waiting", &[("agent", a.agent.as_str().into()), ("title", a.title.as_str().into())]),
        )
    } else if running > 0 {
        ("running", l.t_args("status-running", &[("n", n(running))]))
    } else if s.room.is_some() {
        ("idle", l.t("status-quiet"))
    } else {
        ("idle", l.t("status-no-room"))
    };
    let items: Vec<Value> = s
        .agents
        .iter()
        .map(|a| {
            let (st, sym) = state_word(a.state);
            json!({
                "label": a.label,
                "state": st,
                "symbol": sym,
                "word": l.t(&format!("tui-state-{st}")),
                "color": a.color,
                "detail": a.task,
                "mine": a.mine,
            })
        })
        .collect();
    json!({
        "v": WIDGET_VERSION,
        "title": s.room_name.clone().unwrap_or_else(|| l.t("app-name")),
        "subtitle": subtitle,
        "state": state,
        "badge": if mine.is_empty() { Value::Null } else { json!({ "count": mine.len(), "text": l.t_args("status-approvals", &[("n", n(mine.len()))]) }) },
        "online": s.members_online,
        "items": items,
        "updated_at": now,
    })
}

pub async fn widget_response(d: &Arc<Daemon>) -> Response {
    let s = d.status().await;
    let lang = d.shared.config.lock().lang.clone();
    let l = Localizer::new(&polyph_i18n::detect(lang.as_deref()));
    cors(axum::Json(widget(&s, &l, d.shared.clock.now())).into_response())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use polyph_proto::status::{StatusAgent, StatusApproval};

    fn snap() -> StatusSnapshot {
        StatusSnapshot {
            room: Some("r".into()),
            room_name: Some("payments".into()),
            connected: true,
            members_online: 3,
            agents: vec![StatusAgent {
                id: "a".into(),
                label: "Ann/claude".into(),
                owner: "Ann".into(),
                color: "oklch(0.760 0.120 255)".into(),
                state: AgentState::WaitingApproval,
                task: Some("Add refunds".into()),
                mine: true,
                cloud: false,
            }],
            approvals: vec![StatusApproval {
                id: "p".into(),
                agent: "Ann/claude".into(),
                title: "Run cargo publish".into(),
                mine: true,
                created_at: 0,
            }],
            budgets: vec![],
        }
    }

    #[test]
    fn the_widget_says_what_needs_you_in_every_language() {
        for lang in ["en", "th", "ja", "zh-Hans"] {
            let w = widget(&snap(), &Localizer::new(lang), 5);
            assert_eq!(w["state"], "waiting");
            assert_eq!(w["title"], "payments");
            assert!(w["subtitle"].as_str().unwrap().contains("Run cargo publish"), "{lang}: {w}");
            assert_eq!(w["badge"]["count"], 1);
            assert_eq!(w["items"][0]["symbol"], "!");
            assert!(!w["items"][0]["word"].as_str().unwrap().starts_with("tui-"), "{lang}");
        }
        let mut quiet = snap();
        quiet.approvals.clear();
        quiet.agents.clear();
        let w = widget(&quiet, &Localizer::new("en"), 5);
        assert_eq!(w["state"], "idle");
        assert_eq!(w["badge"], Value::Null);
    }

    #[test]
    fn the_token_is_kept_and_rotates() {
        let dir = tempfile::tempdir().unwrap();
        let p = Paths::at(dir.path());
        let a = token(&p).unwrap();
        assert_eq!(token(&p).unwrap(), a);
        assert_ne!(rotate(&p).unwrap(), a);
    }
}

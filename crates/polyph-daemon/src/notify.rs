//! Notifications for the person at this device, and outgoing webhooks.
//!
//! The notifier watches the daemon's events and turns the ones that need this
//! person into a [`Notification`]: an approval they are first to answer, the
//! result of their own merge, an @mention, an overlap or loop-guard stop by
//! their agent, a budget warning. Local apps receive it as a `notification`
//! event; webhooks (per device, `webhook.*` API) receive it as JSON, Slack or
//! Discord messages. Webhooks never carry chat text, code or file contents:
//! only what happened, where, and who.

use std::collections::{HashSet, VecDeque};
use std::time::Duration;

use hmac::{Hmac, Mac};
use polyph_i18n::{FluentValue, Localizer};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::Sha256;

use crate::daemon::{ApiError, Daemon, Shared};
use crate::events::ApiEvent;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Notification {
    pub id: String,
    pub kind: String,
    pub room: Option<String>,
    pub room_name: Option<String>,
    /// Translation key and arguments (`notify.ftl`).
    pub key: String,
    pub args: Value,
    /// The text in this person's language.
    pub text: String,
    pub urgent: bool,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Json,
    Slack,
    Discord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Webhook {
    pub id: String,
    pub url: String,
    pub format: Format,
    /// Kinds to send; empty means all.
    #[serde(default)]
    pub events: Vec<String>,
    /// Signs JSON deliveries (`X-Polyph-Signature: sha256=<hex>`).
    #[serde(default)]
    pub secret: Option<String>,
    /// Only these rooms (hex ids); empty means all.
    #[serde(default)]
    pub rooms: Vec<String>,
}

const KINDS: &[&str] =
    &["approval", "merged", "merge_failed", "changes_requested", "mention", "overlap", "budget", "loop_guard"];

pub fn kinds() -> &'static [&'static str] {
    KINDS
}

fn webhooks_path(shared: &Shared) -> std::path::PathBuf {
    shared.paths.home.join("webhooks.json")
}

pub fn load_webhooks(shared: &Shared) -> Vec<Webhook> {
    std::fs::read(webhooks_path(shared)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_webhooks(shared: &Shared, hooks: &[Webhook]) -> Result<(), ApiError> {
    let bytes = serde_json::to_vec_pretty(hooks).map_err(|e| ApiError::Failed(e.to_string()))?;
    // Secrets inside: owner-only, like the API token.
    crate::paths::write_private(&webhooks_path(shared), &bytes)?;
    Ok(())
}

pub fn add_webhook(shared: &Shared, mut w: Webhook) -> Result<Webhook, ApiError> {
    let url = reqwest::Url::parse(&w.url).map_err(|e| ApiError::BadRequest(format!("{}: {e}", w.url)))?;
    let local = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if url.scheme() != "https" && !(url.scheme() == "http" && local) {
        return Err(ApiError::BadRequest("webhooks must use https (http only for localhost)".into()));
    }
    if let Some(bad) = w.events.iter().find(|e| !KINDS.contains(&e.as_str())) {
        return Err(ApiError::BadRequest(format!("unknown event {bad}; known: {}", KINDS.join(", "))));
    }
    w.id = polyph_proto::ids::ObjectId::random().map_err(|e| ApiError::Failed(e.to_string()))?.short();
    let mut hooks = load_webhooks(shared);
    hooks.push(w.clone());
    save_webhooks(shared, &hooks)?;
    Ok(w)
}

pub fn remove_webhook(shared: &Shared, id: &str) -> Result<bool, ApiError> {
    let mut hooks = load_webhooks(shared);
    let before = hooks.len();
    hooks.retain(|h| h.id != id);
    save_webhooks(shared, &hooks)?;
    Ok(hooks.len() != before)
}

/// What a webhook sees: secrets never, and the URL's credentials hidden.
pub fn public(w: &Webhook) -> Value {
    let url = reqwest::Url::parse(&w.url)
        .map(|mut u| {
            let _ = u.set_password(None);
            let path_hint = if u.path().len() > 12 { format!("{}…", &u.path()[..12]) } else { u.path().to_string() };
            format!("{}://{}{}", u.scheme(), u.host_str().unwrap_or_default(), path_hint)
        })
        .unwrap_or_default();
    json!({ "id": w.id, "url": url, "format": w.format, "events": w.events, "rooms": w.rooms, "signed": w.secret.is_some() })
}

fn fluent_args(args: &Value) -> Vec<(String, FluentValue<'static>)> {
    args.as_object()
        .map(|o| {
            o.iter()
                .map(|(k, v)| {
                    let fv: FluentValue<'static> = match v {
                        Value::Number(n) => FluentValue::from(n.as_f64().unwrap_or(0.0)),
                        Value::String(s) => FluentValue::from(s.clone()),
                        Value::Bool(b) => FluentValue::from(b.to_string()),
                        Value::Null => FluentValue::from(""),
                        other => FluentValue::from(other.to_string()),
                    };
                    (k.clone(), fv)
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn render(l: &Localizer, key: &str, args: &Value) -> String {
    let a = fluent_args(args);
    let refs: Vec<(&str, FluentValue<'_>)> = a.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    l.t_args(key, &refs)
}

/// Turns one daemon event into a notification for this person, if it is one.
async fn derive(d: &Daemon, ev: &ApiEvent, seen: &mut HashSet<String>) -> Option<(String, Value, bool, String)> {
    let room = ev.room.clone()?;
    let rt = d.find(&room).await.ok()?;
    let (me, my_name, room_name) = {
        let ctx = rt.ctx.lock().await;
        (ctx.core.my_member.to_hex(), ctx.core.my_profile.display_name.clone(), ctx.core.name.clone())
    };
    let e = &ev.event;
    let (kind, args, urgent, dedupe) = match ev.kind() {
        "approval_pending" => {
            let a = &e["approval"];
            if a["agent"]["member"].as_str() != Some(me.as_str()) {
                return None;
            }
            let title = a["kind"].clone();
            let title: polyph_proto::messages::ApprovalKind = serde_json::from_value(title).ok()?;
            let id = a["id"].as_str().unwrap_or_default().to_string();
            (
                "approval",
                json!({ "agent": a["agent"]["label"], "action": crate::export::approval_title(&title), "room": room_name }),
                true,
                format!("approval:{id}"),
            )
        }
        "merge_queue" => {
            let m = &e["entry"];
            if m["member"].as_str() != Some(me.as_str()) {
                return None;
            }
            let state = m["state"].as_str().unwrap_or_default();
            let kind = match state {
                "merged" => "merged",
                "failed" => "merge_failed",
                "changes_requested" => "changes_requested",
                _ => return None,
            };
            (
                kind,
                json!({ "branch": m["branch"], "into": m["into"], "room": room_name }),
                kind != "merged",
                format!("merge:{}:{state}", m["id"].as_str().unwrap_or_default()),
            )
        }
        "chat" => {
            // History caught up after a reconnect is not news.
            let fresh = e["at"].as_i64().is_some_and(|at| d.shared.clock.now() - at < 600);
            if e["author"].as_str() == Some(me.as_str()) || my_name.is_empty() || !fresh {
                return None;
            }
            let text = e["text"].as_str().unwrap_or_default().to_lowercase();
            let tag = format!("@{}", my_name.to_lowercase());
            let hit = text.match_indices(&tag).any(|(i, _)| {
                text[i + tag.len()..].chars().next().is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != '-')
            });
            if !hit {
                return None;
            }
            let from = crate::daemon::member_names(&rt.ctx.lock().await.core)
                .get(e["author"].as_str().unwrap_or_default())
                .cloned()
                .unwrap_or_default();
            (
                "mention",
                json!({ "from": from, "room": room_name, "thread": e["thread"] }),
                false,
                format!("chat:{}", e["id"].as_str().unwrap_or_default()),
            )
        }
        "claim_overlap" => (
            "overlap",
            json!({ "agent": e["agent"], "path": e["path"], "holder": e["holder"], "room": room_name }),
            false,
            format!("overlap:{}:{}", e["agent"], e["path"]),
        ),
        "budget_warning" => (
            "budget",
            json!({ "percent": e["percent"], "scope": e["budget"]["scope"], "room": room_name }),
            false,
            format!("budget:{}:{}", e["budget"]["scope"], e["percent"]),
        ),
        "loop_guard" => {
            ("loop_guard", json!({ "agent": e["label"], "room": room_name }), false, format!("loop:{}", e["agent"]))
        }
        _ => return None,
    };
    if !seen.insert(dedupe) {
        return None;
    }
    Some((kind.to_string(), args, urgent, room_name))
}

fn sign(secret: &str, body: &[u8]) -> String {
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else { return String::new() };
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

async fn deliver(client: &reqwest::Client, w: &Webhook, n: &Notification) {
    let body = match w.format {
        Format::Json => json!({
            "v": 1, "id": n.id, "kind": n.kind, "room": n.room, "room_name": n.room_name,
            "text": n.text, "args": n.args, "urgent": n.urgent, "at": n.at
        }),
        Format::Slack => json!({ "text": n.text }),
        Format::Discord => json!({ "content": n.text, "allowed_mentions": { "parse": [] } }),
    };
    let body = body.to_string().into_bytes();
    for (attempt, wait) in [0u64, 1, 4].iter().enumerate() {
        if *wait > 0 {
            tokio::time::sleep(Duration::from_secs(*wait)).await;
        }
        let mut req = client
            .post(&w.url)
            .timeout(Duration::from_secs(10))
            .header("content-type", "application/json")
            .header("user-agent", concat!("polyph/", env!("CARGO_PKG_VERSION")))
            .header("x-polyph-event", n.kind.as_str())
            .header("x-polyph-delivery", format!("{}-{attempt}", n.id));
        if let Some(s) = &w.secret {
            req = req.header("x-polyph-signature", sign(s, &body));
        }
        match req.body(body.clone()).send().await {
            Ok(r) if r.status().is_success() => return,
            // A 4xx will not get better by retrying.
            Ok(r) if r.status().is_client_error() => {
                tracing::warn!(status = %r.status(), webhook = %w.id, "webhook refused");
                return;
            }
            _ => {}
        }
    }
    tracing::warn!(webhook = %w.id, "webhook delivery failed after retries");
}

/// Sends a test notification to one webhook and waits for it.
pub async fn test_webhook(shared: &Shared, id: &str) -> Result<(), ApiError> {
    let w = load_webhooks(shared)
        .into_iter()
        .find(|w| w.id == id)
        .ok_or_else(|| ApiError::NotFound("no such webhook".into()))?;
    let lang = shared.config.lock().lang.clone();
    let l = Localizer::new(&polyph_i18n::detect(lang.as_deref()));
    let n = Notification {
        id: polyph_proto::ids::ObjectId::random().map(|o| o.to_hex()).unwrap_or_default(),
        kind: "test".into(),
        room: None,
        room_name: None,
        key: "notify-test".into(),
        args: json!({}),
        text: l.t("notify-test"),
        urgent: false,
        at: shared.clock.now(),
    };
    deliver(&client(), &w, &n).await;
    Ok(())
}

pub fn client() -> reqwest::Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder().build().unwrap_or_default()
}

/// Runs for the daemon's lifetime.
pub async fn run(d: std::sync::Weak<Daemon>, mut rx: tokio::sync::broadcast::Receiver<ApiEvent>) {
    let client = client();
    let mut seen: HashSet<String> = HashSet::new();
    let mut order: VecDeque<String> = VecDeque::new();
    loop {
        let ev = match rx.recv().await {
            Ok(e) => e,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(_) => return,
        };
        let Some(d) = d.upgrade() else { return };
        let before: HashSet<String> = seen.clone();
        let Some((kind, args, urgent, room_name)) = derive(&d, &ev, &mut seen).await else { continue };
        for k in seen.difference(&before) {
            order.push_back(k.clone());
        }
        while order.len() > 5000 {
            if let Some(old) = order.pop_front() {
                seen.remove(&old);
            }
        }
        let lang = d.shared.config.lock().lang.clone();
        let l = Localizer::new(&polyph_i18n::detect(lang.as_deref()));
        let key = format!("notify-{}", kind.replace('_', "-"));
        let n = Notification {
            id: polyph_proto::ids::ObjectId::random().map(|o| o.to_hex()).unwrap_or_default(),
            text: polyph_core::redact::redact(&render(&l, &key, &args)).into_owned(),
            kind: kind.clone(),
            room: ev.room.clone(),
            room_name: Some(room_name),
            key,
            args,
            urgent,
            at: d.shared.clock.now(),
        };
        d.shared.emit(ApiEvent::global("notification", json!({ "notification": n })));
        for w in load_webhooks(&d.shared) {
            let wants_kind = w.events.is_empty() || w.events.contains(&kind);
            let wants_room =
                w.rooms.is_empty() || n.room.as_ref().is_some_and(|r| w.rooms.iter().any(|x| r.starts_with(x)));
            if wants_kind && wants_room {
                let (client, n) = (client.clone(), n.clone());
                tokio::spawn(async move { deliver(&client, &w, &n).await });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_matches_hmac_sha256() {
        // RFC 4231 test case 2.
        let s = sign("Jefe", b"what do ya want for nothing?");
        assert_eq!(s, "sha256=5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    }

    #[test]
    fn renders_in_the_persons_language() {
        let en = Localizer::new("en");
        let t =
            render(&en, "notify-merged", &json!({ "branch": "polyph/login-1", "into": "main", "room": "payments" }));
        assert!(t.contains("polyph/login-1") && t.contains("main") && t.contains("payments"), "{t}");
        let th = Localizer::new("th");
        assert_ne!(
            render(&th, "notify-merged", &json!({ "branch": "b", "into": "main", "room": "r" })),
            "notify-merged"
        );
    }
}

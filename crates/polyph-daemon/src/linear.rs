//! Linear: import issues as cards, and move them in Linear as their cards
//! move here (started when work begins, completed when it merges). Only the
//! device that moved the card calls Linear, so teammates never repeat it.

use std::sync::Arc;
use std::time::Duration;

use polyph_proto::crdt::{Board, NewCard, card};
use polyph_proto::ids::ObjectId;
use polyph_proto::messages::DocName;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::daemon::{ApiError, Daemon, Shared};
use crate::integrations::{self, Account};
use crate::runtime::{RoomRuntime, apply};

async fn gql(a: &Account, query: &str, variables: Value) -> Result<Value, ApiError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let r = reqwest::Client::builder()
        .user_agent(concat!("polyph/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| ApiError::Failed(e.to_string()))?
        .post(&a.api_url)
        .header(reqwest::header::AUTHORIZATION, a.token.as_str())
        .json(&json!({ "query": query, "variables": variables }))
        .send()
        .await
        .map_err(|e| ApiError::Failed(format!("could not reach Linear: {e}")))?;
    let status = r.status();
    let v: Value = r.json().await.unwrap_or(Value::Null);
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(ApiError::Forbidden("Linear refused the API key; run `polyph linear connect` again".into()));
    }
    if let Some(e) = v["errors"].as_array().and_then(|e| e.first()) {
        return Err(ApiError::Failed(format!("Linear: {}", e["message"].as_str().unwrap_or("error"))));
    }
    if !status.is_success() {
        return Err(ApiError::Failed(format!("Linear {status}")));
    }
    Ok(v["data"].clone())
}

fn account(shared: &Shared) -> Result<Account, ApiError> {
    integrations::linear(shared).ok_or_else(|| {
        ApiError::BadRequest("Linear is not connected; run `polyph linear connect` or set LINEAR_API_KEY".into())
    })
}

pub async fn connect(shared: &Shared, token: String, api_url: Option<String>) -> Result<Value, ApiError> {
    let mut a = Account {
        token: token.trim().to_string(),
        api_url: api_url.unwrap_or_else(|| "https://api.linear.app/graphql".into()),
        login: String::new(),
    };
    let v = gql(&a, "query { viewer { name email } }", json!({})).await?;
    a.login = v["viewer"]["name"].as_str().unwrap_or_default().to_string();
    let mut i = integrations::load(shared);
    i.linear = Some(a.clone());
    integrations::save(shared, &i)?;
    Ok(json!({ "login": a.login, "api_url": a.api_url }))
}

pub fn disconnect(shared: &Shared) -> Result<(), ApiError> {
    let mut i = integrations::load(shared);
    i.linear = None;
    integrations::save(shared, &i)
}

#[derive(Debug, Deserialize)]
pub struct ImportParams {
    pub room: String,
    /// Team key, e.g. "ENG".
    pub team: String,
    pub limit: Option<u32>,
}

const ISSUES: &str = "query($team: String!, $first: Int!) {
  issues(first: $first, filter: { team: { key: { eq: $team } }, state: { type: { nin: [\"completed\", \"canceled\"] } } }) {
    nodes { id identifier title description url state { type } }
  }
}";

/// Open issues of a team become backlog cards (once).
pub async fn import(d: &Arc<Daemon>, p: ImportParams) -> Result<Value, ApiError> {
    let rt = d.find(&p.room).await?;
    let a = account(&d.shared)?;
    let v = gql(&a, ISSUES, json!({ "team": p.team, "first": p.limit.unwrap_or(50).clamp(1, 250) })).await?;
    let now = d.shared.clock.now();
    let mut ctx = rt.ctx.lock().await;
    let known: Vec<String> =
        Board::new(ctx.core.doc(&DocName::Board)).all_cards().into_iter().filter_map(|c| c.external).collect();
    let me = ctx.core.my_member.to_hex();
    let red = |s: &str| polyph_core::redact::redact(s).into_owned();
    let mut cards = vec![];
    let mut added = vec![];
    for i in v["issues"]["nodes"].as_array().into_iter().flatten() {
        let Some(ident) = i["identifier"].as_str() else { continue };
        let ext = format!("linear:{ident}");
        if known.contains(&ext) {
            continue;
        }
        let id = ObjectId::random().map_err(|e| ApiError::Failed(e.to_string()))?.to_hex();
        let column = if i["state"]["type"] == "started" { "in_progress" } else { "backlog" };
        cards.push((
            NewCard {
                id: id.clone(),
                title: red(&format!("{ident} {}", i["title"].as_str().unwrap_or_default())),
                description: red(i["description"].as_str().unwrap_or_default()),
                column: column.into(),
                created_by: me.clone(),
                created_at: now,
                kind: Some("issue".into()),
                ..Default::default()
            },
            ext,
            i["url"].as_str().unwrap_or_default().to_string(),
        ));
        added.push(json!({ "id": id, "identifier": ident }));
    }
    let fx = ctx.core.change_doc(DocName::Board, |doc| {
        let b = Board::new(doc);
        for (c, ext, url) in &cards {
            if b.add_card(c).is_ok() {
                let _ = b.set_field(&c.id, card::EXTERNAL, yrs::Any::String(ext.as_str().into()), now);
                let _ = b.set_field(&c.id, card::URL, yrs::Any::String(url.as_str().into()), now);
            }
        }
    })?;
    apply(&rt, &mut ctx, &d.shared, fx).await;
    Ok(json!({ "team": p.team, "added": added }))
}

const STATES: &str =
    "query($id: String!) { issue(id: $id) { id state { type } team { states { nodes { id type position } } } } }";
const UPDATE: &str =
    "mutation($id: String!, $state: String!) { issueUpdate(id: $id, input: { stateId: $state }) { success } }";

/// The Linear state type a board column maps to, if any.
fn state_for(column: &str) -> Option<&'static str> {
    match column {
        "in_progress" | "review" => Some("started"),
        "done" => Some("completed"),
        _ => None,
    }
}

/// Moves the Linear issue behind a card that this device just moved.
pub async fn card_moved(shared: &Shared, rt: &RoomRuntime, card_id: &str, column: &str) -> Result<bool, ApiError> {
    let Some(want) = state_for(column) else { return Ok(false) };
    let ident = {
        let mut ctx = rt.ctx.lock().await;
        let c = Board::new(ctx.core.doc(&DocName::Board)).card(card_id);
        match c.and_then(|c| c.external).and_then(|e| e.strip_prefix("linear:").map(str::to_string)) {
            Some(i) => i,
            None => return Ok(false),
        }
    };
    let Some(a) = integrations::linear(shared) else { return Ok(false) };
    let v = gql(&a, STATES, json!({ "id": ident })).await?;
    let issue = &v["issue"];
    if issue["state"]["type"] == want {
        return Ok(false);
    }
    // The team's first state of that type (lowest position).
    let target = issue["team"]["states"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|s| s["type"] == want)
        .min_by(|x, y| x["position"].as_f64().unwrap_or(0.0).total_cmp(&y["position"].as_f64().unwrap_or(0.0)))
        .and_then(|s| s["id"].as_str())
        .ok_or_else(|| ApiError::Failed(format!("the Linear team has no {want} state")))?
        .to_string();
    let id = issue["id"].as_str().unwrap_or(&ident).to_string();
    let r = gql(&a, UPDATE, json!({ "id": id, "state": target })).await?;
    Ok(r["issueUpdate"]["success"].as_bool() == Some(true))
}

/// Fire-and-forget form for the places that move cards.
pub fn spawn_card_moved(shared: &Shared, room: polyph_proto::ids::RoomId, card_id: String, column: String) {
    if state_for(&column).is_none() || integrations::linear(shared).is_none() {
        return;
    }
    let Some(d) = shared.daemon.get().and_then(|w| w.upgrade()) else { return };
    tokio::spawn(async move {
        let Ok(rt) = d.find(&room.to_hex()).await else { return };
        match card_moved(&d.shared, &rt, &card_id, &column).await {
            Ok(true) => d.shared.emit(crate::events::ApiEvent::room_kind(
                &room,
                "linear_synced",
                json!({ "card": card_id, "column": column }),
            )),
            Ok(false) => {}
            Err(e) => d.shared.emit(crate::events::ApiEvent::room_kind(
                &room,
                "linear_error",
                json!({ "card": card_id, "error": e.to_string() }),
            )),
        }
    });
}

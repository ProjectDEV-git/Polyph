//! The local API: JSON-RPC 2.0 over a localhost WebSocket (`/ws`), plus
//! single calls over HTTP (`POST /rpc`). Every request needs the token from
//! `api.token` as `Authorization: Bearer <token>` or `?token=`.
//!
//! WebSocket clients receive events as `{"method":"event","params":{...}}`
//! and, after calling `status.watch`, status feed lines as
//! `{"method":"status","params":{...}}`.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use subtle_compare::eq;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

use crate::daemon::Daemon;
use crate::status::StatusFeed;

/// Largest request accepted on the local API.
const MAX_REQUEST: usize = 4 * 1024 * 1024;

#[derive(Clone)]
struct ApiState {
    daemon: Arc<Daemon>,
    token: Arc<String>,
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

mod subtle_compare {
    pub fn eq(a: &[u8], b: &[u8]) -> bool {
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }
}

fn authorized(st: &ApiState, headers: &HeaderMap, q: &TokenQuery) -> bool {
    let from_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);
    match from_header.or_else(|| q.token.clone()) {
        Some(t) => eq(t.as_bytes(), st.token.as_bytes()),
        None => false,
    }
}

#[derive(Debug)]
pub struct ApiServer {
    pub addr: SocketAddr,
    task: JoinHandle<()>,
}

impl ApiServer {
    pub fn abort(&self) {
        self.task.abort();
    }
}

/// Binds the local API on 127.0.0.1 and writes `daemon.json` with its port.
pub async fn serve(daemon: Arc<Daemon>, port: u16) -> anyhow::Result<ApiServer> {
    let token = crate::secrets::api_token(&daemon.shared.paths)?;
    let listener = match TcpListener::bind(("127.0.0.1", port)).await {
        Ok(l) => l,
        // The configured port is taken (another Polyph home); use any free port.
        Err(_) => TcpListener::bind(("127.0.0.1", 0)).await?,
    };
    let addr = listener.local_addr()?;
    let info = json!({ "port": addr.port(), "pid": std::process::id(), "version": env!("CARGO_PKG_VERSION") });
    crate::paths::write_private(&daemon.shared.paths.daemon_info(), info.to_string().as_bytes())?;
    let st = ApiState { daemon, token: Arc::new(token) };
    let app = Router::new()
        .route("/ws", get(ws))
        .route("/rpc", post(rpc))
        .route("/healthz", get(|| async { "ok" }))
        .with_state(st);
    let task = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(ApiServer { addr, task })
}

async fn rpc(
    State(st): State<ApiState>,
    headers: HeaderMap,
    Query(q): Query<TokenQuery>,
    Json(req): Json<Value>,
) -> Response {
    if !authorized(&st, &headers, &q) {
        return (StatusCode::UNAUTHORIZED, "missing or wrong API token").into_response();
    }
    Json(dispatch(&st, req).await).into_response()
}

async fn dispatch(st: &ApiState, req: Value) -> Value {
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = req.get("method").and_then(|m| m.as_str()) else {
        return json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32600, "message": "missing method" } });
    };
    let params = req.get("params").cloned().unwrap_or_else(|| json!({}));
    match st.daemon.call(method, params).await {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": e.code(), "message": e.to_string() } }),
    }
}

async fn ws(
    State(st): State<ApiState>,
    headers: HeaderMap,
    Query(q): Query<TokenQuery>,
    up: WebSocketUpgrade,
) -> Response {
    if !authorized(&st, &headers, &q) {
        return (StatusCode::UNAUTHORIZED, "missing or wrong API token").into_response();
    }
    up.max_message_size(MAX_REQUEST).on_upgrade(move |s| session(st, s))
}

async fn session(st: ApiState, socket: WebSocket) {
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(1024);
    let writer = tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            if sink.send(Message::Text(m.into())).await.is_err() {
                break;
            }
        }
    });
    let mut events = st.daemon.subscribe();
    let mut feed: Option<StatusFeed> = None;
    loop {
        tokio::select! {
            m = stream.next() => {
                let text = match m {
                    Some(Ok(Message::Text(t))) => t.to_string(),
                    Some(Ok(Message::Binary(_) | Message::Ping(_) | Message::Pong(_))) => continue,
                    _ => break,
                };
                let Ok(req) = serde_json::from_str::<Value>(&text) else { continue };
                if req.get("method").and_then(|m| m.as_str()) == Some("status.watch") {
                    let mut f = StatusFeed::new(st.daemon.shared.clock.now());
                    let snap = f.snapshot(st.daemon.status().await);
                    let _ = tx.send(json!({ "jsonrpc": "2.0", "id": req.get("id").cloned().unwrap_or(Value::Null), "result": {} }).to_string()).await;
                    let _ = tx.send(json!({ "jsonrpc": "2.0", "method": "status", "params": snap }).to_string()).await;
                    feed = Some(f);
                    continue;
                }
                let st2 = st.clone();
                let tx2 = tx.clone();
                // Long calls (join waits for approval) must not block events.
                tokio::spawn(async move {
                    let resp = dispatch(&st2, req).await;
                    let _ = tx2.send(resp.to_string()).await;
                });
            }
            e = events.recv() => match e {
                Ok(ev) => {
                    if let Some(f) = feed.as_mut() {
                        for s in f.translate(&ev, &st.daemon).await {
                            let _ = tx.send(json!({ "jsonrpc": "2.0", "method": "status", "params": s }).to_string()).await;
                        }
                    }
                    let _ = tx.send(json!({ "jsonrpc": "2.0", "method": "event", "params": ev }).to_string()).await;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    }
    writer.abort();
}

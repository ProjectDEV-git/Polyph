//! The local API: JSON-RPC 2.0 over a localhost WebSocket (`/ws`), plus
//! single calls over HTTP (`POST /rpc`). Every request needs the token from
//! `api.token` as `Authorization: Bearer <token>` or `?token=`.
//!
//! The browser client signs in with a one-time link instead (see `web.rs`).
//!
//! WebSocket clients receive events as `{"method":"event","params":{...}}`
//! and, after calling `status.watch`, status feed lines as
//! `{"method":"status","params":{...}}`.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as UrlPath, Query, State};
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
    web: Arc<crate::web::WebAuth>,
    web_dir: Option<Arc<std::path::PathBuf>>,
}

/// How a request proved itself: the API token, or a browser session.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Auth {
    Token,
    Session,
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

/// Constant-time comparison for secrets.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    eq(a, b)
}

fn authorized(st: &ApiState, headers: &HeaderMap, q: &TokenQuery) -> Option<Auth> {
    let from_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::to_string);
    let t = from_header.or_else(|| q.token.clone())?;
    if eq(t.as_bytes(), st.token.as_bytes()) {
        Some(Auth::Token)
    } else if st.web.session_valid(&t) {
        Some(Auth::Session)
    } else {
        None
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
    let web_dir = crate::web::web_dir().map(Arc::new);
    let st = ApiState { daemon, token: Arc::new(token), web: Arc::default(), web_dir };
    let app = Router::new()
        .route("/ws", get(ws))
        .route("/rpc", post(rpc))
        .route("/healthz", get(|| async { "ok" }))
        .route("/web/session", post(web_session))
        .route("/status", get(status_snapshot))
        .route("/status/events", get(status_events))
        .route("/status/widget", get(status_widget))
        .route("/", get(|| async { axum::response::Redirect::temporary("/app/") }))
        .route("/{*path}", get(web_file))
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
    let Some(auth) = authorized(&st, &headers, &q) else {
        return (StatusCode::UNAUTHORIZED, "missing or wrong API token").into_response();
    };
    if !crate::web::origin_allowed(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    Json(dispatch(&st, auth, req).await).into_response()
}

#[derive(Deserialize)]
struct SessionReq {
    code: String,
}

/// Trades a one-time link code for a browser session token.
async fn web_session(State(st): State<ApiState>, headers: HeaderMap, Json(req): Json<SessionReq>) -> Response {
    if !crate::web::local_host(&headers) || !crate::web::origin_allowed(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match st.web.redeem(&req.code) {
        Some(token) => Json(json!({ "token": token })).into_response(),
        None => (StatusCode::UNAUTHORIZED, "link expired or already used").into_response(),
    }
}

async fn status_snapshot(State(st): State<ApiState>, headers: HeaderMap, Query(q): Query<TokenQuery>) -> Response {
    if !crate::provider::authorized(&st.daemon.shared.paths, &headers, q.token.as_deref()) {
        return (StatusCode::UNAUTHORIZED, "missing or wrong status token").into_response();
    }
    crate::provider::snapshot(&st.daemon).await
}

async fn status_events(State(st): State<ApiState>, headers: HeaderMap, Query(q): Query<TokenQuery>) -> Response {
    if !crate::provider::authorized(&st.daemon.shared.paths, &headers, q.token.as_deref()) {
        return (StatusCode::UNAUTHORIZED, "missing or wrong status token").into_response();
    }
    crate::provider::events(st.daemon.clone()).await
}

async fn status_widget(State(st): State<ApiState>, headers: HeaderMap, Query(q): Query<TokenQuery>) -> Response {
    if !crate::provider::authorized(&st.daemon.shared.paths, &headers, q.token.as_deref()) {
        return (StatusCode::UNAUTHORIZED, "missing or wrong status token").into_response();
    }
    crate::provider::widget_response(&st.daemon).await
}

async fn web_file(State(st): State<ApiState>, headers: HeaderMap, UrlPath(path): UrlPath<String>) -> Response {
    if !crate::web::local_host(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match &st.web_dir {
        Some(dir) => crate::web::serve_file(dir, &path).await,
        None => (StatusCode::NOT_FOUND, "the browser client is not installed").into_response(),
    }
}

async fn dispatch(st: &ApiState, auth: Auth, req: Value) -> Value {
    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = req.get("method").and_then(|m| m.as_str()) else {
        return json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32600, "message": "missing method" } });
    };
    if method == "status.token" {
        // The read-only token for widgets; only the API token's holder may see or change it.
        if auth != Auth::Token {
            return json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32001, "message": "not allowed from a browser session" } });
        }
        let rotate = req.pointer("/params/rotate").and_then(Value::as_bool).unwrap_or(false);
        let paths = &st.daemon.shared.paths;
        let t = if rotate { crate::provider::rotate(paths) } else { crate::provider::token(paths) };
        return match t {
            Ok(token) => json!({ "jsonrpc": "2.0", "id": id, "result": { "token": token } }),
            Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32603, "message": e.to_string() } }),
        };
    }
    if method == "web.link" {
        // Only the API token's holder (the CLI, the desktop app) can sign in a browser.
        if auth != Auth::Token || st.web_dir.is_none() {
            let msg = if st.web_dir.is_none() {
                "the browser client is not installed"
            } else {
                "not allowed from a browser session"
            };
            return json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32001, "message": msg } });
        }
        return match st.web.link() {
            Some(code) => json!({ "jsonrpc": "2.0", "id": id, "result": { "code": code } }),
            None => {
                json!({ "jsonrpc": "2.0", "id": id, "error": { "code": -32603, "message": "no system randomness" } })
            }
        };
    }
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
    let Some(auth) = authorized(&st, &headers, &q) else {
        return (StatusCode::UNAUTHORIZED, "missing or wrong API token").into_response();
    };
    if !crate::web::origin_allowed(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    up.max_message_size(MAX_REQUEST).on_upgrade(move |s| session(st, auth, s))
}

async fn session(st: ApiState, auth: Auth, socket: WebSocket) {
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
                    let resp = dispatch(&st2, auth, req).await;
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

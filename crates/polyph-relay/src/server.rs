//! HTTP and WebSocket front end: `/ws`, `/healthz`, `/metrics`.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use polyph_proto::identity::{ctx, verify};
use polyph_proto::relay_wire::{ClientFrame, ErrorCode, MAX_FRAME, RelayFrame, auth_message};
use polyph_proto::version::{PROTOCOL_VERSION, is_compatible};
use tracing::debug;

use crate::hub::Hub;
use crate::limiter::Bucket;
use crate::metrics::inc;

const AUTH_TIMEOUT: Duration = Duration::from_secs(10);
const IDLE_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_RATE_STRIKES: u32 = 200;

pub fn router(hub: Arc<Hub>) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/healthz", get(health))
        .route("/metrics", get(metrics))
        .with_state(hub)
}

async fn health(State(hub): State<Arc<Hub>>) -> impl IntoResponse {
    let (rooms, _, _) = hub.stats();
    Json(serde_json::json!({
        "status": "ok",
        "protocol": PROTOCOL_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        "rooms": rooms,
        "url": hub.cfg.public_url,
    }))
}

async fn metrics(State(hub): State<Arc<Hub>>) -> Response {
    if !hub.cfg.metrics {
        return StatusCode::NOT_FOUND.into_response();
    }
    let (rooms, bytes, codes) = hub.stats();
    ([("content-type", "text/plain; version=0.0.4")], hub.metrics.render(rooms, bytes, codes)).into_response()
}

fn client_ip(peer: SocketAddr, headers: &HeaderMap, trusted: Option<&str>) -> IpAddr {
    let first = |name: &str| -> Option<IpAddr> {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .and_then(|v| v.trim().parse().ok())
    };
    // A proxy every connection passes through (Fly.io) names the client in
    // its own header; trust it only when configured.
    if let Some(ip) = trusted.and_then(first) {
        return ip;
    }
    // Otherwise only trust X-Forwarded-For from a proxy on the same machine (Caddy).
    match peer.ip().is_loopback() {
        true => first("x-forwarded-for").unwrap_or(peer.ip()),
        false => peer.ip(),
    }
}

async fn ws_handler(
    State(hub): State<Arc<Hub>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    let ip = client_ip(peer, &headers, hub.cfg.client_ip_header.as_deref());
    let Some((conn, rx)) = hub.connect(ip) else {
        return (StatusCode::TOO_MANY_REQUESTS, "too many connections from this address").into_response();
    };
    ws.max_message_size(MAX_FRAME).max_frame_size(MAX_FRAME).on_upgrade(move |socket| run(hub, conn, rx, socket))
}

async fn run(
    hub: Arc<Hub>,
    conn: Arc<crate::hub::Conn>,
    mut rx: tokio::sync::mpsc::Receiver<bytes::Bytes>,
    socket: WebSocket,
) {
    let (mut sink, mut stream) = socket.split();
    let writer = tokio::spawn(async move {
        while let Some(b) = rx.recv().await {
            if sink.send(Message::Binary(b)).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });

    let device = match authenticate(&hub, &conn, &mut stream).await {
        Some(d) => d,
        None => {
            hub.disconnect(&conn);
            finish(conn, writer).await;
            return;
        }
    };
    *conn.device.lock() = Some(device);
    hub.send(&conn, &RelayFrame::Welcome { device });

    let mut bucket = Bucket::new(hub.cfg.frames_per_sec as f64, hub.cfg.frames_per_sec as f64 * 2.0);
    let mut strikes = 0u32;
    loop {
        let msg = tokio::select! {
            m = tokio::time::timeout(IDLE_TIMEOUT, stream.next()) => m,
            _ = conn.kill.notified() => break,
        };
        let data = match msg {
            Ok(Some(Ok(Message::Binary(b)))) => b,
            Ok(Some(Ok(Message::Ping(_) | Message::Pong(_)))) => continue,
            Ok(Some(Ok(Message::Text(_)))) => {
                hub.send(&conn, &RelayFrame::Error { code: ErrorCode::BadFrame, detail: "binary frames only".into() });
                continue;
            }
            _ => break,
        };
        if let Some(cap) = &hub.capture {
            cap.lock().push(data.to_vec());
        }
        if !bucket.take() {
            inc(&hub.metrics.rate_limited_total);
            strikes += 1;
            if strikes > MAX_RATE_STRIKES {
                break;
            }
            hub.send(&conn, &RelayFrame::Error { code: ErrorCode::RateLimited, detail: "slow down".into() });
            continue;
        }
        match ClientFrame::decode(&data) {
            Ok(f) => hub.handle(&conn, device, f).await,
            Err(_) => {
                inc(&hub.metrics.rejected_frames_total);
                hub.send(
                    &conn,
                    &RelayFrame::Error { code: ErrorCode::BadFrame, detail: "could not decode frame".into() },
                );
            }
        }
    }
    debug!(conn = conn.id, "connection closed");
    hub.disconnect(&conn);
    finish(conn, writer).await;
}

/// Drops our handle so the writer drains queued frames (such as a final
/// error) and closes the socket, then gives up after a short grace period.
async fn finish(conn: Arc<crate::hub::Conn>, mut writer: tokio::task::JoinHandle<()>) {
    drop(conn);
    if tokio::time::timeout(Duration::from_secs(2), &mut writer).await.is_err() {
        writer.abort();
    }
}

async fn next_frame<S>(stream: &mut S) -> Option<(Vec<u8>, ClientFrame)>
where
    S: futures_util::Stream<Item = Result<Message, axum::Error>> + Unpin,
{
    loop {
        match tokio::time::timeout(AUTH_TIMEOUT, stream.next()).await {
            Ok(Some(Ok(Message::Binary(b)))) => {
                let f = ClientFrame::decode(&b).ok()?;
                return Some((b.to_vec(), f));
            }
            Ok(Some(Ok(Message::Ping(_) | Message::Pong(_)))) => continue,
            _ => return None,
        }
    }
}

async fn authenticate<S>(hub: &Hub, conn: &crate::hub::Conn, stream: &mut S) -> Option<polyph_proto::ids::DeviceId>
where
    S: futures_util::Stream<Item = Result<Message, axum::Error>> + Unpin,
{
    let (raw, hello) = next_frame(stream).await?;
    if let Some(cap) = &hub.capture {
        cap.lock().push(raw);
    }
    let ClientFrame::Hello { protocol, device, .. } = hello else {
        hub.send(conn, &RelayFrame::Error { code: ErrorCode::Unauthenticated, detail: "send hello first".into() });
        return None;
    };
    if !is_compatible(protocol) {
        hub.send(
            conn,
            &RelayFrame::Error {
                code: ErrorCode::UnsupportedVersion,
                detail: format!("relay speaks protocol {PROTOCOL_VERSION:#06x}"),
            },
        );
        return None;
    }
    let nonce: [u8; 32] = rand::random();
    hub.send(conn, &RelayFrame::Challenge { nonce, protocol: PROTOCOL_VERSION });
    let (raw, auth) = next_frame(stream).await?;
    if let Some(cap) = &hub.capture {
        cap.lock().push(raw);
    }
    let ClientFrame::Auth { sig } = auth else {
        hub.send(conn, &RelayFrame::Error { code: ErrorCode::Unauthenticated, detail: "expected auth".into() });
        return None;
    };
    if verify(&device, ctx::RELAY_AUTH, &auth_message(&nonce, &device), &sig).is_err() {
        hub.send(
            conn,
            &RelayFrame::Error { code: ErrorCode::Unauthenticated, detail: "signature did not verify".into() },
        );
        return None;
    }
    Some(device)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, v.parse().unwrap());
        }
        h
    }

    #[test]
    fn client_ip_trusts_only_what_it_should() {
        let local: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let remote: SocketAddr = "[fdaa::3]:5000".parse().unwrap();
        let h = headers(&[("x-forwarded-for", "203.0.113.7, 10.0.0.1"), ("fly-client-ip", "198.51.100.9")]);
        // Caddy on the same machine: X-Forwarded-For.
        assert_eq!(client_ip(local, &h, None).to_string(), "203.0.113.7");
        // A remote peer cannot claim an address.
        assert_eq!(client_ip(remote, &h, None).to_string(), "fdaa::3");
        // Behind Fly's proxy: its header, from any peer.
        assert_eq!(client_ip(remote, &h, Some("fly-client-ip")).to_string(), "198.51.100.9");
        // The configured header missing: back to the usual rules.
        assert_eq!(client_ip(remote, &headers(&[]), Some("fly-client-ip")).to_string(), "fdaa::3");
    }
}

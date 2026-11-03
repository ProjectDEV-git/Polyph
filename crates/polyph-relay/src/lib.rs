//! The Polyph relay: a WebSocket server that routes end-to-end encrypted
//! envelopes between room members. It never sees plaintext.
//!
//! Embeddable: [`serve`] starts a relay in-process (used for LAN mode and
//! tests) and returns a handle with its address.

pub mod client;
pub mod config;
pub mod hub;
pub mod limiter;
pub mod metrics;
pub mod server;
pub mod store;
pub mod tls;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Extension;
use axum::extract::ConnectInfo;
use hyper_util::rt::TokioIo;
use hyper_util::service::TowerToHyperService;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{info, warn};

pub use config::{Limits, Policy, RelayConfig, StaticPolicy};
pub use hub::{Capture, Hub};

#[derive(Debug, Default, Clone)]
pub struct ServeOptions {
    /// Serve TLS with this config (LAN mode passes a self-signed one).
    pub tls: Option<Arc<rustls::ServerConfig>>,
    /// Record raw inbound frames (tests only).
    pub capture: Option<Capture>,
    /// Override plan enforcement (the cloud build passes its own).
    pub policy: Option<Arc<dyn Policy>>,
}

#[derive(Debug)]
pub struct RelayHandle {
    pub addr: SocketAddr,
    pub hub: Arc<Hub>,
    pub tls: bool,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
    maintenance: JoinHandle<()>,
}

impl RelayHandle {
    pub fn url(&self) -> String {
        let scheme = if self.tls { "wss" } else { "ws" };
        format!("{scheme}://{}/ws", self.addr)
    }

    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        self.maintenance.abort();
        let _ = tokio::time::timeout(Duration::from_secs(5), &mut self.task).await;
        self.task.abort();
    }

    /// Waits until the server stops (for the standalone binary).
    pub async fn wait(self) {
        let _ = self.task.await;
    }
}

pub async fn serve(cfg: RelayConfig, opts: ServeOptions) -> anyhow::Result<RelayHandle> {
    let store = store::Store::open(&cfg.database).await?;
    let policy = opts.policy.unwrap_or_else(|| Arc::new(StaticPolicy(cfg.limits.clone())));
    let tls = match (opts.tls, &cfg.tls_cert, &cfg.tls_key) {
        (Some(t), _, _) => Some(t),
        (None, Some(c), Some(k)) => Some(tls::load_pem(c, k)?),
        _ => None,
    };
    let listener = TcpListener::bind(cfg.bind).await?;
    let addr = listener.local_addr()?;
    let hub = Hub::new(cfg, policy, store, opts.capture);
    let app = server::router(hub.clone());
    let (tx, rx) = oneshot::channel::<()>();

    let task = match tls.clone() {
        None => tokio::spawn(async move {
            let svc = app.into_make_service_with_connect_info::<SocketAddr>();
            if let Err(e) = axum::serve(listener, svc)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await
            {
                warn!(error = %e, "relay server stopped");
            }
        }),
        Some(tls_cfg) => tokio::spawn(async move {
            let acceptor = tokio_rustls::TlsAcceptor::from(tls_cfg);
            let mut rx = rx;
            loop {
                let (tcp, peer) = tokio::select! {
                    r = listener.accept() => match r { Ok(v) => v, Err(_) => continue },
                    _ = &mut rx => break,
                };
                let acceptor = acceptor.clone();
                let app = app.clone().layer(Extension(ConnectInfo(peer)));
                tokio::spawn(async move {
                    let Ok(Ok(stream)) = tokio::time::timeout(Duration::from_secs(10), acceptor.accept(tcp)).await
                    else {
                        return;
                    };
                    let svc = TowerToHyperService::new(app);
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), svc)
                        .with_upgrades()
                        .await;
                });
            }
        }),
    };

    let mh = hub.clone();
    let maintenance = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(15));
        let mut n: u64 = 0;
        loop {
            tick.tick().await;
            mh.sweep();
            if n.is_multiple_of(240) {
                mh.expire_snapshots().await;
            }
            n += 1;
        }
    });

    info!(%addr, tls = tls.is_some(), "relay listening");
    Ok(RelayHandle { addr, hub, tls: tls.is_some(), shutdown: Some(tx), task, maintenance })
}

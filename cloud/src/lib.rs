//! Polyph's hosted service (proprietary; see `LICENSE` in this directory).
//!
//! One process runs the hosted relay with a plan policy fed by accounts,
//! and the account API the website and daemons call: sign-in by email code
//! or GitHub, devices linked to accounts by signature, teams, plans and
//! Stripe billing. The relay still sees only ciphertext; accounts add who
//! pays for which devices, nothing about what rooms contain.

pub mod api;
pub mod billing;
pub mod config;
pub mod db;
pub mod mail;
pub mod plans;
pub mod policy;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::warn;

pub use config::CloudConfig;

/// An HTTP client with the ring TLS provider installed.
pub fn http_client() -> reqwest::Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder().timeout(Duration::from_secs(20)).build().unwrap_or_default()
}

#[derive(Debug, Default)]
pub struct CloudOptions {
    /// Replace the configured mailer (tests keep mail in memory).
    pub mailer: Option<mail::Mailer>,
}

#[derive(Debug)]
pub struct CloudHandle {
    pub api_addr: SocketAddr,
    pub relay: Option<polyph_relay::RelayHandle>,
    pub app: Arc<api::App>,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
    sweeper: JoinHandle<()>,
}

impl CloudHandle {
    pub fn api_url(&self) -> String {
        format!("http://{}", self.api_addr)
    }

    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        self.sweeper.abort();
        if let Some(r) = self.relay.take() {
            r.shutdown().await;
        }
        let _ = tokio::time::timeout(Duration::from_secs(5), &mut self.task).await;
        self.task.abort();
    }

    pub async fn wait(self) {
        let _ = self.task.await;
    }
}

pub async fn serve(cfg: CloudConfig, opts: CloudOptions) -> anyhow::Result<CloudHandle> {
    cfg.check()?;
    let db = db::Db::open(&cfg.database).await?;
    let policy = policy::CloudPolicy::new(cfg.plans.clone());
    policy.refresh(&db).await?;
    let relay = match cfg.relay.clone() {
        Some(r) => Some(
            polyph_relay::serve(
                r,
                polyph_relay::ServeOptions { policy: Some(Arc::new(policy.clone())), ..Default::default() },
            )
            .await?,
        ),
        None => None,
    };
    let mailer = opts.mailer.unwrap_or_else(|| mail::Mailer::from_config(&cfg));
    let listener = TcpListener::bind(cfg.api_bind).await?;
    let api_addr = listener.local_addr()?;
    let app = Arc::new(api::App::new(cfg, db, policy, mailer));
    let router = api::router(app.clone());
    let (tx, rx) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let svc = router.into_make_service_with_connect_info::<SocketAddr>();
        if let Err(e) = axum::serve(listener, svc)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await
        {
            warn!(error = %e, "account API stopped");
        }
    });
    let sweep_app = app.clone();
    let sweeper = tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(600));
        loop {
            tick.tick().await;
            if let Err(e) = sweep_app.db.sweep(api::now()).await {
                warn!(error = %e, "sweep failed");
            }
        }
    });
    Ok(CloudHandle { api_addr, relay, app, shutdown: Some(tx), task, sweeper })
}

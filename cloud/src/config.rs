//! Service configuration: a TOML file, then `POLYPH_CLOUD_*` variables for
//! secrets so they never need to sit in the file.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::Path;

use serde::Deserialize;

use crate::plans::{PlanConfig, PlanId};

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct CloudConfig {
    /// Where the account API listens.
    pub api_bind: SocketAddr,
    /// SQLite database for accounts, teams and billing.
    pub database: String,
    /// The hosted relay, run in the same process with this service's plan policy.
    pub relay: Option<polyph_relay::RelayConfig>,
    /// The website's origin (for links in emails and billing return pages).
    pub site_url: String,
    /// Browser origins allowed to call the API (the website).
    pub allowed_origins: Vec<String>,
    /// Plans and their limits. Prices are display labels only and stay unset
    /// until decided; the website then shows "Pricing coming soon".
    pub plans: BTreeMap<PlanId, PlanConfig>,
    pub mail: MailConfig,
    pub github: Option<GithubConfig>,
    pub stripe: Option<StripeConfig>,
    /// Development mode: sign-in codes are written to the log instead of
    /// mailed. Refused together with a real mail provider.
    pub dev: bool,
    /// Sign-in sessions last this many days.
    pub session_days: u32,
}

impl Default for CloudConfig {
    fn default() -> Self {
        Self {
            api_bind: SocketAddr::from(([127, 0, 0, 1], 8790)),
            database: "polyph-cloud.db".into(),
            relay: None,
            site_url: "https://polyph.dev".into(),
            allowed_origins: vec![],
            plans: crate::plans::defaults(),
            mail: MailConfig::default(),
            github: None,
            stripe: None,
            dev: false,
            session_days: 30,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct MailConfig {
    /// A Resend-compatible endpoint (`POST {"from","to","subject","text"}`
    /// with a bearer key), e.g. `https://api.resend.com/emails`.
    pub endpoint: Option<String>,
    pub api_key: Option<String>,
    pub from: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GithubConfig {
    /// OAuth app client id with the device flow enabled.
    pub client_id: String,
    #[serde(default = "github_web")]
    pub web_url: String,
    #[serde(default = "github_api")]
    pub api_url: String,
}

fn github_web() -> String {
    "https://github.com".into()
}
fn github_api() -> String {
    "https://api.github.com".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct StripeConfig {
    pub secret_key: String,
    pub webhook_secret: String,
    #[serde(default = "stripe_api")]
    pub api_url: String,
}

fn stripe_api() -> String {
    "https://api.stripe.com".into()
}

impl CloudConfig {
    /// In-memory configuration for tests.
    pub fn ephemeral() -> Self {
        Self {
            api_bind: SocketAddr::from(([127, 0, 0, 1], 0)),
            database: ":memory:".into(),
            dev: true,
            ..Self::default()
        }
    }

    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        let mut cfg: Self = match path {
            Some(p) => {
                let text =
                    std::fs::read_to_string(p).map_err(|e| anyhow::anyhow!("cannot read {}: {e}", p.display()))?;
                toml::from_str(&text).map_err(|e| anyhow::anyhow!("invalid cloud config {}: {e}", p.display()))?
            }
            None => Self::default(),
        };
        for (id, plan) in crate::plans::defaults() {
            cfg.plans.entry(id).or_insert(plan);
        }
        let var = |k: &str| std::env::var(format!("POLYPH_CLOUD_{k}")).ok().filter(|v| !v.is_empty());
        if let Some(v) = var("API_BIND") {
            cfg.api_bind = v.parse().map_err(|e| anyhow::anyhow!("POLYPH_CLOUD_API_BIND: {e}"))?;
        }
        if let Some(v) = var("DATABASE") {
            cfg.database = v;
        }
        if let Some(v) = var("MAIL_API_KEY") {
            cfg.mail.api_key = Some(v);
        }
        if let (Some(s), Some(w)) = (var("STRIPE_SECRET_KEY"), var("STRIPE_WEBHOOK_SECRET")) {
            cfg.stripe = Some(StripeConfig { secret_key: s, webhook_secret: w, api_url: stripe_api() });
        }
        cfg.check()?;
        Ok(cfg)
    }

    pub fn check(&self) -> anyhow::Result<()> {
        if self.dev && self.mail.endpoint.is_some() {
            anyhow::bail!("dev = true writes sign-in codes to the log; remove it when a mail endpoint is set");
        }
        if !self.dev && self.mail.endpoint.is_none() {
            anyhow::bail!("set [mail] endpoint, api_key and from (or dev = true for local testing)");
        }
        if self.mail.endpoint.is_some() && (self.mail.api_key.is_none() || self.mail.from.is_empty()) {
            anyhow::bail!("[mail] needs api_key (or POLYPH_CLOUD_MAIL_API_KEY) and from");
        }
        Ok(())
    }
}

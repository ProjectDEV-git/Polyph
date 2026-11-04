//! Credentials for GitHub and Linear, per device, sealed with the storage
//! key (like room files). Tokens never enter a room or a shared document.

use serde::{Deserialize, Serialize};

use crate::daemon::{ApiError, Shared};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Account {
    pub token: String,
    pub api_url: String,
    /// The account name the service reported when connecting.
    pub login: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Integrations {
    pub github: Option<Account>,
    pub linear: Option<Account>,
}

const AAD: &[u8] = b"polyph integrations v1";

fn path(shared: &Shared) -> std::path::PathBuf {
    shared.paths.home.join("integrations.sealed")
}

pub fn load(shared: &Shared) -> Integrations {
    let Ok(bytes) = std::fs::read(path(shared)) else { return Integrations::default() };
    shared.key.open(AAD, &bytes).ok().and_then(|plain| serde_json::from_slice(&plain).ok()).unwrap_or_default()
}

pub fn save(shared: &Shared, i: &Integrations) -> Result<(), ApiError> {
    let plain = zeroize::Zeroizing::new(serde_json::to_vec(i).map_err(|e| ApiError::Failed(e.to_string()))?);
    let sealed = shared.key.seal(AAD, &plain)?;
    crate::paths::write_private(&path(shared), &sealed)?;
    Ok(())
}

/// The GitHub account: the connected one, else `GITHUB_TOKEN` from the environment.
pub fn github(shared: &Shared) -> Option<Account> {
    load(shared).github.or_else(|| {
        std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty()).map(|token| Account {
            token,
            api_url: "https://api.github.com".into(),
            login: String::new(),
        })
    })
}

pub fn linear(shared: &Shared) -> Option<Account> {
    load(shared).linear.or_else(|| {
        std::env::var("LINEAR_API_KEY").ok().filter(|t| !t.is_empty()).map(|token| Account {
            token,
            api_url: "https://api.linear.app/graphql".into(),
            login: String::new(),
        })
    })
}

/// Status without secrets.
pub fn status(shared: &Shared) -> serde_json::Value {
    let i = load(shared);
    let show = |a: &Option<Account>| a.as_ref().map(|a| serde_json::json!({ "login": a.login, "api_url": a.api_url }));
    serde_json::json!({
        "github": show(&i.github),
        "linear": show(&i.linear),
        "github_env": i.github.is_none() && std::env::var("GITHUB_TOKEN").is_ok_and(|t| !t.is_empty()),
        "linear_env": i.linear.is_none() && std::env::var("LINEAR_API_KEY").is_ok_and(|t| !t.is_empty()),
    })
}

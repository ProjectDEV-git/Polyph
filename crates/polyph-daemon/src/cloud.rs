//! Signing in to Polyph's hosted service (`polyph login`). The session token
//! is sealed on this device like integration tokens. Signing in links this
//! device to the account: the device signs a one-time challenge with its
//! identity key, so the hosted relay can apply the account's plan to rooms
//! this device owns. Nothing about room contents reaches the service.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::daemon::{ApiError, Shared};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub api: String,
    pub token: String,
    pub account: String,
}

const AAD: &[u8] = b"polyph cloud session v1";

fn path(shared: &Shared) -> std::path::PathBuf {
    shared.paths.home.join("cloud.sealed")
}

pub fn session(shared: &Shared) -> Option<Session> {
    let bytes = std::fs::read(path(shared)).ok()?;
    let plain = shared.key.open(AAD, &bytes).ok()?;
    serde_json::from_slice(&plain).ok()
}

fn save(shared: &Shared, s: &Session) -> Result<(), ApiError> {
    let plain = zeroize::Zeroizing::new(serde_json::to_vec(s).map_err(|e| ApiError::Failed(e.to_string()))?);
    let sealed = shared.key.seal(AAD, &plain)?;
    crate::paths::write_private(&path(shared), &sealed)?;
    Ok(())
}

fn client() -> reqwest::Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder().timeout(std::time::Duration::from_secs(20)).build().unwrap_or_default()
}

fn api_of(shared: &Shared, given: Option<String>) -> String {
    given.unwrap_or_else(|| shared.config.lock().cloud.clone()).trim_end_matches('/').to_string()
}

/// Calls the service. Its error codes come back as `cloud:<code>` so the
/// CLI and apps can show their own text for them.
async fn call(
    api: &str,
    method: reqwest::Method,
    path: &str,
    token: Option<&str>,
    body: Value,
) -> Result<Value, ApiError> {
    let mut r = client().request(method.clone(), format!("{api}{path}"));
    if method != reqwest::Method::GET {
        r = r.json(&body);
    }
    if let Some(t) = token {
        r = r.bearer_auth(t);
    }
    let r = r.send().await.map_err(|e| ApiError::Failed(format!("cannot reach {api}: {e}")))?;
    let status = r.status();
    let v: Value = r.json().await.unwrap_or(Value::Null);
    if status.is_success() {
        return Ok(v);
    }
    let code = v["error"].as_str().unwrap_or("failed");
    let msg = format!("cloud:{code}: {}", v["message"].as_str().unwrap_or("request failed"));
    Err(match status.as_u16() {
        401 | 403 => ApiError::Forbidden(msg),
        404 => ApiError::NotFound(msg),
        400 | 409 | 410 | 429 => ApiError::BadRequest(msg),
        _ => ApiError::Failed(msg),
    })
}

fn lang(shared: &Shared) -> String {
    polyph_i18n::detect(shared.config.lock().lang.as_deref())
}

#[derive(Debug, Deserialize)]
pub struct StartParams {
    /// "email" (default) or "github".
    #[serde(default)]
    pub method: String,
    pub email: Option<String>,
    pub api: Option<String>,
}

pub async fn login_start(shared: &Shared, p: StartParams) -> Result<Value, ApiError> {
    let api = api_of(shared, p.api);
    let mut v = if p.method == "github" {
        call(&api, reqwest::Method::POST, "/v1/auth/github/start", None, json!({ "lang": lang(shared) })).await?
    } else {
        let email = p.email.ok_or_else(|| ApiError::BadRequest("an email address is needed".into()))?;
        call(&api, reqwest::Method::POST, "/v1/auth/email/start", None, json!({ "email": email, "lang": lang(shared) }))
            .await?
    };
    v["api"] = json!(api);
    Ok(v)
}

#[derive(Debug, Deserialize)]
pub struct FinishParams {
    pub api: Option<String>,
    pub pending: String,
    /// The emailed code; absent for GitHub, which is polled.
    pub code: Option<String>,
}

/// Finishes an email sign-in, or polls a GitHub one (`{"status":"pending"}`
/// until the person approves it on GitHub). On success the session is saved
/// and this device is linked.
pub async fn login_finish(shared: &Shared, p: FinishParams) -> Result<Value, ApiError> {
    let api = api_of(shared, p.api);
    let v = match p.code {
        Some(code) => {
            call(
                &api,
                reqwest::Method::POST,
                "/v1/auth/email/finish",
                None,
                json!({ "pending": p.pending, "code": code }),
            )
            .await?
        }
        None => {
            let v = call(&api, reqwest::Method::POST, "/v1/auth/github/poll", None, json!({ "pending": p.pending }))
                .await?;
            if v["status"] == "pending" {
                return Ok(v);
            }
            v
        }
    };
    let (Some(token), Some(account)) = (v["token"].as_str(), v["account"]["id"].as_str()) else {
        return Err(ApiError::Failed("the service did not return a session".into()));
    };
    let s = Session { api: api.clone(), token: token.to_string(), account: account.to_string() };
    link(shared, &s).await?;
    save(shared, &s)?;
    let mut out = status(shared).await?;
    out["status"] = json!("signed_in");
    Ok(out)
}

async fn link(shared: &Shared, s: &Session) -> Result<(), ApiError> {
    let ch = call(&s.api, reqwest::Method::POST, "/v1/devices/challenge", Some(&s.token), json!({})).await?;
    let nonce = ch["nonce"].as_str().unwrap_or_default();
    let msg = format!("{nonce}\n{}", s.account);
    let sig = shared.identity.sign(polyph_proto::identity::ctx::ACCOUNT_LINK, msg.as_bytes());
    let label = shared.config.lock().name.clone().unwrap_or_default();
    call(
        &s.api,
        reqwest::Method::POST,
        "/v1/devices/link",
        Some(&s.token),
        json!({ "device": shared.identity.device_id().to_hex(), "nonce": nonce, "sig": hex::encode(sig.0), "label": label }),
    )
    .await?;
    Ok(())
}

/// The account, plan and devices, or `signed_in: false`.
pub async fn status(shared: &Shared) -> Result<Value, ApiError> {
    let Some(s) = session(shared) else {
        return Ok(json!({ "signed_in": false, "api": api_of(shared, None) }));
    };
    let mut v = call(&s.api, reqwest::Method::GET, "/v1/account", Some(&s.token), json!({})).await?;
    let me = shared.identity.device_id().to_hex();
    let linked = v["devices"].as_array().is_some_and(|d| d.iter().any(|d| d["device"] == me.as_str()));
    v["signed_in"] = json!(true);
    v["api"] = json!(s.api);
    v["this_device"] = json!(me);
    v["this_device_linked"] = json!(linked);
    Ok(v)
}

/// Ends the session on this device. The device stays linked, so rooms it
/// owns keep the account's plan; unlink it from the account page to stop that.
pub async fn logout(shared: &Shared) -> Result<Value, ApiError> {
    if let Some(s) = session(shared) {
        // Best effort: the local session is forgotten even if the service is unreachable.
        let _ = call(&s.api, reqwest::Method::POST, "/v1/auth/logout", Some(&s.token), json!({})).await;
    }
    match std::fs::remove_file(path(shared)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(ApiError::Failed(e.to_string())),
    }
    Ok(json!({}))
}

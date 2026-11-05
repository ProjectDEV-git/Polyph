//! The account API (JSON over HTTPS). The website and the daemon call it.
//!
//! Errors are `{"error": "<code>", "message": "<English detail>"}`; clients
//! show their own localized text for each code.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, Path, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use parking_lot::Mutex;
use polyph_proto::identity::{ctx, verify};
use polyph_proto::ids::{DeviceId, Sig};
use rand::Rng;
use serde::Deserialize;
use serde_json::{Value, json};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tracing::warn;

use crate::billing::{self, Checkout, Stripe};
use crate::config::CloudConfig;
use crate::db::{self, CodeError, Db, Owner};
use crate::mail::{self, Mailer};
use crate::plans::PlanId;
use crate::policy::CloudPolicy;

/// Sign-in codes last ten minutes and allow five wrong tries.
const CODE_TTL: i64 = 600;
const CODE_ATTEMPTS: i64 = 5;
/// Codes one email address may request per hour.
const CODES_PER_HOUR: i64 = 5;
/// Sign-in starts one IP address may make per ten minutes.
const STARTS_PER_IP: u32 = 30;
const CHALLENGE_TTL: i64 = 300;
const INVITE_TTL: i64 = 7 * 86_400;

#[derive(Debug)]
pub struct App {
    pub cfg: CloudConfig,
    pub db: Db,
    pub policy: CloudPolicy,
    pub mailer: Mailer,
    pub stripe: Option<Stripe>,
    pub http: reqwest::Client,
    starts: Mutex<HashMap<IpAddr, (i64, u32)>>,
}

impl App {
    pub fn new(cfg: CloudConfig, db: Db, policy: CloudPolicy, mailer: Mailer) -> Self {
        let stripe = cfg.stripe.clone().map(Stripe::new);
        Self { cfg, db, policy, mailer, stripe, http: crate::http_client(), starts: Mutex::default() }
    }

    async fn refresh(&self) {
        if let Err(e) = self.policy.refresh(&self.db).await {
            warn!(error = %e, "plan refresh failed");
        }
    }
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

#[derive(Debug)]
pub struct ApiErr {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiErr {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self { status, code, message: message.into() }
    }
    fn bad(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "bad_request", message)
    }
    fn unauthorized() -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", "sign in first")
    }
    fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }
    fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }
}

impl From<anyhow::Error> for ApiErr {
    fn from(e: anyhow::Error) -> Self {
        warn!(error = %e, "internal error");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", "internal error")
    }
}

impl IntoResponse for ApiErr {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.code, "message": self.message }))).into_response()
    }
}

type Res = Result<Json<Value>, ApiErr>;
type S = State<Arc<App>>;

pub fn router(app: Arc<App>) -> Router {
    let origins: Vec<HeaderValue> = app.cfg.allowed_origins.iter().filter_map(|o| o.parse().ok()).collect();
    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::POST, Method::DELETE])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);
    let api = Router::new()
        .route("/v1/plans", get(plans))
        .route("/v1/auth/email/start", post(email_start))
        .route("/v1/auth/email/finish", post(email_finish))
        .route("/v1/auth/github/start", post(github_start))
        .route("/v1/auth/github/poll", post(github_poll))
        .route("/v1/auth/logout", post(logout))
        .route("/v1/account", get(account).delete(delete_account))
        .route("/v1/devices/challenge", post(challenge))
        .route("/v1/devices/link", post(link))
        .route("/v1/devices/{device}", delete(unlink))
        .route("/v1/teams", post(create_team))
        .route("/v1/teams/{team}", get(team).delete(delete_team))
        .route("/v1/teams/{team}/invites", post(invite))
        .route("/v1/teams/{team}/members/{account}", delete(remove_member))
        .route("/v1/invites/accept", post(accept_invite))
        .route("/v1/billing/checkout", post(checkout))
        .route("/v1/billing/portal", post(portal))
        .layer(cors);
    Router::new()
        .merge(api)
        // Stripe calls this one directly; no CORS.
        .route("/v1/billing/webhook", post(webhook))
        .route("/healthz", get(|| async { Json(json!({ "ok": true })) }))
        .with_state(app)
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    headers.get(header::AUTHORIZATION)?.to_str().ok()?.strip_prefix("Bearer ").map(str::trim)
}

async fn who(app: &App, headers: &HeaderMap) -> Result<String, ApiErr> {
    let token = bearer(headers).ok_or_else(ApiErr::unauthorized)?;
    app.db.session_account(token, now()).await?.ok_or_else(ApiErr::unauthorized)
}

fn token() -> String {
    let mut b = [0u8; 32];
    rand::rngs::OsRng.fill(&mut b);
    hex::encode(b)
}

fn lang_of(l: Option<String>) -> String {
    polyph_i18n::negotiate(&l.into_iter().collect::<Vec<_>>())
}

fn normalize_email(e: &str) -> Result<String, ApiErr> {
    let e = e.trim().to_lowercase();
    let ok = e.len() <= 254
        && e.split_once('@').is_some_and(|(l, d)| !l.is_empty() && d.contains('.') && !d.starts_with('.'))
        && !e.chars().any(|c| c.is_whitespace() || c.is_control());
    if ok { Ok(e) } else { Err(ApiErr::new(StatusCode::BAD_REQUEST, "invalid_email", "not an email address")) }
}

fn limit_ip(app: &App, ip: IpAddr) -> Result<(), ApiErr> {
    let t = now();
    let mut g = app.starts.lock();
    g.retain(|_, (start, _)| t - *start < 600);
    let e = g.entry(ip).or_insert((t, 0));
    e.1 += 1;
    if e.1 > STARTS_PER_IP {
        return Err(ApiErr::new(StatusCode::TOO_MANY_REQUESTS, "rate_limited", "too many sign-in attempts"));
    }
    Ok(())
}

async fn start_session(app: &App, account: db::Account) -> Res {
    let t = token();
    app.db.create_session(&t, &account.id, now() + app.cfg.session_days as i64 * 86_400).await?;
    Ok(Json(json!({ "token": t, "account": account })))
}

// --- plans ---

async fn plans(State(app): S) -> Res {
    let plans: Vec<Value> = PlanId::ALL
        .iter()
        .filter_map(|id| app.cfg.plans.get(id).map(|p| (id, p)))
        .map(|(id, p)| {
            json!({
                "id": id.as_str(),
                "max_members": p.max_members,
                "retention_days": p.retention_days,
                "spectator_links": p.spectator_links,
                "price": p.price,
                "purchasable": *id != PlanId::Free && p.stripe_price.is_some() && app.stripe.is_some(),
            })
        })
        .collect();
    Ok(Json(json!({ "plans": plans })))
}

// --- email sign-in ---

#[derive(Deserialize)]
struct EmailStart {
    email: String,
    lang: Option<String>,
}

async fn email_start(State(app): S, ConnectInfo(peer): ConnectInfo<SocketAddr>, Json(b): Json<EmailStart>) -> Res {
    limit_ip(&app, peer.ip())?;
    let email = normalize_email(&b.email)?;
    let t = now();
    if app.db.recent_codes(&email, t - 3600).await? >= CODES_PER_HOUR {
        return Err(ApiErr::new(StatusCode::TOO_MANY_REQUESTS, "rate_limited", "too many codes for this address"));
    }
    let lang = lang_of(b.lang);
    let code = format!("{:08}", rand::rngs::OsRng.gen_range(0..100_000_000u32));
    let pending = db::new_id();
    app.db.put_code(&pending, &email, &code, &lang, t, CODE_TTL).await?;
    app.mailer.send(mail::code_mail(&email, &lang, &code)).await.map_err(|e| {
        warn!(error = %e, "sending a sign-in code failed");
        ApiErr::new(StatusCode::BAD_GATEWAY, "mail_failed", "the sign-in email could not be sent")
    })?;
    Ok(Json(json!({ "pending": pending, "expires_in": CODE_TTL })))
}

#[derive(Deserialize)]
struct EmailFinish {
    pending: String,
    code: String,
}

async fn email_finish(State(app): S, Json(b): Json<EmailFinish>) -> Res {
    let code: String = b.code.chars().filter(|c| c.is_ascii_digit()).collect();
    match app.db.take_code(&b.pending, &code, now(), CODE_ATTEMPTS).await? {
        Ok((email, lang)) => {
            let account = app.db.account_by_email(&email, &lang, now()).await?;
            start_session(&app, account).await
        }
        Err(CodeError::Wrong) => Err(ApiErr::new(StatusCode::BAD_REQUEST, "wrong_code", "that code is not right")),
        Err(CodeError::Expired | CodeError::Unknown) => {
            Err(ApiErr::new(StatusCode::GONE, "expired", "this code has expired; ask for a new one"))
        }
    }
}

// --- GitHub sign-in (device flow, so it works from a terminal too) ---

#[derive(Deserialize)]
struct GithubStart {
    lang: Option<String>,
}

async fn github_start(State(app): S, ConnectInfo(peer): ConnectInfo<SocketAddr>, Json(b): Json<GithubStart>) -> Res {
    limit_ip(&app, peer.ip())?;
    let gh = app.cfg.github.as_ref().ok_or_else(|| {
        ApiErr::new(StatusCode::NOT_IMPLEMENTED, "github_not_configured", "GitHub sign-in is not set up here")
    })?;
    let r = app
        .http
        .post(format!("{}/login/device/code", gh.web_url))
        .header(header::ACCEPT, "application/json")
        .form(&[("client_id", gh.client_id.as_str()), ("scope", "read:user")])
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("GitHub device code: {e}"))?;
    let v: Value = r.json().await.map_err(|e| anyhow::anyhow!("GitHub device code: {e}"))?;
    let (Some(device_code), Some(user_code), Some(uri)) =
        (v["device_code"].as_str(), v["user_code"].as_str(), v["verification_uri"].as_str())
    else {
        return Err(anyhow::anyhow!("GitHub device code: unexpected answer").into());
    };
    let pending = db::new_id();
    let ttl = v["expires_in"].as_i64().unwrap_or(900);
    app.db.put_github(&pending, device_code, &lang_of(b.lang), now() + ttl).await?;
    Ok(Json(json!({
        "pending": pending,
        "user_code": user_code,
        "verification_uri": uri,
        "interval": v["interval"].as_i64().unwrap_or(5),
        "expires_in": ttl,
    })))
}

#[derive(Deserialize)]
struct Pending {
    pending: String,
}

async fn github_poll(State(app): S, Json(b): Json<Pending>) -> Res {
    let gh = app.cfg.github.as_ref().ok_or_else(|| {
        ApiErr::new(StatusCode::NOT_IMPLEMENTED, "github_not_configured", "GitHub sign-in is not set up here")
    })?;
    let Some((device_code, lang)) = app.db.github_pending(&b.pending, now()).await? else {
        return Err(ApiErr::new(StatusCode::GONE, "expired", "this sign-in has expired; start again"));
    };
    let r = app
        .http
        .post(format!("{}/login/oauth/access_token", gh.web_url))
        .header(header::ACCEPT, "application/json")
        .form(&[
            ("client_id", gh.client_id.as_str()),
            ("device_code", device_code.as_str()),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("GitHub token: {e}"))?;
    let v: Value = r.json().await.map_err(|e| anyhow::anyhow!("GitHub token: {e}"))?;
    let Some(access) = v["access_token"].as_str() else {
        return match v["error"].as_str() {
            Some("authorization_pending" | "slow_down") => Ok(Json(json!({ "status": "pending" }))),
            Some("access_denied") => {
                app.db.drop_github(&b.pending).await?;
                Err(ApiErr::new(StatusCode::FORBIDDEN, "denied", "the sign-in was declined on GitHub"))
            }
            _ => {
                app.db.drop_github(&b.pending).await?;
                Err(ApiErr::new(StatusCode::GONE, "expired", "this sign-in has expired; start again"))
            }
        };
    };
    // The GitHub token is used once to learn who signed in, then dropped.
    let user: Value = app
        .http
        .get(format!("{}/user", gh.api_url))
        .bearer_auth(access)
        .header(header::USER_AGENT, "polyph-cloud")
        .header(header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("GitHub user: {e}"))?
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("GitHub user: {e}"))?;
    let (Some(id), Some(login)) = (user["id"].as_i64(), user["login"].as_str()) else {
        return Err(anyhow::anyhow!("GitHub user: unexpected answer").into());
    };
    app.db.drop_github(&b.pending).await?;
    let account = app.db.account_by_github(id, login, &lang, now()).await?;
    start_session(&app, account).await
}

async fn logout(State(app): S, headers: HeaderMap) -> Res {
    if let Some(t) = bearer(&headers) {
        app.db.delete_session(t).await?;
    }
    Ok(Json(json!({})))
}

// --- account ---

async fn account(State(app): S, headers: HeaderMap) -> Res {
    let id = who(&app, &headers).await?;
    let Some(account) = app.db.account(&id).await? else { return Err(ApiErr::unauthorized()) };
    let plan = app.db.effective_plan(&id).await?;
    let limits = app.cfg.plans.get(&plan).cloned().unwrap_or_default();
    Ok(Json(json!({
        "account": account,
        "plan": plan.as_str(),
        "limits": { "max_members": limits.max_members, "retention_days": limits.retention_days, "spectator_links": limits.spectator_links },
        "devices": app.db.devices(&id).await?,
        "teams": app.db.teams_of(&id).await?,
        "subscriptions": app.db.subscriptions(&Owner::Account(id.clone())).await?,
        "billing": app.stripe.is_some(),
    })))
}

async fn delete_account(State(app): S, headers: HeaderMap) -> Res {
    let id = who(&app, &headers).await?;
    if app.db.teams_of(&id).await?.iter().any(|t| t.role == "owner") {
        return Err(ApiErr::new(StatusCode::CONFLICT, "owns_team", "delete the teams you own first"));
    }
    if app.db.subscriptions(&Owner::Account(id.clone())).await?.iter().any(|s| db::active(&s.status)) {
        return Err(ApiErr::new(StatusCode::CONFLICT, "subscribed", "cancel the subscription first"));
    }
    app.db.delete_account(&id).await?;
    app.refresh().await;
    Ok(Json(json!({})))
}

// --- devices ---

async fn challenge(State(app): S, headers: HeaderMap) -> Res {
    let id = who(&app, &headers).await?;
    let nonce = token();
    app.db.put_challenge(&nonce, &id, now() + CHALLENGE_TTL).await?;
    Ok(Json(json!({ "nonce": nonce, "account": id })))
}

/// The bytes a device signs (context `account-link`) to join an account.
pub fn link_message(nonce: &str, account: &str) -> Vec<u8> {
    format!("{nonce}\n{account}").into_bytes()
}

#[derive(Deserialize)]
struct Link {
    device: String,
    nonce: String,
    sig: String,
    #[serde(default)]
    label: String,
}

async fn link(State(app): S, headers: HeaderMap, Json(b): Json<Link>) -> Res {
    let id = who(&app, &headers).await?;
    let device = DeviceId::from_hex(&b.device).map_err(|_| ApiErr::bad("device is not a device id"))?;
    let sig = hex::decode(&b.sig).ok().and_then(|s| <[u8; 64]>::try_from(s).ok()).map(Sig);
    let Some(sig) = sig else { return Err(ApiErr::bad("sig is not a signature")) };
    if verify(&device, ctx::ACCOUNT_LINK, &link_message(&b.nonce, &id), &sig).is_err() {
        return Err(ApiErr::forbidden("the device signature did not verify"));
    }
    if !app.db.take_challenge(&b.nonce, &id, now()).await? {
        return Err(ApiErr::new(StatusCode::GONE, "expired", "the challenge expired; try again"));
    }
    let label: String = b.label.chars().filter(|c| !c.is_control()).take(80).collect();
    app.db.link_device(&device, &id, &label, now()).await?;
    app.refresh().await;
    Ok(Json(json!({ "device": device.to_hex(), "plan": app.policy.plan_of(&device).as_str() })))
}

async fn unlink(State(app): S, headers: HeaderMap, Path(device): Path<String>) -> Res {
    let id = who(&app, &headers).await?;
    let device = DeviceId::from_hex(&device).map_err(|_| ApiErr::bad("device is not a device id"))?;
    if !app.db.unlink_device(&device, &id).await? {
        return Err(ApiErr::not_found("no such device on this account"));
    }
    app.refresh().await;
    Ok(Json(json!({})))
}

// --- teams ---

#[derive(Deserialize)]
struct NewTeam {
    name: String,
}

async fn create_team(State(app): S, headers: HeaderMap, Json(b): Json<NewTeam>) -> Res {
    let id = who(&app, &headers).await?;
    let name: String = b.name.trim().chars().filter(|c| !c.is_control()).take(80).collect();
    if name.is_empty() {
        return Err(ApiErr::bad("a team needs a name"));
    }
    let team = app.db.create_team(&name, &id, now()).await?;
    Ok(Json(json!({ "team": { "id": team, "name": name, "role": "owner" } })))
}

async fn role(app: &App, team: &str, account: &str) -> Result<String, ApiErr> {
    app.db.team_role(team, account).await?.ok_or_else(|| ApiErr::not_found("no such team"))
}

async fn team(State(app): S, headers: HeaderMap, Path(team): Path<String>) -> Res {
    let id = who(&app, &headers).await?;
    let my_role = role(&app, &team, &id).await?;
    let subs = app.db.subscriptions(&Owner::Team(team.clone())).await?;
    let plan = subs
        .iter()
        .filter(|s| db::active(&s.status))
        .filter_map(|s| PlanId::parse(&s.plan))
        .max()
        .unwrap_or(PlanId::Free);
    Ok(Json(json!({
        "team": { "id": team, "name": app.db.team_name(&team).await?, "role": my_role },
        "plan": plan.as_str(),
        "members": app.db.team_members(&team).await?,
        "subscriptions": subs,
    })))
}

async fn delete_team(State(app): S, headers: HeaderMap, Path(team): Path<String>) -> Res {
    let id = who(&app, &headers).await?;
    if role(&app, &team, &id).await? != "owner" {
        return Err(ApiErr::forbidden("only the team's owner can delete it"));
    }
    if app.db.subscriptions(&Owner::Team(team.clone())).await?.iter().any(|s| db::active(&s.status)) {
        return Err(ApiErr::new(StatusCode::CONFLICT, "subscribed", "cancel the team's subscription first"));
    }
    for m in app.db.team_members(&team).await? {
        app.db.remove_member(&team, &m.account).await?;
    }
    app.db.delete_team(&team).await?;
    app.refresh().await;
    Ok(Json(json!({})))
}

#[derive(Deserialize)]
struct Invite {
    email: String,
}

async fn invite(State(app): S, headers: HeaderMap, Path(team): Path<String>, Json(b): Json<Invite>) -> Res {
    let id = who(&app, &headers).await?;
    if role(&app, &team, &id).await? != "owner" {
        return Err(ApiErr::forbidden("only the team's owner can invite"));
    }
    let email = normalize_email(&b.email)?;
    let t = token();
    app.db.put_invite(&t, &team, &email, now() + INVITE_TTL).await?;
    let name = app.db.team_name(&team).await?.unwrap_or_default();
    let lang = app.db.account(&id).await?.map(|a| a.lang).unwrap_or_else(|| polyph_i18n::FALLBACK.into());
    let link = format!("{}/account/?invite={t}", app.cfg.site_url.trim_end_matches('/'));
    app.mailer.send(mail::invite_mail(&email, &lang, &name, &link)).await.map_err(|e| {
        warn!(error = %e, "sending an invite failed");
        ApiErr::new(StatusCode::BAD_GATEWAY, "mail_failed", "the invite email could not be sent")
    })?;
    Ok(Json(json!({ "expires_in": INVITE_TTL })))
}

async fn remove_member(State(app): S, headers: HeaderMap, Path((team, account)): Path<(String, String)>) -> Res {
    let id = who(&app, &headers).await?;
    // Owners remove anyone else; members can leave.
    if role(&app, &team, &id).await? != "owner" && account != id {
        return Err(ApiErr::forbidden("only the team's owner can remove members"));
    }
    if !app.db.remove_member(&team, &account).await? {
        return Err(ApiErr::not_found("not a removable member of this team"));
    }
    app.refresh().await;
    Ok(Json(json!({})))
}

#[derive(Deserialize)]
struct Accept {
    token: String,
}

async fn accept_invite(State(app): S, headers: HeaderMap, Json(b): Json<Accept>) -> Res {
    let id = who(&app, &headers).await?;
    let Some(team) = app.db.accept_invite(&b.token, &id, now()).await? else {
        return Err(ApiErr::new(StatusCode::GONE, "expired", "this invite has expired or was used"));
    };
    app.refresh().await;
    Ok(Json(json!({ "team": { "id": team, "name": app.db.team_name(&team).await? } })))
}

// --- billing ---

fn stripe(app: &App) -> Result<&Stripe, ApiErr> {
    app.stripe.as_ref().ok_or_else(|| {
        ApiErr::new(StatusCode::NOT_IMPLEMENTED, "billing_not_configured", "billing is not available yet")
    })
}

/// The owner to bill: a team (owners only) or the account itself.
async fn bill_owner(app: &App, account: &str, team: Option<String>) -> Result<Owner, ApiErr> {
    match team {
        Some(t) => {
            if role(app, &t, account).await? != "owner" {
                return Err(ApiErr::forbidden("only the team's owner manages its billing"));
            }
            Ok(Owner::Team(t))
        }
        None => Ok(Owner::Account(account.to_string())),
    }
}

#[derive(Deserialize)]
struct CheckoutReq {
    plan: String,
    team: Option<String>,
}

async fn checkout(State(app): S, headers: HeaderMap, Json(b): Json<CheckoutReq>) -> Res {
    let id = who(&app, &headers).await?;
    let stripe = stripe(&app)?;
    let plan = PlanId::parse(&b.plan).filter(|p| *p != PlanId::Free).ok_or_else(|| ApiErr::bad("unknown plan"))?;
    if (plan == PlanId::Team) != b.team.is_some() {
        return Err(ApiErr::bad("the Team plan is bought for a team; Pro is bought for yourself"));
    }
    let price = app.cfg.plans.get(&plan).and_then(|p| p.stripe_price.clone()).ok_or_else(|| {
        ApiErr::new(StatusCode::NOT_IMPLEMENTED, "billing_not_configured", "this plan cannot be bought yet")
    })?;
    let owner = bill_owner(&app, &id, b.team).await?;
    let customer = app.db.customer(&owner).await?;
    let email = app.db.account(&id).await?.and_then(|a| a.email);
    let site = app.cfg.site_url.trim_end_matches('/');
    let url = stripe
        .checkout(Checkout {
            owner: &owner,
            plan,
            price: &price,
            customer: customer.as_deref(),
            email: email.as_deref(),
            success_url: &format!("{site}/account/?billing=done"),
            cancel_url: &format!("{site}/account/"),
        })
        .await
        .map_err(|e| {
            warn!(error = %e, "checkout failed");
            ApiErr::new(StatusCode::BAD_GATEWAY, "billing_failed", "the payment page could not be opened")
        })?;
    Ok(Json(json!({ "url": url })))
}

#[derive(Deserialize)]
struct PortalReq {
    team: Option<String>,
}

async fn portal(State(app): S, headers: HeaderMap, Json(b): Json<PortalReq>) -> Res {
    let id = who(&app, &headers).await?;
    let stripe = stripe(&app)?;
    let owner = bill_owner(&app, &id, b.team).await?;
    let Some(customer) = app.db.customer(&owner).await? else {
        return Err(ApiErr::not_found("there is no billing account yet"));
    };
    let url = stripe.portal(&customer, &format!("{}/account/", app.cfg.site_url.trim_end_matches('/'))).await.map_err(
        |e| {
            warn!(error = %e, "portal failed");
            ApiErr::new(StatusCode::BAD_GATEWAY, "billing_failed", "the billing page could not be opened")
        },
    )?;
    Ok(Json(json!({ "url": url })))
}

async fn webhook(State(app): S, headers: HeaderMap, body: axum::body::Bytes) -> Result<StatusCode, ApiErr> {
    let stripe = stripe(&app)?;
    let sig = headers.get("stripe-signature").and_then(|v| v.to_str().ok()).unwrap_or_default();
    if !billing::verify_signature(sig, &body, stripe.webhook_secret(), now()) {
        return Err(ApiErr::new(StatusCode::BAD_REQUEST, "bad_signature", "signature did not verify"));
    }
    let event: Value = serde_json::from_slice(&body).map_err(|_| ApiErr::bad("not JSON"))?;
    if billing::apply_event(&app.db, &event, now()).await? {
        app.refresh().await;
    }
    Ok(StatusCode::OK)
}

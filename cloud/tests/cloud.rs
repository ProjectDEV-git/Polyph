//! The account service end to end: email and GitHub sign-in, device links,
//! teams, Stripe checkout and webhooks, and the relay policy that follows.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Form, Json, Router};
use parking_lot::Mutex;
use polyph_cloud::config::{GithubConfig, StripeConfig};
use polyph_cloud::mail::{Mail, Mailer};
use polyph_cloud::plans::PlanId;
use polyph_cloud::{CloudConfig, CloudHandle, CloudOptions, billing, serve};
use polyph_proto::identity::{DeviceIdentity, ctx};
use polyph_relay::Policy;
use serde_json::{Value, json};

const WHSEC: &str = "whsec_test";

/// Form posts the stand-in received, in order.
type Forms = Arc<Mutex<Vec<Vec<(String, String)>>>>;

struct Env {
    cloud: CloudHandle,
    mail: Arc<Mutex<Vec<Mail>>>,
    stripe_forms: Forms,
    http: reqwest::Client,
}

/// A stand-in for the parts of Stripe and GitHub the service calls.
async fn stand_in(forms: Forms) -> String {
    let app = Router::new()
        .route(
            "/v1/checkout/sessions",
            post(|State(f): State<Forms>, Form(form): Form<Vec<(String, String)>>| async move {
                f.lock().push(form);
                Json(json!({ "id": "cs_1", "url": "https://checkout.example/cs_1" }))
            }),
        )
        .route(
            "/v1/billing_portal/sessions",
            post(|Form(form): Form<Vec<(String, String)>>| async move {
                let cus = form.iter().find(|(k, _)| k == "customer").map(|(_, v)| v.clone()).unwrap_or_default();
                Json(json!({ "url": format!("https://portal.example/{cus}") }))
            }),
        )
        .route(
            "/login/device/code",
            post(|| async {
                Json(json!({ "device_code": "dc1", "user_code": "ABCD-1234", "verification_uri": "https://github.example/login/device", "interval": 1, "expires_in": 900 }))
            }),
        )
        .route(
            "/login/oauth/access_token",
            post(|State(f): State<Forms>| async move {
                // First poll: pending; afterwards: approved.
                let mut g = f.lock();
                g.push(vec![("github".into(), "poll".into())]);
                let polls = g.iter().filter(|v| v.first().is_some_and(|(k, _)| k == "github")).count();
                if polls < 2 { Json(json!({ "error": "authorization_pending" })) } else { Json(json!({ "access_token": "gho_x" })) }
            }),
        )
        .route("/user", get(|| async { Json(json!({ "id": 4242, "login": "octo" })) }))
        .with_state(forms);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

async fn env() -> Env {
    let mail = Arc::new(Mutex::new(vec![]));
    let stripe_forms = Arc::new(Mutex::new(vec![]));
    let base = stand_in(stripe_forms.clone()).await;
    let mut cfg = CloudConfig::ephemeral();
    cfg.stripe =
        Some(StripeConfig { secret_key: "sk_test".into(), webhook_secret: WHSEC.into(), api_url: base.clone() });
    cfg.github = Some(GithubConfig { client_id: "Iv1".into(), web_url: base.clone(), api_url: base });
    cfg.plans.get_mut(&PlanId::Free).unwrap().max_members = Some(3);
    cfg.plans.get_mut(&PlanId::Pro).unwrap().stripe_price = Some("price_pro".into());
    cfg.plans.get_mut(&PlanId::Team).unwrap().stripe_price = Some("price_team".into());
    cfg.site_url = "https://site.example".into();
    let cloud = serve(cfg, CloudOptions { mailer: Some(Mailer::Memory(mail.clone())) }).await.unwrap();
    Env { cloud, mail, stripe_forms, http: reqwest::Client::new() }
}

impl Env {
    async fn call(&self, method: &str, path: &str, token: Option<&str>, body: Value) -> (u16, Value) {
        let url = format!("{}{path}", self.cloud.api_url());
        let mut r = match method {
            "GET" => self.http.get(url),
            "DELETE" => self.http.delete(url),
            _ => self.http.post(url).json(&body),
        };
        if let Some(t) = token {
            r = r.bearer_auth(t);
        }
        let r = r.send().await.unwrap();
        let s = r.status().as_u16();
        (s, r.json().await.unwrap_or(Value::Null))
    }

    fn last_mail_to(&self, to: &str) -> Mail {
        self.mail.lock().iter().rev().find(|m| m.to == to).cloned().unwrap()
    }

    async fn sign_in(&self, email: &str) -> String {
        let (s, v) = self.call("POST", "/v1/auth/email/start", None, json!({ "email": email, "lang": "en" })).await;
        assert_eq!(s, 200, "{v}");
        let text = self.last_mail_to(&email.to_lowercase()).text;
        let code: String = text
            .split_whitespace()
            .find(|w| w.trim_end_matches('.').len() == 8 && w.trim_end_matches('.').chars().all(|c| c.is_ascii_digit()))
            .unwrap()
            .trim_end_matches('.')
            .into();
        let (s, v) =
            self.call("POST", "/v1/auth/email/finish", None, json!({ "pending": v["pending"], "code": code })).await;
        assert_eq!(s, 200, "{v}");
        v["token"].as_str().unwrap().to_string()
    }

    async fn link(&self, token: &str) -> DeviceIdentity {
        let id = DeviceIdentity::generate().unwrap();
        let (_, ch) = self.call("POST", "/v1/devices/challenge", Some(token), json!({})).await;
        let nonce = ch["nonce"].as_str().unwrap();
        let msg = polyph_cloud::api::link_message(nonce, ch["account"].as_str().unwrap());
        let sig = id.sign(ctx::ACCOUNT_LINK, &msg);
        let (s, v) = self
            .call("POST", "/v1/devices/link", Some(token), json!({ "device": id.device_id().to_hex(), "nonce": nonce, "sig": hex::encode(sig.0), "label": "laptop" }))
            .await;
        assert_eq!(s, 200, "{v}");
        id
    }

    async fn webhook(&self, event: Value, secret: &str) -> u16 {
        let body = serde_json::to_vec(&event).unwrap();
        let t = polyph_cloud::api::now();
        self.http
            .post(format!("{}/v1/billing/webhook", self.cloud.api_url()))
            .header("stripe-signature", billing::sign(&body, secret, t))
            .body(body)
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    }

    fn policy_retention(&self, id: &DeviceIdentity) -> Option<u32> {
        self.cloud.app.policy.limits_for_room(Some(&id.device_id())).retention_days
    }
}

fn sub_event(id: &str, sub: &str, owner: &str, plan: &str, status: &str, kind: &str) -> Value {
    json!({ "id": id, "type": kind, "data": { "object": {
        "id": sub, "status": status, "customer": "cus_1", "metadata": { "owner": owner, "plan": plan } } } })
}

#[tokio::test]
async fn email_sign_in_codes_are_single_use_and_limited() {
    let e = env().await;
    let (_, v) =
        e.call("POST", "/v1/auth/email/start", None, json!({ "email": " Ann@Example.com ", "lang": "th" })).await;
    let mail = e.last_mail_to("ann@example.com");
    assert!(mail.subject.contains("Polyph"));
    assert!(!mail.subject.contains("sign-in"), "Thai mail expected: {}", mail.subject);
    let (s, w) =
        e.call("POST", "/v1/auth/email/finish", None, json!({ "pending": v["pending"], "code": "00000000" })).await;
    assert_eq!((s, w["error"].as_str()), (400, Some("wrong_code")));
    // Five wrong tries kill the code.
    for _ in 0..5 {
        e.call("POST", "/v1/auth/email/finish", None, json!({ "pending": v["pending"], "code": "00000001" })).await;
    }
    let code: String = mail.text.chars().filter(|c| c.is_ascii_digit()).take(8).collect();
    let (s, w) = e.call("POST", "/v1/auth/email/finish", None, json!({ "pending": v["pending"], "code": code })).await;
    assert_eq!((s, w["error"].as_str()), (410, Some("expired")));

    let (s, _) = e.call("POST", "/v1/auth/email/start", None, json!({ "email": "not-an-email" })).await;
    assert_eq!(s, 400);
    // The same account comes back for the same address, and sign-out ends the session.
    let t1 = e.sign_in("bo@example.com").await;
    let t2 = e.sign_in("BO@example.com").await;
    let (_, a1) = e.call("GET", "/v1/account", Some(&t1), json!({})).await;
    let (_, a2) = e.call("GET", "/v1/account", Some(&t2), json!({})).await;
    assert_eq!(a1["account"]["id"], a2["account"]["id"]);
    assert_eq!(a1["plan"], "free");
    assert_eq!(a1["limits"]["retention_days"], 7);
    e.call("POST", "/v1/auth/logout", Some(&t1), json!({})).await;
    let (s, _) = e.call("GET", "/v1/account", Some(&t1), json!({})).await;
    assert_eq!(s, 401);
    // Codes per address are limited per hour (two used above plus three more).
    for _ in 0..3 {
        e.call("POST", "/v1/auth/email/start", None, json!({ "email": "bo@example.com" })).await;
    }
    let (s, w) = e.call("POST", "/v1/auth/email/start", None, json!({ "email": "bo@example.com" })).await;
    assert_eq!((s, w["error"].as_str()), (429, Some("rate_limited")));
    e.cloud.shutdown().await;
}

#[tokio::test]
async fn devices_link_only_with_their_own_signature() {
    let e = env().await;
    let t = e.sign_in("cy@example.com").await;
    let dev = e.link(&t).await;
    let (_, a) = e.call("GET", "/v1/account", Some(&t), json!({})).await;
    assert_eq!(a["devices"][0]["device"], dev.device_id().to_hex());
    assert_eq!(a["devices"][0]["label"], "laptop");

    // A signature from another key, or for another account, fails; a nonce works once.
    let other = DeviceIdentity::generate().unwrap();
    let (_, ch) = e.call("POST", "/v1/devices/challenge", Some(&t), json!({})).await;
    let nonce = ch["nonce"].as_str().unwrap();
    let sig = other.sign(ctx::ACCOUNT_LINK, &polyph_cloud::api::link_message(nonce, "someone-else"));
    let body = json!({ "device": other.device_id().to_hex(), "nonce": nonce, "sig": hex::encode(sig.0) });
    let (s, _) = e.call("POST", "/v1/devices/link", Some(&t), body).await;
    assert_eq!(s, 403);
    let sig = other.sign(ctx::RELAY_AUTH, &polyph_cloud::api::link_message(nonce, ch["account"].as_str().unwrap()));
    let body = json!({ "device": other.device_id().to_hex(), "nonce": nonce, "sig": hex::encode(sig.0) });
    let (s, _) = e.call("POST", "/v1/devices/link", Some(&t), body).await;
    assert_eq!(s, 403, "a signature made for another purpose must not link");
    let sig = other.sign(ctx::ACCOUNT_LINK, &polyph_cloud::api::link_message(nonce, ch["account"].as_str().unwrap()));
    let body = json!({ "device": other.device_id().to_hex(), "nonce": nonce, "sig": hex::encode(sig.0) });
    assert_eq!(e.call("POST", "/v1/devices/link", Some(&t), body.clone()).await.0, 200);
    assert_eq!(e.call("POST", "/v1/devices/link", Some(&t), body).await.0, 410);

    let (s, _) = e.call("DELETE", &format!("/v1/devices/{}", dev.device_id().to_hex()), Some(&t), json!({})).await;
    assert_eq!(s, 200);
    let (_, a) = e.call("GET", "/v1/account", Some(&t), json!({})).await;
    assert_eq!(a["devices"].as_array().unwrap().len(), 1);
    e.cloud.shutdown().await;
}

#[tokio::test]
async fn pro_subscription_follows_signed_webhooks_into_the_relay_policy() {
    let e = env().await;
    let t = e.sign_in("di@example.com").await;
    let dev = e.link(&t).await;
    let stranger = DeviceIdentity::generate().unwrap();
    assert_eq!(e.policy_retention(&dev), Some(7));
    assert_eq!(e.cloud.app.policy.limits_for_room(Some(&stranger.device_id())).max_members, Some(3));

    let (_, plans) = e.call("GET", "/v1/plans", None, json!({})).await;
    let pro = plans["plans"].as_array().unwrap().iter().find(|p| p["id"] == "pro").unwrap().clone();
    assert_eq!(pro["price"], Value::Null, "prices are not invented");
    assert_eq!(pro["purchasable"], true);

    let (s, v) = e.call("POST", "/v1/billing/checkout", Some(&t), json!({ "plan": "pro" })).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["url"], "https://checkout.example/cs_1");
    let (_, a) = e.call("GET", "/v1/account", Some(&t), json!({})).await;
    let owner = format!("account:{}", a["account"]["id"].as_str().unwrap());
    let form = e.stripe_forms.lock()[0].clone();
    let get = |k: &str| form.iter().find(|(f, _)| f == k).map(|(_, v)| v.clone());
    assert_eq!(get("line_items[0][price]").as_deref(), Some("price_pro"));
    assert_eq!(get("subscription_data[metadata][owner]"), Some(owner.clone()));
    assert_eq!(get("customer_email").as_deref(), Some("di@example.com"));
    // Team is bought for a team, not a person.
    let (s, _) = e.call("POST", "/v1/billing/checkout", Some(&t), json!({ "plan": "team" })).await;
    assert_eq!(s, 400);

    let ev = sub_event("evt_1", "sub_1", &owner, "pro", "active", "customer.subscription.created");
    assert_eq!(e.webhook(ev.clone(), "whsec_wrong").await, 400);
    assert_eq!(e.policy_retention(&dev), Some(7));
    assert_eq!(e.webhook(ev, WHSEC).await, 200);
    assert_eq!(e.policy_retention(&dev), Some(365));
    let (_, a) = e.call("GET", "/v1/account", Some(&t), json!({})).await;
    assert_eq!(a["plan"], "pro");

    let (_, v) = e.call("POST", "/v1/billing/portal", Some(&t), json!({})).await;
    assert_eq!(v["url"], "https://portal.example/cus_1");

    let (s, _) = e.call("DELETE", "/v1/account", Some(&t), json!({})).await;
    assert_eq!(s, 409, "an active subscription blocks deleting the account");
    let ev = sub_event("evt_2", "sub_1", &owner, "pro", "canceled", "customer.subscription.deleted");
    assert_eq!(e.webhook(ev, WHSEC).await, 200);
    assert_eq!(e.policy_retention(&dev), Some(7));
    // A replayed event is ignored.
    let ev = sub_event("evt_1", "sub_1", &owner, "pro", "active", "customer.subscription.created");
    assert_eq!(e.webhook(ev, WHSEC).await, 200);
    assert_eq!(e.policy_retention(&dev), Some(7));

    assert_eq!(e.call("DELETE", "/v1/account", Some(&t), json!({})).await.0, 200);
    assert_eq!(e.call("GET", "/v1/account", Some(&t), json!({})).await.0, 401);
    e.cloud.shutdown().await;
}

#[tokio::test]
async fn team_plan_covers_members_until_they_leave() {
    let e = env().await;
    let owner = e.sign_in("ed@example.com").await;
    let (_, v) = e.call("POST", "/v1/teams", Some(&owner), json!({ "name": "Payments" })).await;
    let team = v["team"]["id"].as_str().unwrap().to_string();

    let member = e.sign_in("fa@example.com").await;
    let member_dev = e.link(&member).await;
    // Only the owner invites.
    let (s, _) =
        e.call("POST", &format!("/v1/teams/{team}/invites"), Some(&member), json!({ "email": "x@example.com" })).await;
    assert_eq!(s, 404);
    let (s, _) =
        e.call("POST", &format!("/v1/teams/{team}/invites"), Some(&owner), json!({ "email": "fa@example.com" })).await;
    assert_eq!(s, 200);
    let mail = e.last_mail_to("fa@example.com");
    assert!(mail.subject.contains("Payments"));
    let link = mail.text.split_whitespace().find(|w| w.starts_with("https://site.example/account/?invite=")).unwrap();
    let invite = link.rsplit('=').next().unwrap();
    let (s, v) = e.call("POST", "/v1/invites/accept", Some(&member), json!({ "token": invite })).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(e.call("POST", "/v1/invites/accept", Some(&member), json!({ "token": invite })).await.0, 410);

    let (_, v) = e.call("GET", &format!("/v1/teams/{team}"), Some(&member), json!({})).await;
    assert_eq!(v["members"].as_array().unwrap().len(), 2);
    assert_eq!(v["plan"], "free");

    // Only the owner buys for the team.
    let (s, _) = e.call("POST", "/v1/billing/checkout", Some(&member), json!({ "plan": "team", "team": team })).await;
    assert_eq!(s, 403);
    let (s, _) = e.call("POST", "/v1/billing/checkout", Some(&owner), json!({ "plan": "team", "team": team })).await;
    assert_eq!(s, 200);
    let ev = sub_event("evt_t", "sub_t", &format!("team:{team}"), "team", "active", "customer.subscription.created");
    assert_eq!(e.webhook(ev, WHSEC).await, 200);
    let (_, a) = e.call("GET", "/v1/account", Some(&member), json!({})).await;
    assert_eq!(a["plan"], "team");
    assert_eq!(e.policy_retention(&member_dev), Some(365));

    let (_, a) = e.call("GET", "/v1/account", Some(&member), json!({})).await;
    let member_id = a["account"]["id"].as_str().unwrap().to_string();
    let (s, _) = e.call("DELETE", &format!("/v1/teams/{team}/members/{member_id}"), Some(&member), json!({})).await;
    assert_eq!(s, 200, "members can leave");
    assert_eq!(e.policy_retention(&member_dev), Some(7));
    assert_eq!(e.call("DELETE", &format!("/v1/teams/{team}"), Some(&owner), json!({})).await.0, 409);
    e.cloud.shutdown().await;
}

#[tokio::test]
async fn github_sign_in_uses_the_device_flow() {
    let e = env().await;
    let (s, v) = e.call("POST", "/v1/auth/github/start", None, json!({ "lang": "ja" })).await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["user_code"], "ABCD-1234");
    let pending = v["pending"].clone();
    let (_, p) = e.call("POST", "/v1/auth/github/poll", None, json!({ "pending": pending })).await;
    assert_eq!(p["status"], "pending");
    let (s, p) = e.call("POST", "/v1/auth/github/poll", None, json!({ "pending": pending })).await;
    assert_eq!(s, 200, "{p}");
    assert_eq!(p["account"]["github_login"], "octo");
    assert_eq!(p["account"]["lang"], "ja");
    let t = p["token"].as_str().unwrap();
    assert_eq!(e.call("GET", "/v1/account", Some(t), json!({})).await.0, 200);
    // The pending sign-in is gone once used.
    assert_eq!(e.call("POST", "/v1/auth/github/poll", None, json!({ "pending": pending })).await.0, 410);
    e.cloud.shutdown().await;
}

#[tokio::test]
async fn billing_is_refused_cleanly_when_not_configured() {
    let mail = Arc::new(Mutex::new(vec![]));
    let cloud =
        serve(CloudConfig::ephemeral(), CloudOptions { mailer: Some(Mailer::Memory(mail.clone())) }).await.unwrap();
    let e = Env { cloud, mail, stripe_forms: Arc::default(), http: reqwest::Client::new() };
    let t = e.sign_in("gu@example.com").await;
    let (s, v) = e.call("POST", "/v1/billing/checkout", Some(&t), json!({ "plan": "pro" })).await;
    assert_eq!((s, v["error"].as_str()), (501, Some("billing_not_configured")));
    let (_, p) = e.call("GET", "/v1/plans", None, json!({})).await;
    assert!(p["plans"].as_array().unwrap().iter().all(|p| p["purchasable"] == false && p["price"].is_null()));
    e.cloud.shutdown().await;
}

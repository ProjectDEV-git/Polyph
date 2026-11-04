//! A real daemon signs in (`polyph login`'s daemon calls), links its device
//! by signature, and the hosted relay's policy follows the account's plan.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use parking_lot::Mutex;
use polyph_cloud::mail::Mailer;
use polyph_cloud::plans::PlanId;
use polyph_cloud::{CloudConfig, CloudOptions, billing, serve};
use polyph_relay::Policy;
use polyph_testkit::harness::Harness;
use serde_json::json;

#[tokio::test]
async fn daemon_signs_in_links_its_device_and_gets_the_plan() {
    let mail = Arc::new(Mutex::new(vec![]));
    let mut cfg = CloudConfig::ephemeral();
    cfg.stripe = Some(polyph_cloud::config::StripeConfig {
        secret_key: "sk".into(),
        webhook_secret: "whsec".into(),
        api_url: "http://127.0.0.1:9".into(),
    });
    cfg.plans.get_mut(&PlanId::Free).unwrap().max_members = Some(2);
    let cloud = serve(cfg, CloudOptions { mailer: Some(Mailer::Memory(mail.clone())) }).await.unwrap();

    let mut h = Harness::new().await.unwrap();
    h.add("ann").await.unwrap();
    let ann = &h.nodes[0];
    ann.call("config.set", json!({ "key": "cloud", "value": cloud.api_url() })).await.unwrap();
    let out = ann.call("cloud.status", json!({})).await.unwrap();
    assert_eq!(out["signed_in"], false);

    let start = ann.call("cloud.login.start", json!({ "email": "ann@example.com" })).await.unwrap();
    let text = mail.lock().last().unwrap().text.clone();
    let code: String = text.chars().filter(|c| c.is_ascii_digit()).take(8).collect();
    let err =
        ann.call("cloud.login.finish", json!({ "pending": start["pending"], "code": "11111111" })).await.unwrap_err();
    assert!(err.to_string().contains("cloud:wrong_code"), "{err}");
    let done = ann.call("cloud.login.finish", json!({ "pending": start["pending"], "code": code })).await.unwrap();
    assert_eq!(done["status"], "signed_in");
    assert_eq!(done["this_device_linked"], true);
    assert_eq!(done["devices"][0]["label"], "ann");

    // The token is sealed on disk, never stored in plain text.
    let home = ann.daemon.shared.paths.home.clone();
    let sealed = std::fs::read(home.join("cloud.sealed")).unwrap();
    let token_part = done["account"]["id"].as_str().unwrap();
    assert!(!String::from_utf8_lossy(&sealed).contains(token_part));

    let device = ann.daemon.shared.identity.device_id();
    let policy = &cloud.app.policy;
    assert_eq!(policy.limits_for_room(Some(&device)).max_members, Some(2));
    let owner = format!("account:{}", done["account"]["id"].as_str().unwrap());
    let ev = json!({ "id": "evt_1", "type": "customer.subscription.created", "data": { "object": {
        "id": "sub_1", "status": "active", "customer": "cus_1", "metadata": { "owner": owner, "plan": "pro" } } } });
    let body = serde_json::to_vec(&ev).unwrap();
    let r = reqwest::Client::new()
        .post(format!("{}/v1/billing/webhook", cloud.api_url()))
        .header("stripe-signature", billing::sign(&body, "whsec", polyph_cloud::api::now()))
        .body(body)
        .send()
        .await
        .unwrap();
    assert!(r.status().is_success());
    assert_eq!(policy.limits_for_room(Some(&device)).max_members, None);
    assert_eq!(policy.limits_for_room(Some(&device)).retention_days, Some(365));
    assert_eq!(ann.call("cloud.status", json!({})).await.unwrap()["plan"], "pro");

    // Signing out forgets the session here; the device stays linked.
    ann.call("cloud.logout", json!({})).await.unwrap();
    assert_eq!(ann.call("cloud.status", json!({})).await.unwrap()["signed_in"], false);
    assert!(!home.join("cloud.sealed").exists());
    assert_eq!(policy.limits_for_room(Some(&device)).retention_days, Some(365));
    cloud.shutdown().await;
}

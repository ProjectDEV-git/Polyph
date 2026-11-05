//! Billing through Stripe: Checkout for new subscriptions, the customer
//! portal for changes and cancellation, and signed webhooks that keep
//! subscriptions here in step. Polyph never sees card details.
//!
//! Who a subscription pays for travels in its metadata (`owner` is
//! `account:<id>` or `team:<id>`, `plan` is the plan id), so events can be
//! applied in any order.

use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;

use crate::config::StripeConfig;
use crate::db::{Db, Owner};
use crate::plans::PlanId;

/// How old a webhook signature may be.
pub const TOLERANCE_SECS: i64 = 300;

#[derive(Debug, thiserror::Error)]
pub enum BillingError {
    #[error("Stripe answered {0}: {1}")]
    Stripe(u16, String),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
}

#[derive(Debug, Clone)]
pub struct Stripe {
    cfg: StripeConfig,
    client: reqwest::Client,
}

#[derive(Debug)]
pub struct Checkout<'a> {
    pub owner: &'a Owner,
    pub plan: PlanId,
    pub price: &'a str,
    pub customer: Option<&'a str>,
    pub email: Option<&'a str>,
    pub success_url: &'a str,
    pub cancel_url: &'a str,
}

impl Stripe {
    pub fn new(cfg: StripeConfig) -> Self {
        Self { cfg, client: crate::http_client() }
    }

    pub fn webhook_secret(&self) -> &str {
        &self.cfg.webhook_secret
    }

    async fn post(&self, path: &str, form: &[(String, String)]) -> Result<Value, BillingError> {
        let r = self
            .client
            .post(format!("{}{path}", self.cfg.api_url.trim_end_matches('/')))
            .basic_auth(&self.cfg.secret_key, Option::<&str>::None)
            .form(form)
            .send()
            .await?;
        let status = r.status();
        let v: Value = r.json().await.unwrap_or(Value::Null);
        if !status.is_success() {
            let msg = v["error"]["message"].as_str().unwrap_or("request failed").to_string();
            return Err(BillingError::Stripe(status.as_u16(), msg));
        }
        Ok(v)
    }

    /// Starts Checkout and returns the page to send the person to.
    pub async fn checkout(&self, c: Checkout<'_>) -> Result<String, BillingError> {
        let tag = c.owner.tag();
        let mut form: Vec<(String, String)> = vec![
            ("mode".into(), "subscription".into()),
            ("line_items[0][price]".into(), c.price.into()),
            ("line_items[0][quantity]".into(), "1".into()),
            ("success_url".into(), c.success_url.into()),
            ("cancel_url".into(), c.cancel_url.into()),
            ("client_reference_id".into(), tag.clone()),
            ("metadata[owner]".into(), tag.clone()),
            ("metadata[plan]".into(), c.plan.as_str().into()),
            ("subscription_data[metadata][owner]".into(), tag),
            ("subscription_data[metadata][plan]".into(), c.plan.as_str().into()),
        ];
        match (c.customer, c.email) {
            (Some(cus), _) => form.push(("customer".into(), cus.into())),
            (None, Some(e)) => form.push(("customer_email".into(), e.into())),
            (None, None) => {}
        }
        let v = self.post("/v1/checkout/sessions", &form).await?;
        v["url"].as_str().map(str::to_string).ok_or_else(|| BillingError::Stripe(200, "no checkout url".into()))
    }

    /// Opens the customer portal (change plan, card, cancel).
    pub async fn portal(&self, customer: &str, return_url: &str) -> Result<String, BillingError> {
        let form = vec![("customer".to_string(), customer.to_string()), ("return_url".into(), return_url.into())];
        let v = self.post("/v1/billing_portal/sessions", &form).await?;
        v["url"].as_str().map(str::to_string).ok_or_else(|| BillingError::Stripe(200, "no portal url".into()))
    }
}

/// Checks a `Stripe-Signature` header (`t=<unix>,v1=<hex hmac>`, possibly
/// several `v1`) against the raw body.
pub fn verify_signature(header: &str, body: &[u8], secret: &str, now: i64) -> bool {
    let mut t = None;
    let mut sigs = vec![];
    for part in header.split(',') {
        match part.trim().split_once('=') {
            Some(("t", v)) => t = v.parse::<i64>().ok(),
            Some(("v1", v)) => sigs.extend(hex::decode(v).ok()),
            _ => {}
        }
    }
    let Some(t) = t else { return false };
    if (now - t).abs() > TOLERANCE_SECS {
        return false;
    }
    sigs.iter().any(|sig| {
        let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else { return false };
        mac.update(t.to_string().as_bytes());
        mac.update(b".");
        mac.update(body);
        mac.verify_slice(sig).is_ok()
    })
}

/// Builds a signature header (tests and local stand-ins).
pub fn sign(body: &[u8], secret: &str, t: i64) -> String {
    let mut mac = match Hmac::<Sha256>::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return String::new(),
    };
    mac.update(t.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    format!("t={t},v1={}", hex::encode(mac.finalize().into_bytes()))
}

/// Applies one verified event. Returns whether plans may have changed.
pub async fn apply_event(db: &Db, event: &Value, now: i64) -> anyhow::Result<bool> {
    let Some(id) = event["id"].as_str() else { anyhow::bail!("event without id") };
    if !db.first_event(id, now).await? {
        return Ok(false);
    }
    let obj = &event["data"]["object"];
    let owner = obj["metadata"]["owner"].as_str().and_then(Owner::parse_tag);
    match event["type"].as_str().unwrap_or_default() {
        "checkout.session.completed" => {
            if let (Some(owner), Some(customer)) = (&owner, obj["customer"].as_str()) {
                db.set_customer(owner, customer).await?;
            }
            Ok(false)
        }
        "customer.subscription.created" | "customer.subscription.updated" | "customer.subscription.deleted" => {
            let (Some(owner), Some(sub)) = (owner, obj["id"].as_str()) else { return Ok(false) };
            let Some(plan) = obj["metadata"]["plan"].as_str().and_then(PlanId::parse) else { return Ok(false) };
            let status = obj["status"].as_str().unwrap_or("canceled");
            if let Some(customer) = obj["customer"].as_str() {
                db.set_customer(&owner, customer).await?;
            }
            db.put_subscription(sub, &owner, plan, status, now).await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_check_secret_body_and_age() {
        let body = br#"{"id":"evt_1"}"#;
        let h = sign(body, "whsec_test", 1_000);
        assert!(verify_signature(&h, body, "whsec_test", 1_100));
        assert!(!verify_signature(&h, body, "whsec_other", 1_100));
        assert!(!verify_signature(&h, br#"{"id":"evt_2"}"#, "whsec_test", 1_100));
        assert!(!verify_signature(&h, body, "whsec_test", 1_000 + TOLERANCE_SECS + 1));
        assert!(!verify_signature("v1=00", body, "whsec_test", 1_000));
        // A rotated secret: one of several v1 signatures matches.
        let two = format!("{},v1={}", h, "ab".repeat(32));
        assert!(verify_signature(&two, body, "whsec_test", 1_000));
    }
}

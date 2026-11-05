//! `polyph login`, `polyph logout` and `polyph cloud`: the optional hosted
//! account. Texts in `locales/*/account.ftl`.

use std::io::Write;
use std::time::Duration;

use clap::Subcommand;
use polyph_i18n::tr;
use serde_json::{Value, json};

use super::{Api, Ctx};
use crate::client::CallError;

#[derive(Subcommand, Debug)]
pub enum CloudCmd {
    Status,
}

/// Calls the daemon; errors from the service (`cloud:<code>: ...`) are
/// shown in the person's language when a text exists for the code.
async fn call(ctx: &Ctx, api: &mut Api, method: &str, params: Value) -> anyhow::Result<Value> {
    api.call(method, params).await.map_err(|e| {
        let m = match e {
            CallError::Api(m) | CallError::Unreachable(m) => m,
        };
        let key = m.strip_prefix("cloud:").and_then(|r| r.split(':').next()).map(|c| format!("cloud-error-{c}"));
        match key {
            Some(k) if ctx.l.has(&k) => anyhow::anyhow!(ctx.l.t(&k)),
            _ => anyhow::anyhow!(m),
        }
    })
}

fn ask(prompt: &str) -> anyhow::Result<String> {
    eprint!("{prompt} ");
    std::io::stderr().flush()?;
    let mut s = String::new();
    std::io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

fn plan_name(ctx: &Ctx, plan: &str) -> String {
    let key = format!("cloud-plan-{plan}");
    if ctx.l.has(&key) { ctx.l.t(&key) } else { plan.to_string() }
}

fn who(v: &Value) -> String {
    let a = &v["account"];
    a["email"].as_str().or(a["github_login"].as_str()).unwrap_or_default().to_string()
}

fn signed_in(ctx: &Ctx, v: &Value) {
    if ctx.json {
        ctx.print_json(v);
        return;
    }
    let l = &ctx.l;
    ctx.out(tr!(l, "login-done", who = who(v), plan = plan_name(ctx, v["plan"].as_str().unwrap_or("free"))));
    ctx.out(l.t("login-device-linked"));
}

pub async fn login(
    ctx: &Ctx,
    api: &mut Api,
    email: Option<String>,
    github: bool,
    server: Option<String>,
) -> anyhow::Result<()> {
    let l = &ctx.l;
    if github {
        let v = call(ctx, api, "cloud.login.start", json!({ "method": "github", "api": server })).await?;
        let server = v["api"].clone();
        eprintln!(
            "{}",
            tr!(
                l,
                "login-github-open",
                url = v["verification_uri"].as_str().unwrap_or_default(),
                code = v["user_code"].as_str().unwrap_or_default()
            )
        );
        let every = Duration::from_secs(v["interval"].as_u64().unwrap_or(5).max(1));
        let deadline = std::time::Instant::now() + Duration::from_secs(v["expires_in"].as_u64().unwrap_or(900));
        while std::time::Instant::now() < deadline {
            tokio::time::sleep(every).await;
            let r = call(ctx, api, "cloud.login.finish", json!({ "api": server, "pending": v["pending"] })).await?;
            if r["status"] != "pending" {
                signed_in(ctx, &r);
                return Ok(());
            }
        }
        anyhow::bail!(l.t("cloud-error-expired"));
    }
    let email = match email {
        Some(e) => e,
        None => ask(&l.t("login-email-prompt"))?,
    };
    let v = call(ctx, api, "cloud.login.start", json!({ "method": "email", "email": email, "api": server })).await?;
    let mut last = None;
    for _ in 0..5 {
        let code = ask(&tr!(l, "login-code-prompt", email = email.trim()))?;
        match call(ctx, api, "cloud.login.finish", json!({ "api": v["api"], "pending": v["pending"], "code": code }))
            .await
        {
            Ok(r) => {
                signed_in(ctx, &r);
                return Ok(());
            }
            Err(e) => {
                let wrong = e.to_string() == l.t("cloud-error-wrong_code");
                eprintln!("{e}");
                last = Some(e);
                if !wrong {
                    break;
                }
            }
        }
    }
    Err(last.unwrap_or_else(|| anyhow::anyhow!(l.t("cloud-error-expired"))))
}

pub async fn logout(ctx: &Ctx, api: &mut Api) -> anyhow::Result<()> {
    let v = call(ctx, api, "cloud.logout", json!({})).await?;
    if ctx.json {
        ctx.print_json(&v);
    } else {
        ctx.out(ctx.l.t("logout-done"));
    }
    Ok(())
}

pub async fn cloud(ctx: &Ctx, api: &mut Api, cmd: CloudCmd) -> anyhow::Result<()> {
    let l = &ctx.l;
    match cmd {
        CloudCmd::Status => {
            let v = call(ctx, api, "cloud.status", json!({})).await?;
            if ctx.json {
                ctx.print_json(&v);
                return Ok(());
            }
            if v["signed_in"] != true {
                ctx.out(tr!(l, "cloud-signed-out", server = v["api"].as_str().unwrap_or_default()));
                return Ok(());
            }
            ctx.out(tr!(l, "cloud-account", who = who(&v), server = v["api"].as_str().unwrap_or_default()));
            ctx.out(tr!(l, "cloud-plan", plan = plan_name(ctx, v["plan"].as_str().unwrap_or("free"))));
            let lim = &v["limits"];
            let num = |k: &str| lim[k].as_i64().map(|n| n.to_string()).unwrap_or_else(|| l.t("cloud-unlimited"));
            ctx.out(tr!(
                l,
                "cloud-limits",
                members = num("max_members"),
                days = num("retention_days"),
                links = num("spectator_links")
            ));
            let devices = v["devices"].as_array().map(|d| d.len()).unwrap_or(0);
            ctx.out(tr!(l, "cloud-devices", count = devices));
            ctx.out(if v["this_device_linked"] == true {
                l.t("cloud-this-linked")
            } else {
                l.t("cloud-this-unlinked")
            });
            for t in v["teams"].as_array().into_iter().flatten() {
                ctx.out(tr!(
                    l,
                    "cloud-team",
                    name = t["name"].as_str().unwrap_or_default(),
                    role = t["role"].as_str().unwrap_or_default()
                ));
            }
        }
    }
    Ok(())
}

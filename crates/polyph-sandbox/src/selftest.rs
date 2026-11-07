//! `polyph sandbox test`: tries forbidden writes and network calls through
//! each available mechanism and reports which protections hold.

use std::sync::Arc;

use serde::Serialize;

use crate::helper::BLOCKED;
use crate::proxy::{Allowlist, Decider};
use crate::{Mechanism, Policy, detect, wrap};

#[derive(Clone, Debug, Serialize)]
pub struct Check {
    /// Translation key for the check, e.g. `sandbox-check-write-outside`.
    pub key: String,
    pub passed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct MechanismReport {
    pub mechanism: Mechanism,
    pub available: bool,
    pub detail: String,
    pub checks: Vec<Check>,
}

impl MechanismReport {
    pub fn protects(&self) -> bool {
        self.available && !self.checks.is_empty() && self.checks.iter().all(|c| c.passed)
    }
}

async fn run(w: &crate::Wrapped) -> Option<i32> {
    let mut c = tokio::process::Command::new(&w.program);
    c.args(&w.args)
        .current_dir(&w.cwd)
        .envs(w.env.iter().cloned())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    c.status().await.ok().and_then(|s| s.code())
}

/// Runs every probe through every available mechanism (except containers,
/// which need an image). `helper` is the command line for the helper binary.
pub async fn run_all(helper: Vec<String>) -> anyhow::Result<Vec<MechanismReport>> {
    let base = std::env::temp_dir().join(format!("polyph-sandbox-test-{}", std::process::id()));
    let worktree = base.join("worktree");
    let outside = base.join("outside");
    std::fs::create_dir_all(&worktree)?;
    std::fs::create_dir_all(&outside)?;
    std::fs::create_dir_all(base.join("tmp"))?;
    // A loopback service outside the sandbox that must stay unreachable.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let outside_port = listener.local_addr()?.port();
    let accept = tokio::spawn(async move { while listener.accept().await.is_ok() {} });
    let deny: Decider = Arc::new(|_, _| Box::pin(async { false }));
    let proxy = crate::proxy::start(Allowlist::default(), deny, Some(base.join("run"))).await?;
    let policy = Policy {
        worktree: worktree.clone(),
        writable: vec![],
        tmp: Some(base.join("tmp")),
        hidden: vec![],
        read_only: vec![],
        git_writable: vec![],
        proxy_tcp: Some(proxy.tcp),
        proxy_unix: proxy.unix.clone(),
        helper: helper.clone(),
        container_image: None,
    };
    let mut out = vec![];
    for a in detect(&helper) {
        let mut rep = MechanismReport {
            mechanism: a.mechanism,
            available: a.available,
            detail: a.detail.clone(),
            checks: vec![],
        };
        if a.available && a.mechanism != Mechanism::Container {
            let probes: [(&str, Vec<String>, bool); 4] = [
                (
                    "sandbox-check-write-inside",
                    vec!["write".into(), worktree.join("ok.txt").to_string_lossy().into_owned()],
                    true,
                ),
                (
                    "sandbox-check-write-outside",
                    vec!["write".into(), outside.join("escape.txt").to_string_lossy().into_owned()],
                    false,
                ),
                ("sandbox-check-direct-network", vec!["connect".into(), format!("127.0.0.1:{outside_port}")], false),
                ("sandbox-check-proxy-unlisted", vec!["proxy".into(), "unlisted.invalid:443".into()], false),
            ];
            for (key, probe_args, should_succeed) in probes {
                let mut args = vec!["probe".to_string()];
                args.extend(probe_args);
                let w = wrap(a.mechanism, &policy, &helper[0], &[&helper[1..], &args[..]].concat())?;
                let code = run(&w).await;
                let passed = if should_succeed { code == Some(0) } else { code == Some(BLOCKED) };
                rep.checks.push(Check { key: key.into(), passed });
            }
        }
        out.push(rep);
    }
    accept.abort();
    drop(proxy);
    let _ = std::fs::remove_dir_all(&base);
    Ok(out)
}

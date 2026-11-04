//! GitHub: import issues as cards, and with `[merge] via = "github"`, finish
//! tasks as pull requests that Polyph follows until they merge or close.
//! Calls go from the daemon with the person's token; agents never see it.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use polyph_agents::project::ProjectConfig;
use polyph_core::docs::{MergeEntry, MergeState};
use polyph_proto::crdt::{Board, NewCard, card};
use polyph_proto::ids::ObjectId;
use polyph_proto::messages::{AgentRef, ApprovalKind, Decision, DocName};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::daemon::{ApiError, Daemon, Shared};
use crate::integrations::{self, Account};
use crate::runtime::{RoomRuntime, apply};
use crate::tasks::{set_state, update_card};

/// How often open pull requests are checked.
pub const SYNC_EVERY: Duration = Duration::from_secs(60);
const BODY_MAX: usize = 4000;

fn http(a: &Account) -> Result<reqwest::Client, ApiError> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut h = reqwest::header::HeaderMap::new();
    let auth = format!("Bearer {}", a.token);
    h.insert(
        reqwest::header::AUTHORIZATION,
        auth.parse().map_err(|_| ApiError::BadRequest("the token has invalid characters".into()))?,
    );
    h.insert(
        reqwest::header::ACCEPT,
        "application/vnd.github+json".parse().map_err(|_| ApiError::Failed("header".into()))?,
    );
    h.insert("x-github-api-version", "2022-11-28".parse().map_err(|_| ApiError::Failed("header".into()))?);
    reqwest::Client::builder()
        .default_headers(h)
        .user_agent(concat!("polyph/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| ApiError::Failed(e.to_string()))
}

async fn get(a: &Account, path: &str) -> Result<Value, ApiError> {
    let r = http(a)?.get(format!("{}{path}", a.api_url.trim_end_matches('/'))).send().await.map_err(net)?;
    answer(r).await
}

async fn post(a: &Account, path: &str, body: &Value) -> Result<(u16, Value), ApiError> {
    let r = http(a)?.post(format!("{}{path}", a.api_url.trim_end_matches('/'))).json(body).send().await.map_err(net)?;
    let status = r.status().as_u16();
    Ok((status, r.json().await.unwrap_or(Value::Null)))
}

fn net(e: reqwest::Error) -> ApiError {
    ApiError::Failed(format!("could not reach GitHub: {e}"))
}

async fn answer(r: reqwest::Response) -> Result<Value, ApiError> {
    let status = r.status();
    let v: Value = r.json().await.unwrap_or(Value::Null);
    if status.is_success() {
        return Ok(v);
    }
    let msg = v["message"].as_str().unwrap_or("request failed");
    Err(match status.as_u16() {
        401 => ApiError::Forbidden(format!("GitHub refused the token ({msg}); run `polyph github connect` again")),
        403 | 404 => ApiError::NotFound(format!("GitHub: {msg} (check the repository and the token's access)")),
        _ => ApiError::Failed(format!("GitHub {status}: {msg}")),
    })
}

fn account(shared: &Shared) -> Result<Account, ApiError> {
    integrations::github(shared).ok_or_else(|| {
        ApiError::BadRequest("GitHub is not connected; run `polyph github connect` or set GITHUB_TOKEN".into())
    })
}

pub async fn connect(shared: &Shared, token: String, api_url: Option<String>) -> Result<Value, ApiError> {
    let mut a = Account {
        token: token.trim().to_string(),
        api_url: api_url.unwrap_or_else(|| "https://api.github.com".into()),
        login: String::new(),
    };
    let me = get(&a, "/user").await?;
    a.login = me["login"].as_str().unwrap_or_default().to_string();
    let mut i = integrations::load(shared);
    i.github = Some(a.clone());
    integrations::save(shared, &i)?;
    Ok(json!({ "login": a.login, "api_url": a.api_url }))
}

pub fn disconnect(shared: &Shared) -> Result<(), ApiError> {
    let mut i = integrations::load(shared);
    i.github = None;
    integrations::save(shared, &i)
}

/// `owner/name` for the repository at `root`.
pub fn slug(project: &ProjectConfig, root: &Path) -> Result<String, ApiError> {
    if let Some(r) = &project.github.repo {
        return Ok(r.clone());
    }
    let remote = project.merge.remote.clone().unwrap_or_else(|| "origin".into());
    polyph_git::Repo::open(root)
        .ok()
        .and_then(|r| r.remote_url(&remote))
        .and_then(|u| polyph_git::github_slug(&u))
        .ok_or_else(|| ApiError::BadRequest(format!("cannot tell the GitHub repository; set [github] repo in polyph.toml or add a GitHub remote named {remote}")))
}

fn clip(s: &str) -> String {
    let s = polyph_core::redact::redact(s).into_owned();
    if s.len() <= BODY_MAX {
        return s;
    }
    let mut cut = BODY_MAX;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}…", &s[..cut])
}

#[derive(Debug, Deserialize)]
pub struct ImportParams {
    pub room: String,
    pub cwd: Option<std::path::PathBuf>,
    pub repo: Option<String>,
    #[serde(default)]
    pub labels: Vec<String>,
    pub limit: Option<u32>,
}

/// Open issues become backlog cards (once; re-importing skips known ones).
pub async fn import(d: &Arc<Daemon>, p: ImportParams) -> Result<Value, ApiError> {
    let rt = d.find(&p.room).await?;
    let a = account(&d.shared)?;
    let repo = match p.repo {
        Some(r) => r,
        None => {
            let cwd = p.cwd.clone().unwrap_or_else(|| std::path::PathBuf::from("."));
            slug(&ProjectConfig::load(&cwd).unwrap_or_default(), &cwd)?
        }
    };
    let per_page = p.limit.unwrap_or(50).clamp(1, 100);
    let mut path = format!("/repos/{repo}/issues?state=open&per_page={per_page}");
    if !p.labels.is_empty() {
        path.push_str(&format!("&labels={}", p.labels.join(",")));
    }
    let issues = get(&a, &path).await?;
    let now = d.shared.clock.now();
    let mut ctx = rt.ctx.lock().await;
    let known: Vec<String> =
        Board::new(ctx.core.doc(&DocName::Board)).all_cards().into_iter().filter_map(|c| c.external).collect();
    let me = ctx.core.my_member.to_hex();
    let mut added = vec![];
    let mut cards = vec![];
    for i in issues.as_array().into_iter().flatten() {
        // The issues endpoint lists pull requests too.
        if i.get("pull_request").is_some() {
            continue;
        }
        let Some(n) = i["number"].as_u64() else { continue };
        let ext = format!("github:{repo}#{n}");
        if known.contains(&ext) {
            continue;
        }
        let id = ObjectId::random().map_err(|e| ApiError::Failed(e.to_string()))?.to_hex();
        cards.push((
            NewCard {
                id: id.clone(),
                title: clip(i["title"].as_str().unwrap_or_default()),
                description: clip(i["body"].as_str().unwrap_or_default()),
                column: "backlog".into(),
                created_by: me.clone(),
                created_at: now,
                kind: Some("issue".into()),
                ..Default::default()
            },
            ext,
            i["html_url"].as_str().unwrap_or_default().to_string(),
        ));
        added.push(json!({ "id": id, "number": n }));
    }
    let fx = ctx.core.change_doc(DocName::Board, |doc| {
        let b = Board::new(doc);
        for (c, ext, url) in &cards {
            if b.add_card(c).is_ok() {
                let _ = b.set_field(&c.id, card::EXTERNAL, yrs::Any::String(ext.as_str().into()), now);
                let _ = b.set_field(&c.id, card::URL, yrs::Any::String(url.as_str().into()), now);
            }
        }
    })?;
    apply(&rt, &mut ctx, &d.shared, fx).await;
    Ok(json!({ "repo": repo, "added": added }))
}

/// Pushes the task branch (after a person approves) and opens its pull request.
pub async fn open_pr(
    d: &Arc<Daemon>,
    rt: &Arc<RoomRuntime>,
    e: &mut MergeEntry,
    root: &Path,
    agent: &AgentRef,
) -> Result<(), ApiError> {
    let shared = d.shared.clone();
    let project = ProjectConfig::load(root).unwrap_or_default();
    let a = account(&shared)?;
    let repo = slug(&project, root)?;
    let remote = project.merge.remote.clone().unwrap_or_else(|| "origin".into());
    let task = e.card.as_deref().and_then(|c| ObjectId::from_hex(c).ok());
    let kind = ApprovalKind::Push { remote: remote.clone(), branch: e.branch.clone() };
    if let Decision::Deny { note } = crate::agents::request_approval(rt, &shared, agent, kind, task).await {
        let why = note.map(|n| format!(": {n}")).unwrap_or_default();
        set_state(rt, &shared, e, MergeState::Failed, Some(format!("pushing {} was not approved{why}", e.branch)))
            .await;
        return Ok(());
    }
    let (root2, remote2, branch2) = (root.to_path_buf(), remote.clone(), e.branch.clone());
    tokio::task::spawn_blocking(move || polyph_git::Repo::open(&root2).and_then(|r| r.push_branch(&remote2, &branch2)))
        .await
        .map_err(|x| ApiError::Failed(x.to_string()))?
        .map_err(|x| ApiError::Failed(format!("pushing {} failed: {x}", e.branch)))?;

    let card = match &e.card {
        Some(c) => crate::tasks::card(&mut *rt.ctx.lock().await, c),
        None => None,
    };
    let intent = crate::tasks::intent_of(card.as_ref(), &e.branch);
    let mut body = String::new();
    if !intent.description.trim().is_empty() {
        body.push_str(intent.description.trim());
        body.push_str("\n\n");
    }
    if !intent.acceptance.is_empty() {
        body.push_str("Done when:\n");
        for x in &intent.acceptance {
            body.push_str(&format!("- [ ] {x}\n"));
        }
        body.push('\n');
    }
    if let Some(n) =
        card.as_ref().and_then(|c| c.external.as_deref()).and_then(|x| x.strip_prefix(&format!("github:{repo}#")))
    {
        body.push_str(&format!("Closes #{n}\n\n"));
    }
    body.push_str(&format!("Opened by Polyph for {}.", e.agent.clone().unwrap_or_default()));
    let title = if intent.title.is_empty() { e.branch.clone() } else { intent.title.clone() };
    let req = json!({ "title": clip(&title), "head": e.branch, "base": e.into, "body": clip(&body) });
    let (status, v) = post(&a, &format!("/repos/{repo}/pulls"), &req).await?;
    let pr = if (200..300).contains(&status) {
        v
    } else if status == 422 {
        // Already open for this branch (a task finished again after review).
        let owner = repo.split('/').next().unwrap_or_default();
        let list = get(&a, &format!("/repos/{repo}/pulls?state=open&head={owner}:{}", e.branch)).await?;
        list.as_array().and_then(|l| l.first().cloned()).ok_or_else(|| {
            ApiError::Failed(format!(
                "GitHub would not open the pull request: {}",
                v["message"].as_str().unwrap_or("unprocessable")
            ))
        })?
    } else {
        return Err(ApiError::Failed(format!(
            "GitHub {status}: {}",
            v["message"].as_str().unwrap_or("could not open the pull request")
        )));
    };
    let url = pr["html_url"].as_str().unwrap_or_default().to_string();
    e.pr = Some(url.clone());
    e.pr_number = pr["number"].as_u64();
    e.pr_repo = Some(repo.clone());
    set_state(rt, &shared, e, MergeState::PullRequest, Some(url.clone())).await;
    if let Some(c) = &e.card {
        update_card(rt, &shared, c, Some("review"), &[(card::PR, Some(url))]).await;
    }
    Ok(())
}

/// Checks this device's open pull requests in a room. Returns how many changed.
pub async fn sync_room(d: &Arc<Daemon>, rt: &Arc<RoomRuntime>) -> Result<usize, ApiError> {
    let me = d.shared.identity.device_id().to_hex();
    let open: Vec<MergeEntry> = crate::tasks::queue(rt)
        .await
        .into_iter()
        .filter(|e| e.state == MergeState::PullRequest && e.device == me && e.pr_number.is_some())
        .collect();
    if open.is_empty() {
        return Ok(0);
    }
    let a = account(&d.shared)?;
    let mut changed = 0;
    for mut e in open {
        let Some(url) = e.pr.clone() else { continue };
        let Some(repo) = e.pr_repo.clone() else { continue };
        let n = e.pr_number.unwrap_or_default();
        let pr = get(&a, &format!("/repos/{repo}/pulls/{n}")).await?;
        if pr["merged"].as_bool() == Some(true) {
            e.merge_commit = pr["merge_commit_sha"].as_str().map(str::to_string);
            set_state(rt, &d.shared, &mut e, MergeState::Merged, Some(url.clone())).await;
            if let Some(c) = &e.card {
                update_card(rt, &d.shared, c, Some("done"), &[]).await;
                crate::plan::promote_unblocked(rt, &d.shared).await;
            }
            changed += 1;
            continue;
        }
        if pr["state"].as_str() == Some("closed") {
            set_state(rt, &d.shared, &mut e, MergeState::Failed, Some(format!("{url} was closed without merging")))
                .await;
            changed += 1;
            continue;
        }
        // The latest review from each reviewer decides.
        let reviews = get(&a, &format!("/repos/{repo}/pulls/{n}/reviews?per_page=100")).await?;
        let mut latest: std::collections::BTreeMap<String, (String, String)> = Default::default();
        for r in reviews.as_array().into_iter().flatten() {
            let who = r["user"]["login"].as_str().unwrap_or_default().to_string();
            let state = r["state"].as_str().unwrap_or_default().to_string();
            if state == "COMMENTED" {
                continue;
            }
            latest.insert(who, (state, r["body"].as_str().unwrap_or_default().to_string()));
        }
        let asks: Vec<String> = latest
            .iter()
            .filter(|(_, (s, _))| s == "CHANGES_REQUESTED")
            .map(
                |(who, (_, body))| if body.trim().is_empty() { who.clone() } else { format!("{who}: {}", body.trim()) },
            )
            .collect();
        if !asks.is_empty() {
            let notes = clip(&asks.join("\n"));
            set_state(rt, &d.shared, &mut e, MergeState::ChangesRequested, Some(notes.clone())).await;
            if let Some(c) = &e.card {
                update_card(rt, &d.shared, c, Some("in_progress"), &[]).await;
            }
            changed += 1;
        }
    }
    Ok(changed)
}

/// Follows open pull requests for the daemon's lifetime.
pub async fn run(d: std::sync::Weak<Daemon>) {
    loop {
        tokio::time::sleep(SYNC_EVERY).await;
        let Some(d) = d.upgrade() else { return };
        if integrations::github(&d.shared).is_none() {
            continue;
        }
        for rt in d.room_list().await {
            if let Err(e) = sync_room(&d, &rt).await {
                tracing::debug!(error = %e, "GitHub sync");
            }
        }
    }
}

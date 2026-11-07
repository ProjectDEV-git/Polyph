//! `polyph.toml` at the repository root: agents, checks, prices, autonomy,
//! sandbox and router settings for a project.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::policy::Preset;
use crate::registry::AgentConfig;
use crate::usage::PriceTable;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PolicyConfig {
    pub preset: Preset,
    /// Claim unclaimed files automatically when an agent edits them.
    pub auto_claim: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SandboxConfig {
    /// "auto" (default), "container", or "off" (no isolation; needs explicit opt-in).
    pub mode: Option<String>,
    pub image: Option<String>,
    /// Hosts added to the network allowlist for every agent.
    pub allow_hosts: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RouterConfig {
    /// Tier name ("small", "standard", "frontier") to model id, per agent kind.
    pub tiers: BTreeMap<String, BTreeMap<String, String>>,
    pub enabled: bool,
}

/// `[merge]`: how finished tasks reach their target branch.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct MergeConfig {
    /// Branch tasks merge into; default: the branch the task started from.
    pub into: Option<String>,
    /// Agent kind that reviews each task before it merges; none by default.
    pub reviewer: Option<String>,
    /// Agent kind that resolves conflicts Polyph cannot merge by itself.
    pub resolver: Option<String>,
    /// Ask a person before merging (default true).
    pub approval: Option<bool>,
    /// Run `checks` on the merge result before the branch moves (default true).
    pub run_checks: Option<bool>,
    /// Remote to push the target branch to after each merge (asks a person first).
    pub push: Option<String>,
    /// "queue" (default): Polyph merges. "github": push the branch and open a
    /// pull request; the card follows the pull request.
    pub via: Option<String>,
    /// Remote task branches are pushed to for pull requests (default "origin").
    pub remote: Option<String>,
}

/// `[github]`: the repository's place on GitHub.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct GithubConfig {
    /// `owner/name`; default: read from the remote's URL.
    pub repo: Option<String>,
}

/// `[linear]`: the Linear team this project's cards sync with.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LinearConfig {
    /// Team key, e.g. "ENG".
    pub team: Option<String>,
}

/// `[planner]`: the agent that splits a goal into tasks.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PlannerConfig {
    pub agent: Option<String>,
}

/// `[claims]`: how edits become claims.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ClaimsConfig {
    /// "file" (default) or "symbol": claim only the functions an edit changes.
    pub granularity: Option<String>,
    /// Watch worktrees for edits made outside the agent protocol (default true).
    pub watch: Option<bool>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProjectConfig {
    pub agents: BTreeMap<String, AgentConfig>,
    /// Commands agents may run without approval, e.g. "cargo test".
    pub checks: Vec<String>,
    pub prices: PriceTable,
    pub policy: PolicyConfig,
    pub sandbox: SandboxConfig,
    pub router: RouterConfig,
    pub merge: MergeConfig,
    pub planner: PlannerConfig,
    pub claims: ClaimsConfig,
    pub github: GithubConfig,
    pub linear: LinearConfig,
}

impl ProjectConfig {
    /// Reads `polyph.toml` from `dir` or its nearest parent that has one.
    /// The file may be an agent's (in its worktree), so it is read without
    /// following links (SEC-16), and an error names the place in it but
    /// never quotes it: errors reach agents (a task's handoff, for one).
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        let mut d = Some(dir);
        while let Some(cur) = d {
            let p = cur.join("polyph.toml");
            if std::fs::symlink_metadata(&p).is_ok() {
                let s = polyph_plugins::read_text_under(cur, Path::new("polyph.toml"))
                    .map_err(|e| anyhow::anyhow!("cannot read {}: {}", p.display(), describe(&e)))?;
                return toml::from_str(&s).map_err(|e| {
                    let at = match e.span() {
                        Some(span) => {
                            let before = &s[..span.start.min(s.len())];
                            let line = before.matches('\n').count() + 1;
                            let column = before.rsplit('\n').next().map(|l| l.chars().count()).unwrap_or(0) + 1;
                            format!(" at line {line}, column {column}")
                        }
                        None => String::new(),
                    };
                    anyhow::anyhow!("{} is not valid TOML or has a setting of the wrong type{at}", p.display())
                });
            }
            d = cur.parent();
        }
        Ok(Self::default())
    }

    pub fn auto_claim(&self) -> bool {
        self.policy.auto_claim.unwrap_or(true)
    }

    pub fn symbol_claims(&self) -> bool {
        self.claims.granularity.as_deref() == Some("symbol")
    }

    pub fn watch(&self) -> bool {
        self.claims.watch.unwrap_or(true)
    }
}

/// Why a file could not be read, without the operating system's wording
/// for odd cases (which may name other paths).
fn describe(e: &std::io::Error) -> &'static str {
    match e.kind() {
        std::io::ErrorKind::InvalidInput => "it is a link, a hard link or not a regular file",
        std::io::ErrorKind::FileTooLarge => "it is too large",
        std::io::ErrorKind::InvalidData => "it is not UTF-8 text",
        std::io::ErrorKind::PermissionDenied => "permission denied",
        _ => "it could not be read",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_project_file() {
        let t = r#"
checks = ["cargo test", "cargo clippy"]

[policy]
preset = "auto"

[agents.mine]
command = "my-agent"
args = ["--acp"]
hosts = ["api.example.com"]

[prices."model-a"]
input = 3.0
output = 15.0

[router.tiers.claude]
small = "haiku"
"#;
        let c: ProjectConfig = toml::from_str(t).unwrap();
        assert_eq!(c.policy.preset, Preset::Auto);
        assert_eq!(c.agents["mine"].args, vec!["--acp"]);
        assert_eq!(c.prices.0["model-a"].output, 15.0);
        assert_eq!(c.router.tiers["claude"]["small"], "haiku");
        assert!(c.auto_claim());
    }

    /// SEC-16: a `polyph.toml` linked to a secret is not read, and a parse
    /// error never quotes the file.
    #[test]
    fn project_file_errors_quote_nothing() {
        let d = tempfile::tempdir().unwrap();
        let wt = d.path().join("wt");
        std::fs::create_dir_all(&wt).unwrap();
        let secret = d.path().join("secret");
        std::fs::write(&secret, "token = \"SECRET-TOKEN-123\"\nnot toml SECRET-LINE\n").unwrap();
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&secret, wt.join("polyph.toml")).unwrap();
            let e = ProjectConfig::load(&wt).unwrap_err().to_string();
            assert!(!e.contains("SECRET"), "{e}");
            assert!(e.contains("is a link"), "{e}");
            std::fs::remove_file(wt.join("polyph.toml")).unwrap();
            std::fs::hard_link(&secret, wt.join("polyph.toml")).unwrap();
            let e = ProjectConfig::load(&wt).unwrap_err().to_string();
            assert!(!e.contains("SECRET"), "{e}");
            std::fs::remove_file(wt.join("polyph.toml")).unwrap();
        }
        // The agent's own file, invalid: where, never what.
        std::fs::write(wt.join("polyph.toml"), "checks = [\"ok\"]\nnot toml SECRET-LINE\n").unwrap();
        let e = ProjectConfig::load(&wt).unwrap_err().to_string();
        assert!(!e.contains("SECRET"), "{e}");
        assert!(e.contains("at line 2, column"), "{e}");
        // A value of the wrong type is not echoed either.
        std::fs::write(wt.join("polyph.toml"), "checks = \"SECRET-VALUE\"\n").unwrap();
        let e = ProjectConfig::load(&wt).unwrap_err().to_string();
        assert!(!e.contains("SECRET") && e.contains("line 1"), "{e}");
        std::fs::write(wt.join("polyph.toml"), "checks = [\"cargo test\"]\n").unwrap();
        assert_eq!(ProjectConfig::load(&wt).unwrap().checks, ["cargo test"]);
    }
}

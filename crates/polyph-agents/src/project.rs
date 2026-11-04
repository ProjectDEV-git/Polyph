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
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        let mut d = Some(dir);
        while let Some(cur) = d {
            let p = cur.join("polyph.toml");
            if p.exists() {
                let s = std::fs::read_to_string(&p)?;
                return toml::from_str(&s).map_err(|e| anyhow::anyhow!("{} is not valid: {e}", p.display()));
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
}

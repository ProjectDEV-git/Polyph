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

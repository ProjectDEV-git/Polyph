//! The built-in model router: which model each agent runs on.
//!
//! `[router.tiers.<kind>]` maps tier names ("small", "standard", "frontier")
//! to model ids per agent kind, and [`select`] picks one for a role under
//! budget pressure. The choice is applied with ACP `session/set_model`;
//! agents that do not support it keep the model they started with.

use crate::project::RouterConfig;
use crate::usage::BudgetState;

/// Tier names, cheapest first. Custom tier names are allowed; unknown names
/// only fall back to "small".
pub const TIERS: [&str; 3] = ["small", "standard", "frontier"];

/// How hard budget pressure is; higher picks cheaper models.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Pressure {
    /// Every applicable budget under 80%.
    Ok,
    /// A budget at or above 80%.
    Tight,
    /// A budget at or over its limit.
    Over,
}

impl Pressure {
    /// The worst state among `states`.
    pub fn of(states: impl Iterator<Item = BudgetState>) -> Self {
        let mut p = Pressure::Ok;
        for s in states {
            p = p.max(match s {
                BudgetState::Ok => Pressure::Ok,
                BudgetState::Warning { .. } => Pressure::Tight,
                BudgetState::Exceeded => Pressure::Over,
            });
        }
        p
    }
}

/// Whether routing is on: `enabled` when set, otherwise on when tiers exist.
pub fn enabled(cfg: &RouterConfig) -> bool {
    cfg.enabled.unwrap_or(!cfg.tiers.is_empty())
}

/// The tier a `role` starts at: `task_tiers`, else `default_tier`, else
/// "standard".
pub fn tier_for(cfg: &RouterConfig, role: &str) -> String {
    cfg.task_tiers.get(role).map(|t| t.as_str()).or(cfg.default_tier.as_deref()).unwrap_or("standard").to_string()
}

/// One step cheaper; "small" and unknown names stay "small".
pub fn cheaper(tier: &str) -> &'static str {
    match tier {
        "frontier" => "standard",
        "standard" => "small",
        _ => "small",
    }
}

/// The tiers to try from `tier` down: the name itself, then cheaper ones,
/// ending at "small".
fn chain(tier: &str) -> Vec<&str> {
    let mut out = vec![tier];
    let mut cur = tier;
    while cur != "small" {
        cur = cheaper(cur);
        out.push(cur);
    }
    out
}

/// The model for `kind` at `role` under `pressure`: one step down on `Tight`,
/// the cheapest tier on `Over`, falling back to cheaper tiers when the
/// configured one names no model. None when routing is off or `kind` has no
/// models at all.
pub fn select(cfg: &RouterConfig, kind: &str, role: &str, pressure: Pressure) -> Option<String> {
    if !enabled(cfg) {
        return None;
    }
    let models = cfg.tiers.get(kind)?;
    let mut tier = tier_for(cfg, role);
    if pressure >= Pressure::Over {
        tier = "small".into();
    } else if pressure >= Pressure::Tight {
        tier = cheaper(&tier).into();
    }
    chain(&tier).into_iter().find_map(|t| models.get(t).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> RouterConfig {
        let mut tiers = std::collections::BTreeMap::new();
        let mut claude = std::collections::BTreeMap::new();
        claude.insert("small".into(), "haiku".into());
        claude.insert("standard".into(), "sonnet".into());
        claude.insert("frontier".into(), "opus".into());
        tiers.insert("claude".into(), claude);
        RouterConfig { tiers, ..Default::default() }
    }

    #[test]
    fn off_when_disabled_or_empty() {
        let mut c = cfg();
        c.enabled = Some(false);
        assert_eq!(select(&c, "claude", "task", Pressure::Ok), None);
        assert!(!enabled(&c));
        let empty = RouterConfig::default();
        assert!(!enabled(&empty));
        assert_eq!(select(&empty, "claude", "task", Pressure::Ok), None);
        assert!(enabled(&cfg()));
    }

    #[test]
    fn tier_by_role_and_default() {
        let mut c = cfg();
        assert_eq!(tier_for(&c, "task"), "standard");
        c.default_tier = Some("frontier".into());
        assert_eq!(tier_for(&c, "task"), "frontier");
        c.task_tiers.insert("planner".into(), "small".into());
        assert_eq!(tier_for(&c, "planner"), "small");
        assert_eq!(tier_for(&c, "task"), "frontier");
    }

    #[test]
    fn pressure_walks_the_ladder() {
        let mut c = cfg();
        c.task_tiers.insert("planner".into(), "frontier".into());
        assert_eq!(select(&c, "claude", "planner", Pressure::Ok).as_deref(), Some("opus"));
        assert_eq!(select(&c, "claude", "planner", Pressure::Tight).as_deref(), Some("sonnet"));
        assert_eq!(select(&c, "claude", "planner", Pressure::Over).as_deref(), Some("haiku"));
        // Tight on the cheapest tier stays cheap.
        c.task_tiers.insert("task".into(), "small".into());
        assert_eq!(select(&c, "claude", "task", Pressure::Tight).as_deref(), Some("haiku"));
    }

    #[test]
    fn falls_back_to_cheaper_tiers() {
        let mut c = cfg();
        c.tiers.get_mut("claude").unwrap().remove("frontier");
        c.task_tiers.insert("planner".into(), "frontier".into());
        assert_eq!(select(&c, "claude", "planner", Pressure::Ok).as_deref(), Some("sonnet"));
        c.tiers.get_mut("claude").unwrap().remove("standard");
        assert_eq!(select(&c, "claude", "planner", Pressure::Ok).as_deref(), Some("haiku"));
    }

    #[test]
    fn custom_tier_names() {
        let mut c = cfg();
        let mut models = c.tiers.get_mut("claude").unwrap().clone();
        models.insert("big".into(), "opus".into());
        *c.tiers.get_mut("claude").unwrap() = models;
        c.default_tier = Some("big".into());
        assert_eq!(select(&c, "claude", "task", Pressure::Ok).as_deref(), Some("opus"));
        // Tight drops the unknown name to "small".
        assert_eq!(select(&c, "claude", "task", Pressure::Tight).as_deref(), Some("haiku"));
    }

    #[test]
    fn unknown_kind_routes_nothing() {
        assert_eq!(select(&cfg(), "codex", "task", Pressure::Ok), None);
    }
}

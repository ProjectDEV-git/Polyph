//! Plans and their limits. Prices are undecided: a plan's `price` stays unset
//! until the owner fills it in, and the website says "Pricing coming soon".

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanId {
    Free,
    Pro,
    Team,
}

impl PlanId {
    pub const ALL: [PlanId; 3] = [PlanId::Free, PlanId::Pro, PlanId::Team];

    pub fn as_str(self) -> &'static str {
        match self {
            PlanId::Free => "free",
            PlanId::Pro => "pro",
            PlanId::Team => "team",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == s)
    }
}

/// One plan. `None` limits mean unlimited.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlanConfig {
    /// Distinct people per room.
    pub max_members: Option<usize>,
    /// Days the hosted relay keeps a room's encrypted snapshots after the last write.
    pub retention_days: Option<u32>,
    /// Spectator links per room.
    pub spectator_links: Option<u32>,
    /// Display price, exactly as it should appear (for example a monthly
    /// amount with its currency). Unset until prices are decided.
    pub price: Option<String>,
    /// The Stripe price id checkout uses. Without it the plan cannot be bought.
    pub stripe_price: Option<String>,
}

impl PlanConfig {
    pub fn limits(&self) -> polyph_relay::Limits {
        polyph_relay::Limits { max_members: self.max_members, retention_days: self.retention_days }
    }
}

/// What the brief fixes: Free keeps rooms 7 days with one spectator link per
/// room, Pro keeps them a year. Team keeps at least what Pro keeps (inferred).
/// Member counts are unset here and set in the service's configuration.
pub fn defaults() -> BTreeMap<PlanId, PlanConfig> {
    BTreeMap::from([
        (PlanId::Free, PlanConfig { retention_days: Some(7), spectator_links: Some(1), ..Default::default() }),
        (PlanId::Pro, PlanConfig { retention_days: Some(365), ..Default::default() }),
        (PlanId::Team, PlanConfig { retention_days: Some(365), ..Default::default() }),
    ])
}

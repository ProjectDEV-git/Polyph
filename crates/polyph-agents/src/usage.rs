//! Usage accounting, cost estimates and budgets.
//!
//! Usage comes only from what agents report. When an agent reports nothing
//! for a turn, the turn is counted as "unknown" rather than estimated.
//! Prices come from an editable table; models without a price have no cost
//! estimate.

use std::collections::{BTreeMap, HashMap};

use polyph_proto::messages::Usage;
use serde::{Deserialize, Serialize};

/// Dollars per million tokens.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Price {
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub cache_read: f64,
    #[serde(default)]
    pub cache_write: f64,
}

/// `[prices."model-name"]` tables from `polyph.toml` or user config.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct PriceTable(pub HashMap<String, Price>);

impl PriceTable {
    pub fn cost(&self, u: &Usage) -> Option<f64> {
        if u.subscription {
            return None;
        }
        let model = u.model.as_ref()?;
        // Exact name first, then the longest configured prefix.
        let p = self.0.get(model).or_else(|| {
            self.0.iter().filter(|(k, _)| model.starts_with(k.as_str())).max_by_key(|(k, _)| k.len()).map(|(_, v)| v)
        })?;
        let m = 1_000_000.0;
        Some(
            u.input_tokens.unwrap_or(0) as f64 * p.input / m
                + u.output_tokens.unwrap_or(0) as f64 * p.output / m
                + u.cache_read_tokens.unwrap_or(0) as f64 * p.cache_read / m
                + u.cache_write_tokens.unwrap_or(0) as f64 * p.cache_write / m,
        )
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Totals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    /// Estimated dollars, for usage that has a price.
    pub cost: f64,
    /// Turns whose cost could not be estimated (no price or subscription).
    pub unpriced_turns: u32,
    /// Turns where the agent reported no usage at all.
    pub unknown_turns: u32,
    pub turns: u32,
    pub subscription: bool,
}

impl Totals {
    pub fn tokens(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.cache_read_tokens + self.cache_write_tokens
    }

    fn add(&mut self, u: Option<&Usage>, cost: Option<f64>) {
        self.turns += 1;
        let Some(u) = u else {
            self.unknown_turns += 1;
            return;
        };
        self.input_tokens += u.input_tokens.unwrap_or(0);
        self.output_tokens += u.output_tokens.unwrap_or(0);
        self.cache_read_tokens += u.cache_read_tokens.unwrap_or(0);
        self.cache_write_tokens += u.cache_write_tokens.unwrap_or(0);
        self.subscription |= u.subscription;
        match cost {
            Some(c) => self.cost += c,
            None => self.unpriced_turns += 1,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Ledger {
    pub by_member: BTreeMap<String, Totals>,
    pub by_agent: BTreeMap<String, Totals>,
    pub by_task: BTreeMap<String, Totals>,
    pub room: Totals,
}

impl Ledger {
    /// Records one turn. `usage` is None when the agent reported nothing.
    pub fn record(
        &mut self,
        prices: &PriceTable,
        member: &str,
        agent: &str,
        task: Option<&str>,
        usage: Option<&Usage>,
    ) {
        let cost = usage.and_then(|u| prices.cost(u));
        self.room.add(usage, cost);
        self.by_member.entry(member.into()).or_default().add(usage, cost);
        self.by_agent.entry(agent.into()).or_default().add(usage, cost);
        if let Some(t) = task {
            self.by_task.entry(t.into()).or_default().add(usage, cost);
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "unit", content = "amount", rename_all = "snake_case")]
pub enum Limit {
    Tokens(u64),
    Dollars(f64),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BudgetScope {
    Room,
    Member(String),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Budget {
    pub scope: BudgetScope,
    pub limit: Limit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetState {
    Ok,
    /// At or above 80%: warn the owner.
    Warning {
        percent: u32,
    },
    /// At or above 100%: stop sending prompts until a Maintainer raises it.
    Exceeded,
}

impl Budget {
    pub fn used(&self, ledger: &Ledger) -> f64 {
        let t = match &self.scope {
            BudgetScope::Room => Some(&ledger.room),
            BudgetScope::Member(m) => ledger.by_member.get(m),
        };
        match (self.limit, t) {
            (_, None) => 0.0,
            (Limit::Tokens(_), Some(t)) => t.tokens() as f64,
            (Limit::Dollars(_), Some(t)) => t.cost,
        }
    }

    pub fn state(&self, ledger: &Ledger) -> BudgetState {
        let limit = match self.limit {
            Limit::Tokens(n) => n as f64,
            Limit::Dollars(d) => d,
        };
        if limit <= 0.0 {
            return BudgetState::Exceeded;
        }
        let pct = self.used(ledger) / limit * 100.0;
        if pct >= 100.0 {
            BudgetState::Exceeded
        } else if pct >= 80.0 {
            BudgetState::Warning { percent: pct.floor() as u32 }
        } else {
            BudgetState::Ok
        }
    }

    pub fn applies_to(&self, member: &str) -> bool {
        match &self.scope {
            BudgetScope::Room => true,
            BudgetScope::Member(m) => m == member,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usage(i: u64, o: u64, model: &str) -> Usage {
        Usage {
            input_tokens: Some(i),
            output_tokens: Some(o),
            cache_read_tokens: None,
            cache_write_tokens: None,
            model: Some(model.into()),
            subscription: false,
        }
    }

    #[test]
    fn costs_unknowns_and_budgets() {
        let mut prices = PriceTable::default();
        prices.0.insert("model-a".into(), Price { input: 3.0, output: 15.0, ..Default::default() });
        let mut l = Ledger::default();
        l.record(&prices, "m1", "a1", Some("t1"), Some(&usage(1_000_000, 100_000, "model-a-2026")));
        l.record(&prices, "m1", "a1", Some("t1"), None);
        l.record(&prices, "m2", "a2", None, Some(&usage(10, 10, "unpriced")));
        assert!((l.room.cost - 4.5).abs() < 1e-9);
        assert_eq!(l.room.unknown_turns, 1);
        assert_eq!(l.room.unpriced_turns, 1);
        assert_eq!(l.by_task["t1"].turns, 2);

        let b = Budget { scope: BudgetScope::Member("m1".into()), limit: Limit::Dollars(5.0) };
        assert_eq!(b.state(&l), BudgetState::Warning { percent: 90 });
        let b = Budget { scope: BudgetScope::Room, limit: Limit::Tokens(1_000_000) };
        assert_eq!(b.state(&l), BudgetState::Exceeded);
        assert!(!Budget { scope: BudgetScope::Member("m2".into()), limit: Limit::Tokens(1) }.applies_to("m1"));
    }
}

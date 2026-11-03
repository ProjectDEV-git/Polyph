//! The claim table: who may edit what. Only the acting host's daemon grants
//! claims; every daemon keeps a copy for local checks.

use std::collections::BTreeMap;

use polyph_proto::ids::ObjectId;
use polyph_proto::messages::{AgentRef, Claim, ClaimTarget};

/// Normalizes a repository-relative path: forward slashes, no `./`, no
/// duplicate separators. Directory claims keep a trailing `/`.
pub fn normalize_path(p: &str) -> String {
    let is_dir = p.ends_with('/') || p.ends_with('\\');
    let parts: Vec<&str> = p.split(['/', '\\']).filter(|s| !s.is_empty() && *s != ".").collect();
    let mut s = parts.join("/");
    if is_dir && !s.is_empty() {
        s.push('/');
    }
    s
}

pub fn normalize(t: &ClaimTarget) -> ClaimTarget {
    ClaimTarget { path: normalize_path(&t.path), symbol: t.symbol.clone() }
}

/// Whether two claim targets overlap (see docs/protocol.md 10.4).
pub fn targets_conflict(a: &ClaimTarget, b: &ClaimTarget) -> bool {
    let (a, b) = (normalize(a), normalize(b));
    let a_dir = a.path.ends_with('/');
    let b_dir = b.path.ends_with('/');
    match (a_dir, b_dir) {
        (true, true) => a.path.starts_with(&b.path) || b.path.starts_with(&a.path),
        (true, false) => b.path.starts_with(&a.path),
        (false, true) => a.path.starts_with(&b.path),
        (false, false) => {
            if a.path != b.path {
                return false;
            }
            match (&a.symbol, &b.symbol) {
                (Some(x), Some(y)) => x == y,
                _ => true,
            }
        }
    }
}

fn same_agent(a: &AgentRef, b: &AgentRef) -> bool {
    a.id == b.id
}

#[derive(Debug, Default, Clone)]
pub struct ClaimTable {
    claims: BTreeMap<ObjectId, Claim>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GrantResult {
    pub granted: Vec<Claim>,
    pub denied: Vec<Claim>,
}

impl ClaimTable {
    pub fn all(&self) -> Vec<Claim> {
        let mut v: Vec<Claim> = self.claims.values().cloned().collect();
        v.sort_by(|a, b| a.granted_at.cmp(&b.granted_at).then(a.id.cmp(&b.id)));
        v
    }

    pub fn len(&self) -> usize {
        self.claims.len()
    }

    pub fn is_empty(&self) -> bool {
        self.claims.is_empty()
    }

    pub fn get(&self, id: &ObjectId) -> Option<&Claim> {
        self.claims.get(id)
    }

    /// The live claim held by someone other than `agent` that blocks `target`.
    pub fn holder(&self, target: &ClaimTarget, agent: Option<&AgentRef>, now: i64) -> Option<&Claim> {
        self.claims.values().find(|c| {
            c.expires_at > now && agent.is_none_or(|a| !same_agent(a, &c.owner)) && targets_conflict(&c.target, target)
        })
    }

    /// Arbiter: grants what is free, renews what the agent already holds,
    /// and reports the conflicting claims for the rest.
    #[allow(clippy::too_many_arguments)]
    pub fn grant(
        &mut self,
        agent: &AgentRef,
        targets: &[ClaimTarget],
        reason: &str,
        task: Option<ObjectId>,
        lease_secs: u32,
        now: i64,
        term: u64,
        new_id: &mut dyn FnMut() -> ObjectId,
    ) -> GrantResult {
        self.expire(now);
        let mut granted = vec![];
        let mut denied = vec![];
        for t in targets {
            let t = normalize(t);
            if let Some(holder) = self.holder(&t, Some(agent), now) {
                denied.push(holder.clone());
                continue;
            }
            let expires_at = now + lease_secs as i64;
            if let Some(existing) = self.claims.values_mut().find(|c| same_agent(&c.owner, agent) && c.target == t) {
                existing.expires_at = expires_at;
                existing.reason = reason.to_string();
                granted.push(existing.clone());
                continue;
            }
            let c = Claim {
                id: new_id(),
                target: t,
                owner: agent.clone(),
                reason: reason.to_string(),
                task,
                granted_at: now,
                expires_at,
                term,
            };
            self.claims.insert(c.id, c.clone());
            granted.push(c);
        }
        GrantResult { granted, denied }
    }

    pub fn renew(&mut self, ids: &[ObjectId], lease_secs: u32, now: i64) {
        for id in ids {
            if let Some(c) = self.claims.get_mut(id) {
                c.expires_at = now + lease_secs as i64;
            }
        }
    }

    pub fn release(&mut self, ids: &[ObjectId]) {
        for id in ids {
            self.claims.remove(id);
        }
    }

    pub fn release_agent(&mut self, agent: &ObjectId) {
        self.claims.retain(|_, c| c.owner.id != *agent);
    }

    /// Hands every claim of `from` to `to` (task handoff transfers claims).
    pub fn transfer(&mut self, from: &ObjectId, to: &AgentRef) {
        for c in self.claims.values_mut() {
            if c.owner.id == *from {
                c.owner = to.clone();
            }
        }
    }

    pub fn expire(&mut self, now: i64) {
        self.claims.retain(|_, c| c.expires_at > now);
    }

    /// Replaces the table with an authoritative one from the acting host.
    pub fn replace(&mut self, claims: Vec<Claim>) {
        self.claims = claims.into_iter().map(|c| (c.id, c)).collect();
    }

    /// Merges another table (after an election or a healed split). For every
    /// conflicting pair from different agents the earlier grant wins; ties go
    /// to the lower claim id. Returns `(loser, winner)` pairs.
    pub fn merge(&mut self, other: Vec<Claim>, now: i64) -> Vec<(Claim, Claim)> {
        for c in other {
            if c.expires_at > now {
                self.claims.entry(c.id).or_insert(c);
            }
        }
        let mut all = self.all();
        all.sort_by(|a, b| a.granted_at.cmp(&b.granted_at).then(a.id.cmp(&b.id)));
        let mut kept: Vec<Claim> = vec![];
        let mut lost = vec![];
        for c in all {
            match kept.iter().find(|k| !same_agent(&k.owner, &c.owner) && targets_conflict(&k.target, &c.target)) {
                Some(winner) => lost.push((c, winner.clone())),
                None => kept.push(c),
            }
        }
        for (l, _) in &lost {
            self.claims.remove(&l.id);
        }
        lost
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polyph_proto::ids::{DeviceId, MemberId};

    fn agent(n: u8) -> AgentRef {
        AgentRef {
            id: ObjectId([n; 16]),
            member: MemberId([n; 16]),
            device: DeviceId([n; 32]),
            kind: "fake".into(),
            label: format!("a{n}"),
        }
    }

    fn t(s: &str) -> ClaimTarget {
        ClaimTarget::parse(s)
    }

    fn ids() -> impl FnMut() -> ObjectId {
        let mut n = 0u8;
        move || {
            n += 1;
            ObjectId([n; 16])
        }
    }

    #[test]
    fn conflict_rules() {
        assert!(targets_conflict(&t("src/a.rs"), &t("./src//a.rs")));
        assert!(!targets_conflict(&t("src/a.rs"), &t("src/b.rs")));
        assert!(targets_conflict(&t("src/"), &t("src/a.rs")));
        assert!(targets_conflict(&t("src/a.rs"), &t("src/")));
        assert!(!targets_conflict(&t("src/"), &t("srcx/a.rs")));
        assert!(targets_conflict(&t("src/"), &t("src/deep/")));
        assert!(targets_conflict(&t("a.rs#f"), &t("a.rs#f")));
        assert!(!targets_conflict(&t("a.rs#f"), &t("a.rs#g")));
        assert!(targets_conflict(&t("a.rs"), &t("a.rs#g")));
    }

    #[test]
    fn grant_deny_renew_release() {
        let mut tab = ClaimTable::default();
        let mut id = ids();
        let r = tab.grant(&agent(1), &[t("src/a.rs")], "edit", None, 600, 100, 1, &mut id);
        assert_eq!(r.granted.len(), 1);
        let r2 = tab.grant(&agent(2), &[t("src/a.rs"), t("src/b.rs")], "edit", None, 600, 101, 1, &mut id);
        assert_eq!(r2.granted.len(), 1);
        assert_eq!(r2.denied[0].owner.id, agent(1).id);
        // Same agent re-claiming renews instead of duplicating.
        let r3 = tab.grant(&agent(1), &[t("src/a.rs")], "edit more", None, 600, 200, 1, &mut id);
        assert_eq!(r3.granted[0].id, r.granted[0].id);
        assert_eq!(tab.len(), 2);
        assert_eq!(tab.get(&r.granted[0].id).unwrap().expires_at, 800);
        // Expired claims free the file.
        let r4 = tab.grant(&agent(2), &[t("src/a.rs")], "edit", None, 600, 900, 1, &mut id);
        assert_eq!(r4.granted.len(), 1);
        tab.release(&[r4.granted[0].id]);
        assert!(tab.holder(&t("src/a.rs"), None, 900).is_none());
    }

    #[test]
    fn merge_after_split_keeps_earliest() {
        let mut a = ClaimTable::default();
        let mut b = ClaimTable::default();
        let mut ia = ids();
        let mut ib = move || ObjectId([200; 16]);
        a.grant(&agent(1), &[t("src/a.rs")], "x", None, 600, 105, 3, &mut ia);
        b.grant(&agent(2), &[t("src/a.rs")], "y", None, 600, 100, 3, &mut ib);
        let lost = a.merge(b.all(), 110);
        assert_eq!(lost.len(), 1);
        assert_eq!(lost[0].0.owner.id, agent(1).id);
        assert_eq!(lost[0].1.owner.id, agent(2).id);
        assert_eq!(a.all().len(), 1);
        assert_eq!(a.all()[0].owner.id, agent(2).id);
    }
}

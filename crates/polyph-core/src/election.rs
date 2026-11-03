//! Acting host ranking (docs/protocol.md section 10).

use std::cmp::Ordering;

use polyph_proto::cert::Role;
use polyph_proto::ids::DeviceId;

/// Seconds without a heartbeat after which a device counts as offline.
pub const ONLINE_WINDOW: i64 = 30;
/// Heartbeat and announcement interval.
pub const HEARTBEAT: i64 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub device: DeviceId,
    pub role: Role,
    pub joined_at: i64,
}

/// Higher is better: role, then earliest join, then lowest device id.
pub fn rank_cmp(a: &Candidate, b: &Candidate) -> Ordering {
    a.role.cmp(&b.role).then_with(|| b.joined_at.cmp(&a.joined_at)).then_with(|| b.device.cmp(&a.device))
}

pub fn eligible(role: Role) -> bool {
    role >= Role::Member
}

pub fn top<'a, I: IntoIterator<Item = &'a Candidate>>(cands: I) -> Option<Candidate> {
    cands.into_iter().filter(|c| eligible(c.role)).max_by(|a, b| rank_cmp(a, b)).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(d: u8, role: Role, joined: i64) -> Candidate {
        Candidate { device: DeviceId([d; 32]), role, joined_at: joined }
    }

    #[test]
    fn ranking() {
        let owner = c(9, Role::Owner, 100);
        let early_member = c(1, Role::Member, 10);
        let late_member = c(2, Role::Member, 50);
        let viewer = c(0, Role::Viewer, 0);
        assert_eq!(top(&[early_member, late_member, owner, viewer]).unwrap().device, owner.device);
        assert_eq!(top(&[late_member, early_member]).unwrap().device, early_member.device);
        assert_eq!(top(&[c(5, Role::Member, 10), c(3, Role::Member, 10)]).unwrap().device, DeviceId([3; 32]));
        assert!(top(&[viewer]).is_none());
    }
}

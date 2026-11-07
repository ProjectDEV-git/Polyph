//! Multi-device simulations of the room engine without a network.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::VecDeque;
use std::sync::Arc;

use polyph_core::{Effect, RoomCore, RoomEvent};
use polyph_proto::cert::{MembershipCert, Role, name_commitment};
use polyph_proto::delivery::seal_epoch_key;
use polyph_proto::identity::DeviceIdentity;
use polyph_proto::ids::{MemberId, ObjectId};
use polyph_proto::join::JoinApproved;
use polyph_proto::messages::{AgentRef, ApprovalKind, ApprovalScope, ClaimTarget, Decision, Knock, RoomMessage};

struct Node {
    core: RoomCore,
    online: bool,
    events: Vec<RoomEvent>,
}

struct Net {
    nodes: Vec<Node>,
    now: i64,
}

impl Net {
    fn new(owner_name: &str) -> Self {
        let id = Arc::new(DeviceIdentity::generate().unwrap());
        let (core, _) =
            RoomCore::create(id, MemberId::random().unwrap(), "payments", owner_name, false, 1_000).unwrap();
        Net { nodes: vec![Node { core, online: true, events: vec![] }], now: 1_000 }
    }

    /// Seals and delivers effects breadth-first until the network is quiet.
    fn run(&mut self, from: usize, effects: Vec<Effect>) {
        let mut q: VecDeque<(usize, Vec<u8>)> = VecDeque::new();
        self.absorb(from, effects, &mut q);
        while let Some((src, env)) = q.pop_front() {
            for i in 0..self.nodes.len() {
                if i == src || !self.nodes[i].online || !self.nodes[src].online {
                    continue;
                }
                let now = self.now;
                let out = self.nodes[i].core.receive(&env, now);
                self.absorb(i, out, &mut q);
            }
        }
    }

    fn absorb(&mut self, i: usize, effects: Vec<Effect>, q: &mut VecDeque<(usize, Vec<u8>)>) {
        for e in effects {
            match e {
                Effect::Send(m) => {
                    let env = self.nodes[i].core.seal(&m).unwrap();
                    q.push_back((i, env.encode()));
                }
                Effect::Event(ev) => self.nodes[i].events.push(ev),
            }
        }
        self.nodes[i].core.commit_rotation();
    }

    fn connect(&mut self, i: usize) {
        let now = self.now;
        let out = self.nodes[i].core.on_connected(now);
        self.run(i, out);
    }

    fn tick_all(&mut self) {
        for i in 0..self.nodes.len() {
            if self.nodes[i].online {
                let now = self.now;
                let out = self.nodes[i].core.tick(now);
                self.run(i, out);
            }
        }
    }

    fn advance(&mut self, secs: i64) {
        for _ in 0..(secs / 10).max(1) {
            self.now += 10.min(secs);
            self.tick_all();
        }
    }

    /// Joins a new device through `via`, approved by `approver`.
    fn join(&mut self, name: &str, via: usize, approver: usize, role: Role) -> usize {
        let id = Arc::new(DeviceIdentity::generate().unwrap());
        let salt = [7u8; 16];
        let knock = Knock {
            id: ObjectId::random().unwrap(),
            display_name: name.into(),
            member: MemberId::random().unwrap(),
            device: id.device_id(),
            requested_role: role,
            via: self.nodes[via].core.me,
            at: self.now,
            kx: id.public().kx.to_vec(),
            name_salt: salt.to_vec(),
            surface: None,
            capabilities: vec![],
        };
        let out = self.nodes[via].core.post_knock(knock.clone());
        self.run(via, out);
        assert!(self.nodes[approver].events.iter().any(|e| matches!(e, RoomEvent::Knock { .. })));
        let now = self.now;
        let out = self.nodes[approver].core.approve_knock(knock.id, role, now).unwrap();
        self.run(approver, out);
        let approved = self.nodes[via]
            .events
            .iter()
            .find_map(|e| match e {
                RoomEvent::JoinReady { knock: k, approved } if *k == knock.id => Some(approved.clone()),
                _ => None,
            })
            .expect("join ready on the handshake device");
        let core = RoomCore::from_join(id, &approved, name, salt, self.now).unwrap();
        self.nodes.push(Node { core, online: true, events: vec![] });
        let i = self.nodes.len() - 1;
        self.connect(i);
        let out = self.nodes[i].core.set_hue_if_unset();
        self.run(i, out);
        i
    }

    fn agent(&self, i: usize, n: u8) -> AgentRef {
        let c = &self.nodes[i].core;
        AgentRef {
            id: ObjectId([n; 16]),
            member: c.my_member,
            device: c.me,
            kind: "fake".into(),
            label: format!("agent{n}"),
        }
    }
}

#[test]
fn join_with_approval_and_chat_converges() {
    let mut net = Net::new("Ann");
    net.connect(0);
    let b = net.join("Bo", 0, 0, Role::Member);
    let c = net.join("Chai", b, 0, Role::Member);
    assert_eq!(net.nodes[c].core.role(), Role::Member);
    for i in 0..3 {
        assert_eq!(net.nodes[i].core.peers.len(), 2, "node {i} sees both others");
    }
    let now = net.now;
    let out = net.nodes[c].core.send_chat("สวัสดี, hello", None, None, now).unwrap();
    net.run(c, out);
    let out = net.nodes[0].core.send_chat("second", None, None, now + 1).unwrap();
    net.run(0, out);
    let a = net.nodes[0].core.chat();
    assert_eq!(a.len(), 2);
    assert_eq!(net.nodes[b].core.chat(), a);
    assert_eq!(net.nodes[c].core.chat(), a);
    // Hues are assigned and distinct.
    let hues: Vec<u16> = (0..3).map(|i| net.nodes[i].core.my_profile.hue).collect();
    assert!(hues.iter().all(|h| *h < 360));
    assert_ne!(hues[1], hues[2]);
}

#[test]
fn denied_knock_tells_the_handshake_device() {
    let mut net = Net::new("Ann");
    net.connect(0);
    let id = DeviceIdentity::generate().unwrap();
    let k = Knock {
        id: ObjectId::random().unwrap(),
        display_name: "Eve".into(),
        member: MemberId::random().unwrap(),
        device: id.device_id(),
        requested_role: Role::Member,
        via: net.nodes[0].core.me,
        at: 0,
        kx: id.public().kx.to_vec(),
        name_salt: vec![1; 16],
        surface: None,
        capabilities: vec![],
    };
    let out = net.nodes[0].core.post_knock(k.clone());
    net.run(0, out);
    let out = net.nodes[0].core.deny_knock(k.id).unwrap();
    net.run(0, out);
    assert!(net.nodes[0].events.iter().any(|e| matches!(e, RoomEvent::JoinDenied { .. })));
}

#[test]
fn members_cannot_approve_joins_and_viewers_cannot_write() {
    let mut net = Net::new("Ann");
    net.connect(0);
    let b = net.join("Bo", 0, 0, Role::Member);
    let v = net.join("Vee", 0, 0, Role::Viewer);
    let now = net.now;
    assert!(net.nodes[v].core.send_chat("hi", None, None, now).is_err());
    let vm = net.nodes[v].core.my_member;
    assert!(net.nodes[b].core.remove_member(vm, now).is_err());
}

#[test]
fn viewer_approval_is_ignored_and_owner_goes_first() {
    let mut net = Net::new("Ann");
    net.connect(0);
    let b = net.join("Bo", 0, 0, Role::Member);
    let v = net.join("Vee", 0, 0, Role::Viewer);
    let agent = net.agent(b, 1);
    let now = net.now;
    let (id, out) = net.nodes[b]
        .core
        .request_approval(agent, ApprovalKind::Command { command: "cargo publish".into(), cwd: ".".into() }, None, now)
        .unwrap();
    net.run(b, out);
    assert!(net.nodes[v].core.approvals.contains_key(&id));

    // The viewer forges a response message directly (its own core refuses).
    assert!(net.nodes[v].core.answer_approval(id, Decision::Approve { scope: ApprovalScope::Once }, now).is_err());
    let forged = RoomMessage::Approval(polyph_proto::messages::ApprovalMsg::Respond {
        id,
        decision: Decision::Approve { scope: ApprovalScope::Once },
        by: net.nodes[v].core.my_member,
        at: now,
    });
    net.run(v, vec![Effect::Send(forged)]);
    assert!(net.nodes[b].core.approvals.contains_key(&id), "viewer approval must be ignored");

    // The owner of the room (not the agent's owner) must wait for escalation.
    assert!(net.nodes[0].core.answer_approval(id, Decision::Approve { scope: ApprovalScope::Once }, now).is_err());
    net.now += 121;
    let now = net.now;
    let out = net.nodes[0].core.answer_approval(id, Decision::Deny { note: Some("not now".into()) }, now).unwrap();
    net.run(0, out);
    assert!(!net.nodes[b].core.approvals.contains_key(&id));
    assert!(net.nodes[b].events.iter().any(|e| matches!(e, RoomEvent::ApprovalResolved { for_me: true, .. })));
}

#[test]
fn removed_member_loses_access_after_rotation() {
    let mut net = Net::new("Ann");
    net.connect(0);
    let b = net.join("Bo", 0, 0, Role::Member);
    let c = net.join("Chai", 0, 0, Role::Member);
    let now = net.now;
    let cm = net.nodes[c].core.my_member;
    let out = net.nodes[0].core.remove_member(cm, now).unwrap();
    net.run(0, out);
    assert!(net.nodes[c].events.iter().any(|e| matches!(e, RoomEvent::YouWereRemoved)));
    assert_eq!(net.nodes[0].core.epoch(), 2);
    assert_eq!(net.nodes[b].core.epoch(), 2);
    assert_eq!(net.nodes[c].core.epoch(), 1);
    let out = net.nodes[b].core.send_chat("after removal", None, None, now).unwrap();
    net.run(b, out);
    assert_eq!(net.nodes[0].core.chat().len(), 1);
    assert_eq!(net.nodes[c].core.chat().len(), 0);
}

#[test]
fn acting_host_is_elected_and_fails_over() {
    let mut net = Net::new("Ann");
    net.connect(0);
    let b = net.join("Bo", 0, 0, Role::Maintainer);
    let c = net.join("Chai", 0, 0, Role::Member);
    net.advance(10);
    let owner = net.nodes[0].core.me;
    for i in 0..3 {
        assert_eq!(net.nodes[i].core.host.device, Some(owner), "node {i}");
    }
    // Owner goes offline; the maintainer takes over after the online window.
    net.nodes[0].online = false;
    net.advance(40);
    let maint = net.nodes[b].core.me;
    assert_eq!(net.nodes[b].core.host.device, Some(maint));
    assert_eq!(net.nodes[c].core.host.device, Some(maint));
    let term = net.nodes[c].core.host.term;
    // Owner returns and takes back the role with a higher term.
    net.nodes[0].online = true;
    net.connect(0);
    net.advance(20);
    for i in 0..3 {
        assert_eq!(net.nodes[i].core.host.device, Some(owner), "node {i} after return");
    }
    assert!(net.nodes[c].core.host.term > term);
}

#[test]
fn claim_race_has_exactly_one_winner() {
    let mut net = Net::new("Ann");
    net.connect(0);
    let b = net.join("Bo", 0, 0, Role::Member);
    let c = net.join("Chai", 0, 0, Role::Member);
    net.advance(10);
    let target = [ClaimTarget::parse("src/auth.rs")];
    let now = net.now;
    let ag = net.agent(b, 1);
    let (rb, ob) = net.nodes[b].core.request_claims(&ag, &target, "login", None, 600, now).unwrap();
    let ag = net.agent(c, 2);
    let (rc, oc) = net.nodes[c].core.request_claims(&ag, &target, "logout", None, 600, now).unwrap();
    // Both requests are in flight before either is answered.
    net.run(b, ob);
    net.run(c, oc);
    let decision = |n: &Node, r| {
        n.events.iter().find_map(|e| match e {
            RoomEvent::ClaimDecision { request, granted, denied } if *request == r => {
                Some((granted.len(), denied.clone()))
            }
            _ => None,
        })
    };
    let (gb, db) = decision(&net.nodes[b], rb).unwrap();
    let (gc, dc) = decision(&net.nodes[c], rc).unwrap();
    assert_eq!(gb + gc, 1, "exactly one winner");
    let loser_denied = if gb == 1 { dc } else { db };
    assert_eq!(loser_denied.len(), 1, "the loser learns who holds it");
    assert_eq!(net.nodes[0].core.claims.len(), 1);
    assert_eq!(net.nodes[b].core.claims.len(), 1);
}

#[test]
fn claims_reconcile_after_split() {
    let mut net = Net::new("Ann");
    net.connect(0);
    let b = net.join("Bo", 0, 0, Role::Member);
    let c = net.join("Chai", 0, 0, Role::Member);
    net.advance(10);
    // Chai is cut off and becomes acting host of its own partition.
    net.nodes[c].online = false;
    net.advance(40);
    let cme = net.nodes[c].core.me;
    let now = net.now;
    // Chai claims first, during the split.
    let out = net.nodes[c].core.tick(now);
    drop(out);
    assert_eq!(net.nodes[c].core.host.device, Some(cme));
    let target = [ClaimTarget::parse("src/db.rs")];
    let ag = net.agent(c, 2);
    let (_, oc) = net.nodes[c].core.request_claims(&ag, &target, "migrate", None, 600, now).unwrap();
    net.run(c, oc);
    net.now += 5;
    let now = net.now;
    let ag = net.agent(b, 1);
    let (_, ob) = net.nodes[b].core.request_claims(&ag, &target, "index", None, 600, now).unwrap();
    net.run(b, ob);
    assert_eq!(net.nodes[0].core.claims.len(), 1);
    assert_eq!(net.nodes[c].core.claims.len(), 1);

    // Heal.
    net.nodes[c].online = true;
    net.connect(c);
    net.advance(20);
    let winner_agent = net.agent(c, 2).id;
    for i in [0, b, c] {
        let all = net.nodes[i].core.claims.all();
        assert_eq!(all.len(), 1, "node {i} has one claim");
        assert_eq!(all[0].owner.id, winner_agent, "earlier claim wins on node {i}");
    }
    assert!(net.nodes[b].events.iter().any(|e| matches!(e, RoomEvent::ClaimRevoked { .. })), "loser notified");
    assert_eq!(net.nodes[c].core.host.device, Some(net.nodes[0].core.me));
}

#[test]
fn returning_member_catches_up_from_snapshot() {
    let mut net = Net::new("Ann");
    net.connect(0);
    let b = net.join("Bo", 0, 0, Role::Member);
    let now = net.now;
    let out = net.nodes[0].core.send_chat("before you left", None, None, now).unwrap();
    net.run(0, out);
    // Bo leaves; Ann keeps working, uploads a snapshot, then everyone leaves.
    net.nodes[b].online = false;
    for t in ["while you were away", "and more"] {
        let out = net.nodes[0].core.send_chat(t, None, None, now + 1).unwrap();
        net.run(0, out);
    }
    let (_, blob) = net.nodes[0].core.relay_snapshot().unwrap();
    // Bo comes back the next day with nobody online and reads the snapshot.
    assert!(net.nodes[b].core.apply_relay_snapshot(&blob, now + 86_400));
    assert_eq!(net.nodes[b].core.chat().len(), 3);
}

#[test]
fn save_and_load_round_trip() {
    let mut net = Net::new("Ann");
    net.connect(0);
    let b = net.join("Bo", 0, 0, Role::Member);
    let now = net.now;
    let out = net.nodes[b].core.send_chat("persist me", None, None, now).unwrap();
    net.run(b, out);
    let saved = net.nodes[0].core.save();
    let bytes = polyph_proto::codec::to_cbor(&saved).unwrap();
    let back: polyph_core::SavedRoom = polyph_proto::codec::from_cbor(&bytes).unwrap();
    let id = Arc::new(DeviceIdentity::from_secret_bytes(&net.nodes[0].core.identity().to_secret_bytes()));
    let mut core = RoomCore::load(id, back, now).unwrap();
    assert_eq!(core.chat().len(), 1);
    assert_eq!(core.peers.len(), 1);
    assert_eq!(core.role(), Role::Owner);
}

#[test]
fn snapshots_get_the_same_role_revocation_and_epoch_checks_as_live_messages() {
    let mut net = Net::new("Ann");
    net.connect(0);
    let b = net.join("Bo", 0, 0, Role::Member);
    let c = net.join("Chai", 0, 0, Role::Member);
    let d = net.join("Dao", 0, 0, Role::Member);
    let v = net.join("Vee", 0, 0, Role::Viewer);
    let now = net.now;
    // A viewer cannot write live, so its snapshot is refused too.
    let (_, viewer_snap) = net.nodes[v].core.relay_snapshot().unwrap();
    assert!(!net.nodes[b].core.apply_relay_snapshot(&viewer_snap, now));
    // A member may share docs but not removals: a forged revocation is ignored.
    let ann = net.nodes[0].core.me;
    net.nodes[b].core.revoked.insert(ann);
    let (_, member_snap) = net.nodes[b].core.relay_snapshot().unwrap();
    net.nodes[b].core.revoked.remove(&ann);
    assert!(net.nodes[c].core.apply_relay_snapshot(&member_snap, now));
    assert!(!net.nodes[c].core.revoked.contains(&ann));

    // Snapshots from before two rotations, one by a member removed since.
    let (_, old_owner) = net.nodes[0].core.relay_snapshot().unwrap();
    let (_, removed) = net.nodes[c].core.relay_snapshot().unwrap();
    let cm = net.nodes[c].core.my_member;
    let out = net.nodes[0].core.remove_member(cm, now).unwrap();
    net.run(0, out);
    assert_eq!(net.nodes[b].core.epoch(), 2);
    assert!(!net.nodes[b].core.apply_relay_snapshot(&removed, now), "removed sender");
    let dm = net.nodes[d].core.my_member;
    let out = net.nodes[0].core.remove_member(dm, now).unwrap();
    net.run(0, out);
    assert_eq!(net.nodes[b].core.epoch(), 3);
    assert!(!net.nodes[b].core.apply_relay_snapshot(&old_owner, now), "epoch too old");
    let (_, fresh) = net.nodes[0].core.relay_snapshot().unwrap();
    assert!(net.nodes[b].core.apply_relay_snapshot(&fresh, now));
}

#[test]
fn a_demotion_cannot_be_undone_by_replaying_the_old_certificate() {
    let mut net = Net::new("Ann");
    net.connect(0);
    let b = net.join("Bo", 0, 0, Role::Member);
    let c = net.join("Chai", 0, 0, Role::Member);
    let (bm, bd) = (net.nodes[b].core.my_member, net.nodes[b].core.me);
    let old = net.nodes[b].core.my_chain.clone();
    // Same second as the join, so the new certificate must still sort newer.
    let now = net.now;
    let out = net.nodes[0].core.change_role(bm, Role::Viewer, now).unwrap();
    net.run(0, out);
    assert_eq!(net.nodes[b].core.role(), Role::Viewer);
    assert_eq!(net.nodes[c].core.peers[&bd].role(), Role::Viewer);

    // Bo replays the Member chain in a hello and as a role change.
    net.nodes[b].core.my_chain = old.clone();
    let hello = net.nodes[b].core.hello(now);
    net.run(b, vec![Effect::Send(hello), Effect::Send(RoomMessage::RoleChanged { member: bm, chain: Box::new(old) })]);
    for i in [0, c] {
        assert_eq!(net.nodes[i].core.peers[&bd].role(), Role::Viewer, "node {i}");
    }
    // A later promotion still applies.
    net.now += 1;
    let now = net.now;
    let out = net.nodes[0].core.change_role(bm, Role::Member, now).unwrap();
    net.run(0, out);
    assert_eq!(net.nodes[c].core.peers[&bd].role(), Role::Member);

    // Bo becomes a Maintainer, lets Dee in, and is demoted again.
    net.now += 1;
    let now = net.now;
    let out = net.nodes[0].core.change_role(bm, Role::Maintainer, now).unwrap();
    net.run(0, out);
    assert_eq!(net.nodes[b].core.role(), Role::Maintainer);
    let maint = net.nodes[b].core.my_chain.clone();
    let dee = net.join("Dee", b, b, Role::Member);
    let dd = net.nodes[dee].core.me;
    net.now += 1;
    let now = net.now;
    let out = net.nodes[0].core.change_role(bm, Role::Member, now).unwrap();
    net.run(0, out);
    assert_eq!(net.nodes[c].core.peers[&bd].role(), Role::Member);
    // Dee, whom Bo let in before the demotion, is re-issued and stays.
    assert_eq!(net.nodes[dee].core.my_chain.certs.len(), 2, "re-issued from the owner's chain");
    let hello = net.nodes[dee].core.hello(now);
    net.run(dee, vec![Effect::Send(hello)]);
    assert_eq!(net.nodes[c].core.peers[&dd].role(), Role::Member);

    // 1. Bo extends its old Maintainer chain with a certificate for itself, dated later.
    let bo = net.nodes[b].core.identity();
    let held = maint.verify(now, &|_| false).unwrap();
    let restamp = MembershipCert {
        room: held.room,
        member: bm,
        device: held.device,
        name_commit: held.name_commit,
        role: Role::Maintainer,
        // Later than the demotion's certificate, which would win on date alone.
        issued_at: now + 60,
        expires_at: None,
        capabilities: vec![],
    };
    let restamped = maint.issue(bo, &restamp).unwrap();
    net.nodes[b].core.my_chain = restamped.clone();
    let hello = net.nodes[b].core.hello(now);
    net.run(
        b,
        vec![Effect::Send(hello), Effect::Send(RoomMessage::RoleChanged { member: bm, chain: Box::new(restamped) })],
    );
    for i in [0, c, dee] {
        assert_eq!(net.nodes[i].core.peers[&bd].role(), Role::Member, "self-issued, node {i}");
    }

    // 2. Bo issues a Maintainer certificate for a fresh device from the old chain.
    let fresh = Arc::new(DeviceIdentity::generate().unwrap());
    let cert = MembershipCert {
        room: held.room,
        member: MemberId::random().unwrap(),
        device: fresh.public(),
        name_commit: name_commitment("Eve", &[7; 16]),
        role: Role::Maintainer,
        issued_at: now,
        expires_at: None,
        capabilities: vec![],
    };
    let forged = maint.issue(net.nodes[b].core.identity(), &cert).unwrap();
    let key = seal_epoch_key(net.nodes[b].core.identity(), &fresh.public(), net.nodes[b].core.current_key().unwrap())
        .unwrap();
    let approved = JoinApproved {
        room: held.room,
        room_name: None,
        role: Role::Maintainer,
        chain: forged.clone(),
        key,
        relay: None,
    };
    net.run(b, vec![Effect::Send(RoomMessage::MemberAdded { chain: Box::new(forged) })]);
    let core = RoomCore::from_join(fresh.clone(), &approved, "Eve", [7; 16], now).unwrap();
    net.nodes.push(Node { core, online: true, events: vec![] });
    let eve = net.nodes.len() - 1;
    net.connect(eve);
    for i in [0, c, dee] {
        assert!(!net.nodes[i].core.peers.contains_key(&fresh.device_id()), "fresh device, node {i}");
    }
}

#[test]
fn knocks_cannot_claim_an_existing_member() {
    let mut net = Net::new("Ann");
    net.connect(0);
    let b = net.join("Bo", 0, 0, Role::Member);
    let bm = net.nodes[b].core.my_member;
    let knock = |id: &DeviceIdentity, member: MemberId, net: &Net| Knock {
        id: ObjectId::random().unwrap(),
        display_name: "Bo".into(),
        member,
        device: id.device_id(),
        requested_role: Role::Member,
        via: net.nodes[0].core.me,
        at: net.now,
        kx: id.public().kx.to_vec(),
        name_salt: vec![7; 16],
        surface: None,
        capabilities: vec![],
    };
    let now = net.now;
    // A new device claiming Bo's (or the owner's) member id is refused.
    for member in [bm, net.nodes[0].core.my_member] {
        let k = knock(&DeviceIdentity::generate().unwrap(), member, &net);
        let out = net.nodes[0].core.post_knock(k.clone());
        net.run(0, out);
        assert!(net.nodes[0].core.approve_knock(k.id, Role::Member, now).is_err());
    }
    // A brand-new member is still approved as before.
    let k = knock(&DeviceIdentity::generate().unwrap(), MemberId::random().unwrap(), &net);
    let out = net.nodes[0].core.post_knock(k.clone());
    net.run(0, out);
    assert!(net.nodes[0].core.approve_knock(k.id, Role::Member, now).is_ok());
}

#[test]
fn a_member_can_add_their_own_device_through_a_code_they_made() {
    let mut net = Net::new("Ann");
    net.connect(0);
    let b = net.join("Bo", 0, 0, Role::Member);
    let (am, bm) = (net.nodes[0].core.my_member, net.nodes[b].core.my_member);
    let now = net.now;
    let knock = |member: MemberId, net: &Net| {
        let id = DeviceIdentity::generate().unwrap();
        Knock {
            id: ObjectId::random().unwrap(),
            display_name: "Ann".into(),
            member,
            device: id.device_id(),
            requested_role: Role::Member,
            via: net.nodes[0].core.me,
            at: net.now,
            kx: id.public().kx.to_vec(),
            name_salt: vec![7; 16],
            surface: None,
            capabilities: vec![],
        }
    };
    // Ann marks the knock that came through her own pairing code: allowed.
    let k = knock(am, &net);
    let out = net.nodes[0].core.post_knock(k.clone());
    net.run(0, out);
    net.nodes[0].core.allow_own_device(k.id);
    assert!(net.nodes[0].core.approve_knock(k.id, Role::Member, now).is_ok());
    // The mark covers only the approver's own member: Bo's id is still refused.
    let k = knock(bm, &net);
    let out = net.nodes[0].core.post_knock(k.clone());
    net.run(0, out);
    net.nodes[0].core.allow_own_device(k.id);
    assert!(net.nodes[0].core.approve_knock(k.id, Role::Member, now).is_err());
}

#[test]
fn a_device_rejoining_must_present_the_key_its_certificate_names() {
    let mut net = Net::new("Ann");
    net.connect(0);
    let b = net.join("Bo", 0, 0, Role::Member);
    let (bm, bd) = (net.nodes[b].core.my_member, net.nodes[b].core.me);
    let bo_kx = net.nodes[0].core.peers[&bd].verified.as_ref().unwrap().device.kx.to_vec();
    let now = net.now;
    let knock = |kx: Vec<u8>, net: &Net| Knock {
        id: ObjectId::random().unwrap(),
        display_name: "Bo".into(),
        member: bm,
        device: bd,
        requested_role: Role::Member,
        via: net.nodes[0].core.me,
        at: net.now,
        kx,
        name_salt: vec![7; 16],
        surface: None,
        capabilities: vec![],
    };
    // Bo's device id is public; with another encryption key the knock is refused.
    let k = knock(DeviceIdentity::generate().unwrap().public().kx.to_vec(), &net);
    let out = net.nodes[0].core.post_knock(k.clone());
    net.run(0, out);
    assert!(net.nodes[0].core.knock_claims_member(&k), "never approved automatically");
    assert!(net.nodes[0].core.approve_knock(k.id, Role::Member, now).is_err());
    // With the key its certificate names, the same device may re-join.
    let k = knock(bo_kx, &net);
    let out = net.nodes[0].core.post_knock(k.clone());
    net.run(0, out);
    assert!(net.nodes[0].core.knock_claims_member(&k), "never approved automatically");
    assert!(net.nodes[0].core.approve_knock(k.id, Role::Member, now).is_ok());
}

#[test]
fn an_own_device_mark_is_cleared_once_the_knock_is_decided() {
    let mut net = Net::new("Ann");
    net.connect(0);
    let am = net.nodes[0].core.my_member;
    let now = net.now;
    let knock = |net: &Net| {
        let id = DeviceIdentity::generate().unwrap();
        Knock {
            id: ObjectId::random().unwrap(),
            display_name: "Ann".into(),
            member: am,
            device: id.device_id(),
            requested_role: Role::Member,
            via: net.nodes[0].core.me,
            at: net.now,
            kx: id.public().kx.to_vec(),
            name_salt: vec![7; 16],
            surface: None,
            capabilities: vec![],
        }
    };
    let approved = knock(&net);
    let denied = knock(&net);
    for k in [&approved, &denied] {
        let out = net.nodes[0].core.post_knock(k.clone());
        net.run(0, out);
        net.nodes[0].core.allow_own_device(k.id);
        assert!(net.nodes[0].core.is_own_device_knock(&k.id));
    }
    let out = net.nodes[0].core.approve_knock(approved.id, Role::Member, now).unwrap();
    net.run(0, out);
    let out = net.nodes[0].core.deny_knock(denied.id).unwrap();
    net.run(0, out);
    assert!(!net.nodes[0].core.is_own_device_knock(&approved.id));
    assert!(!net.nodes[0].core.is_own_device_knock(&denied.id));
}

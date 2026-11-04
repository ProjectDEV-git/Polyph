//! Read models shared by every device type: the daemon's local API and the
//! browser member (`polyph-web`) answer with these, so every surface sees the
//! same shapes.

use std::collections::{HashMap, HashSet};

use polyph_proto::cert::Role;
use polyph_proto::crdt::{Activity, ActivityEntry, Board};
use polyph_proto::ids::MemberId;
use polyph_proto::messages::DocName;
use serde::Serialize;
use serde_json::{Value, json};

use crate::room::RoomCore;

#[derive(Debug, Clone, Serialize)]
pub struct MemberInfo {
    pub member: String,
    pub name: String,
    pub role: Role,
    pub online: bool,
    pub hue: u16,
    pub devices: usize,
    pub me: bool,
}

/// One row per person, with how many devices they have in the room.
pub fn members(core: &RoomCore, now: i64) -> Vec<MemberInfo> {
    let mut by: HashMap<MemberId, MemberInfo> = HashMap::new();
    by.insert(
        core.my_member,
        MemberInfo {
            member: core.my_member.to_hex(),
            name: core.my_profile.display_name.clone(),
            role: core.role(),
            online: true,
            hue: core.my_profile.hue,
            devices: 1,
            me: true,
        },
    );
    for p in core.peers.values() {
        let Some(m) = p.member() else { continue };
        let e = by.entry(m).or_insert(MemberInfo {
            member: m.to_hex(),
            name: p.name(),
            role: p.role(),
            online: false,
            hue: p.profile.as_ref().map(|x| x.hue).unwrap_or(u16::MAX),
            devices: 0,
            me: false,
        });
        e.devices += 1;
        e.online |= p.online(now);
        if e.name.is_empty() {
            e.name = p.name();
        }
        if e.hue == u16::MAX {
            e.hue = p.profile.as_ref().map(|x| x.hue).unwrap_or(u16::MAX);
        }
        e.role = e.role.max(p.role());
    }
    let mut v: Vec<MemberInfo> = by.into_values().collect();
    v.sort_by(|a, b| b.role.cmp(&a.role).then(a.name.cmp(&b.name)));
    v
}

/// People online now (this device counts as one).
pub fn members_online(core: &RoomCore, now: i64) -> usize {
    core.peers
        .values()
        .filter(|p| p.online(now))
        .filter_map(|p| p.member())
        .filter(|m| *m != core.my_member)
        .collect::<HashSet<_>>()
        .len()
        + 1
}

/// Members known to the room (this person included).
pub fn member_count(core: &RoomCore) -> usize {
    let mut s: HashSet<MemberId> = core.peers.values().filter_map(|p| p.member()).collect();
    s.insert(core.my_member);
    s.len()
}

pub fn member_names(core: &RoomCore) -> HashMap<String, String> {
    let mut m: HashMap<String, String> =
        core.peers.values().filter_map(|p| p.member().map(|id| (id.to_hex(), p.name()))).collect();
    m.insert(core.my_member.to_hex(), core.my_profile.display_name.clone());
    m
}

/// The last `limit` chat messages (room chat and thread posts), oldest first.
pub fn chat(core: &mut RoomCore, limit: Option<usize>) -> Vec<Value> {
    let mut v = core.chat();
    let names = member_names(core);
    if let Some(l) = limit {
        let skip = v.len().saturating_sub(l);
        v.drain(..skip);
    }
    v.into_iter()
        .map(|e| json!({ "id": e.id, "author": e.author, "name": names.get(&e.author).cloned().unwrap_or_default(), "agent": e.agent, "thread": e.thread, "text": e.text, "at": e.at }))
        .collect()
}

pub fn board(core: &mut RoomCore) -> Value {
    let b = Board::new(core.doc(&DocName::Board));
    let cols = b.columns();
    let cards = b.all_cards();
    json!({ "columns": cols.into_iter().map(|(id, title)| json!({"id": id, "title": title})).collect::<Vec<_>>(), "cards": cards })
}

pub fn threads(core: &mut RoomCore) -> Vec<Value> {
    let threads = crate::docs::threads(core.doc(&DocName::Threads));
    let chat = core.chat();
    threads
        .iter()
        .map(|t| {
            let posts: Vec<_> = chat.iter().filter(|m| m.thread.as_deref() == Some(t.id.as_str())).collect();
            json!({ "thread": t, "posts": posts.len(), "last_at": posts.last().map(|m| m.at) })
        })
        .collect()
}

pub fn activity(core: &mut RoomCore, since: Option<i64>, kinds: &[String], limit: Option<usize>) -> Vec<ActivityEntry> {
    let mut v: Vec<ActivityEntry> = Activity::new(core.doc(&DocName::Activity))
        .entries()
        .into_iter()
        .filter(|e| since.is_none_or(|s| e.at >= s))
        .filter(|e| kinds.is_empty() || kinds.contains(&e.kind))
        .map(|mut e| {
            // Checkpoints are for replay, not for reading.
            e.checkpoint = e.checkpoint.map(|_| String::new());
            e
        })
        .collect();
    if let Some(l) = limit {
        let skip = v.len().saturating_sub(l);
        v.drain(..skip);
    }
    v
}

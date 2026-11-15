//! The phone's answers to the app's API calls. Method names, parameters and
//! reply shapes match the daemon's (`polyph-daemon/src/daemon.rs`), so the
//! shared React app (`packages/ui`) runs unchanged on top of either. Methods
//! that need a daemon (agents, terminals, merges, integrations) answer with
//! an "unsupported" error the app shows in place.

use std::collections::{HashMap, HashSet};

use polyph_core::room::{NO_HUE, approval_title};
use polyph_core::voice::{Step, Theme, voice_css};
use polyph_proto::cert::Role;
use polyph_proto::crdt::{Activity, ActivityEntry, Board, NewCard};
use polyph_proto::ids::{DeviceId, MemberId, ObjectId};
use polyph_proto::messages::{AgentEventKind, AgentState, ApprovalScope, Decision, DocName, HostMsg, RoomMessage};
use polyph_proto::status::{StatusAgent, StatusApproval, StatusSnapshot};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::{Output, Phone, PhoneError, Result, Room, apply};

fn param<T: DeserializeOwned>(v: &Value) -> Result<T> {
    serde_json::from_value(v.clone()).map_err(|e| PhoneError::Bad(format!("bad parameters: {e}")))
}

#[derive(Deserialize)]
struct RoomParam {
    room: String,
}

fn room<'a>(p: &'a mut Phone, id: &str) -> Result<&'a mut Room> {
    let want = id.to_lowercase();
    match &mut p.room {
        Some(r) if !want.is_empty() && r.id_hex().starts_with(&want) => Ok(r),
        _ => Err(PhoneError::NotFound("no such room on this phone".into())),
    }
}

fn room_of<'a>(p: &'a mut Phone, params: &Value) -> Result<&'a mut Room> {
    let rp: RoomParam = param(params)?;
    room(p, &rp.room)
}

pub fn member_names(r: &Room) -> HashMap<String, String> {
    let mut m: HashMap<String, String> =
        r.core.peers.values().filter_map(|p| p.member().map(|id| (id.to_hex(), p.name()))).collect();
    m.insert(r.core.my_member.to_hex(), r.core.my_profile.display_name.clone());
    m
}

fn room_info(p: &Phone, r: &Room, now: i64) -> Value {
    let online: HashSet<MemberId> =
        r.core.peers.values().filter(|x| x.online(now)).filter_map(|x| x.member()).collect();
    let members: HashSet<MemberId> =
        r.core.peers.values().filter_map(|x| x.member()).filter(|m| *m != r.core.my_member).collect();
    json!({
        "id": r.id_hex(),
        "name": r.core.name,
        "quick": r.core.quick,
        "role": r.core.role(),
        "relay": p.relay,
        "connected": p.is_open(),
        "open": true,
        "members_online": online.iter().filter(|m| **m != r.core.my_member).count() + 1,
        "members": members.len() + 1,
        "epoch": r.core.epoch(),
        "lan": Value::Null,
    })
}

fn agent_state(log: &std::collections::VecDeque<polyph_proto::messages::AgentEvent>) -> Option<AgentState> {
    log.iter().rev().find_map(|e| match &e.kind {
        AgentEventKind::State { state, .. } => Some(*state),
        AgentEventKind::Stopped { .. } => Some(AgentState::Stopped),
        _ => None,
    })
}

/// The status snapshot the pill and the "Needs you" stack read.
pub fn status(p: &mut Phone, theme: Theme, now: i64) -> StatusSnapshot {
    let open = p.is_open();
    let Some(r) = &mut p.room else {
        return StatusSnapshot {
            room: None,
            room_name: None,
            connected: open,
            members_online: 0,
            agents: vec![],
            approvals: vec![],
            budgets: vec![],
        };
    };
    let names = member_names(r);
    let cards = Board::new(r.core.doc(&DocName::Board)).all_cards();
    let hue_of = |m: &str| -> u16 {
        if r.core.my_member.to_hex() == m {
            return r.core.my_profile.hue;
        }
        r.core
            .peers
            .values()
            .find(|x| x.member().is_some_and(|id| id.to_hex() == m))
            .and_then(|x| x.profile.as_ref().map(|pr| pr.hue))
            .unwrap_or(NO_HUE)
    };
    let mut per_member: HashMap<String, u8> = HashMap::new();
    let mut logs: Vec<_> = r.transcripts.values().filter_map(|l| l.front().map(|f| (f.at, l))).collect();
    logs.sort_by_key(|(at, _)| *at);
    let mut agents = vec![];
    for (_, log) in logs {
        let Some(first) = log.front() else { continue };
        let a = &first.agent;
        let member = a.member.to_hex();
        let mut task = None;
        let mut model = None;
        for e in log {
            match &e.kind {
                AgentEventKind::Started { task: t, model: m, .. } => {
                    task = t.map(|t| t.to_hex());
                    model = m.clone();
                }
                _ => {}
            }
        }
        let state = agent_state(log).unwrap_or(AgentState::Starting);
        let n = per_member.entry(member.clone()).or_insert(0);
        *n = n.saturating_add(1);
        if state == AgentState::Stopped {
            continue;
        }
        let hue = hue_of(&member);
        agents.push(StatusAgent {
            id: a.id.to_hex(),
            label: a.label.clone(),
            owner: names.get(&member).cloned().unwrap_or_default(),
            color: if hue == NO_HUE { String::new() } else { voice_css(hue, Step::Agent(*n - 1), theme) },
            state,
            task: task.and_then(|t| cards.iter().find(|c| c.id == t).map(|c| c.title.clone())),
            model,
            mine: false,
            cloud: r.core.is_agent_host(&a.device),
        });
    }
    let approvals = r
        .core
        .approvals
        .values()
        .map(|a| StatusApproval {
            id: a.id.to_hex(),
            agent: a.agent.label.clone(),
            title: approval_title(&a.kind),
            mine: a.agent.member == r.core.my_member,
            created_at: a.created_at,
        })
        .collect();
    let online: HashSet<MemberId> =
        r.core.peers.values().filter(|x| x.online(now)).filter_map(|x| x.member()).collect();
    StatusSnapshot {
        room: Some(r.id_hex()),
        room_name: Some(r.core.name.clone()),
        connected: open,
        members_online: (online.iter().filter(|m| **m != r.core.my_member).count() + 1) as u32,
        agents,
        approvals,
        budgets: vec![],
    }
}

fn needs_daemon(method: &str) -> PhoneError {
    PhoneError::Unsupported(format!("{method} needs a computer running Polyph; it is not available on this phone"))
}

pub fn call(p: &mut Phone, method: &str, params: &Value, now: i64, out: &mut Output) -> Result<Value> {
    match method {
        "ping" => Ok(json!({ "version": env!("CARGO_PKG_VERSION"), "protocol": polyph_proto::PROTOCOL_VERSION })),
        "whoami" => Ok(json!({
            "device": p.device_hex(),
            "member": p.member.to_hex(),
            "name": p.name,
            "keystore": "browser",
            "relay": p.relay,
        })),
        "room.list" => {
            let open = &*p;
            Ok(match &open.room {
                Some(r) => json!([room_info(open, r, now)]),
                None => json!([]),
            })
        }
        "room.get" => {
            let rp: RoomParam = param(params)?;
            room(p, &rp.room)?;
            let ph = &*p;
            let r = ph.room.as_ref().ok_or_else(|| PhoneError::NotFound("no such room".into()))?;
            Ok(room_info(ph, r, now))
        }
        "room.open" => {
            room_of(p, params)?;
            Ok(json!({}))
        }
        "room.leave" => {
            room_of(p, params)?;
            let o = p.forget_room();
            out.frames.extend(o.frames);
            out.dirty = true;
            Ok(json!({}))
        }
        "room.members" => {
            let r = room_of(p, params)?;
            #[derive(serde::Serialize)]
            struct M {
                member: String,
                name: String,
                role: Role,
                online: bool,
                hue: u16,
                devices: usize,
                me: bool,
            }
            let mut by: HashMap<MemberId, M> = HashMap::new();
            by.insert(
                r.core.my_member,
                M {
                    member: r.core.my_member.to_hex(),
                    name: r.core.my_profile.display_name.clone(),
                    role: r.core.role(),
                    online: true,
                    hue: r.core.my_profile.hue,
                    devices: 1,
                    me: true,
                },
            );
            for x in r.core.peers.values() {
                let Some(m) = x.member() else { continue };
                let e = by.entry(m).or_insert(M {
                    member: m.to_hex(),
                    name: x.name(),
                    role: x.role(),
                    online: false,
                    hue: x.profile.as_ref().map(|pr| pr.hue).unwrap_or(NO_HUE),
                    devices: 0,
                    me: false,
                });
                if !e.me {
                    e.devices += 1;
                }
                e.online |= x.online(now);
                if e.name.is_empty() {
                    e.name = x.name();
                }
                if e.hue == NO_HUE {
                    e.hue = x.profile.as_ref().map(|pr| pr.hue).unwrap_or(NO_HUE);
                }
                e.role = e.role.max(x.role());
            }
            let mut v: Vec<M> = by.into_values().collect();
            v.sort_by(|a, b| b.role.cmp(&a.role).then(a.name.cmp(&b.name)));
            Ok(serde_json::to_value(v).unwrap_or_default())
        }
        "knock.list" => {
            let r = room_of(p, params)?;
            if !r.core.role().can_approve_joins() {
                return Err(PhoneError::Forbidden("only Owners and Maintainers see join requests".into()));
            }
            Ok(serde_json::to_value(r.core.knocks.values().collect::<Vec<_>>()).unwrap_or_default())
        }
        "knock.decide" => {
            #[derive(Deserialize)]
            struct P {
                room: String,
                knock: String,
                approve: bool,
                role: Option<Role>,
            }
            let q: P = param(params)?;
            let r = room(p, &q.room)?;
            let id = r
                .core
                .knocks
                .keys()
                .find(|k| k.to_hex().starts_with(&q.knock.to_lowercase()))
                .copied()
                .ok_or_else(|| PhoneError::NotFound("no such knock".into()))?;
            let fx = if q.approve {
                r.core.approve_knock(id, q.role.unwrap_or(Role::Member), now)?
            } else {
                r.core.deny_knock(id)?
            };
            apply(r, fx, out);
            Ok(json!({}))
        }
        "chat.send" => {
            #[derive(Deserialize)]
            struct P {
                room: String,
                text: String,
                thread: Option<String>,
            }
            let q: P = param(params)?;
            if q.text.trim().is_empty() {
                return Err(PhoneError::Bad("message is empty".into()));
            }
            let r = room(p, &q.room)?;
            let fx = r.core.send_chat(&q.text, q.thread, None, now)?;
            apply(r, fx, out);
            Ok(json!({}))
        }
        "chat.list" => {
            #[derive(Deserialize)]
            struct P {
                room: String,
                limit: Option<usize>,
            }
            let q: P = param(params)?;
            let r = room(p, &q.room)?;
            let names = member_names(r);
            let mut v = r.core.chat();
            if let Some(l) = q.limit {
                let skip = v.len().saturating_sub(l);
                v.drain(..skip);
            }
            Ok(Value::Array(
                v.into_iter()
                    .map(|e| json!({ "id": e.id, "author": e.author, "name": names.get(&e.author).cloned().unwrap_or_default(), "agent": e.agent, "thread": e.thread, "text": e.text, "at": e.at }))
                    .collect(),
            ))
        }
        "thread.list" => {
            let r = room_of(p, params)?;
            let threads = polyph_core::docs::threads(r.core.doc(&DocName::Threads));
            let chat = r.core.chat();
            Ok(Value::Array(
                threads
                    .iter()
                    .map(|t| {
                        let posts: Vec<_> =
                            chat.iter().filter(|m| m.thread.as_deref() == Some(t.id.as_str())).collect();
                        json!({ "thread": t, "posts": posts.len(), "last_at": posts.last().map(|m| m.at) })
                    })
                    .collect(),
            ))
        }
        "thread.get" => {
            #[derive(Deserialize)]
            struct P {
                room: String,
                thread: String,
            }
            let q: P = param(params)?;
            let r = room(p, &q.room)?;
            let want = q.thread.to_lowercase();
            let t = polyph_core::docs::threads(r.core.doc(&DocName::Threads))
                .into_iter()
                .find(|t| t.id.starts_with(&want))
                .ok_or_else(|| PhoneError::NotFound("no such thread".into()))?;
            let names = member_names(r);
            let posts: Vec<Value> = r
                .core
                .chat()
                .into_iter()
                .filter(|m| m.thread.as_deref() == Some(t.id.as_str()))
                .map(|m| {
                    let name = names.get(&m.author).cloned().unwrap_or_default();
                    json!({ "id": m.id, "author": m.author, "name": name, "agent": m.agent, "text": m.text, "at": m.at })
                })
                .collect();
            Ok(json!({ "thread": t, "posts": posts }))
        }
        "board.get" => {
            let r = room_of(p, params)?;
            let b = Board::new(r.core.doc(&DocName::Board));
            let cols = b.columns();
            let cards = b.all_cards();
            Ok(json!({
                "columns": cols.into_iter().map(|(id, title)| json!({"id": id, "title": title})).collect::<Vec<_>>(),
                "cards": cards,
            }))
        }
        "board.add" => {
            #[derive(Deserialize)]
            struct P {
                room: String,
                title: String,
                #[serde(default)]
                description: String,
                column: Option<String>,
                #[serde(default)]
                acceptance: Vec<String>,
                #[serde(default)]
                files: Vec<String>,
            }
            let q: P = param(params)?;
            if q.title.trim().is_empty() {
                return Err(PhoneError::Bad("a card needs a title".into()));
            }
            let r = room(p, &q.room)?;
            let id = ObjectId::random()?.to_hex();
            let card = NewCard {
                id: id.clone(),
                title: q.title,
                description: q.description,
                column: q.column.unwrap_or_else(|| "backlog".into()),
                created_by: r.core.my_member.to_hex(),
                created_at: now,
                acceptance: q.acceptance,
                files: q.files,
                ..Default::default()
            };
            let fx = r.core.change_doc(DocName::Board, |d| {
                let _ = Board::new(d).add_card(&card);
            })?;
            apply(r, fx, out);
            Ok(json!({ "id": id }))
        }
        "board.move" => {
            #[derive(Deserialize)]
            struct P {
                room: String,
                card: String,
                column: String,
                after: Option<String>,
            }
            let q: P = param(params)?;
            let r = room(p, &q.room)?;
            let card = Board::new(r.core.doc(&DocName::Board))
                .all_cards()
                .into_iter()
                .find(|x| x.id.starts_with(&q.card))
                .ok_or_else(|| PhoneError::NotFound("no such card".into()))?
                .id;
            let mut res = Ok(());
            let fx = r.core.change_doc(DocName::Board, |d| {
                res = Board::new(d).move_card(&card, &q.column, q.after.as_deref(), now)
            })?;
            res.map_err(|e| PhoneError::Bad(e.to_string()))?;
            apply(r, fx, out);
            Ok(json!({}))
        }
        "activity.list" => {
            #[derive(Deserialize)]
            struct P {
                room: String,
                since: Option<i64>,
                #[serde(default)]
                kinds: Vec<String>,
                limit: Option<usize>,
            }
            let q: P = param(params)?;
            let r = room(p, &q.room)?;
            let mut v: Vec<ActivityEntry> = Activity::new(r.core.doc(&DocName::Activity))
                .entries()
                .into_iter()
                .filter(|e| q.since.is_none_or(|s| e.at >= s))
                .filter(|e| q.kinds.is_empty() || q.kinds.contains(&e.kind))
                .map(|mut e| {
                    e.checkpoint = e.checkpoint.map(|_| String::new());
                    e
                })
                .collect();
            if let Some(l) = q.limit {
                let skip = v.len().saturating_sub(l);
                v.drain(..skip);
            }
            Ok(serde_json::to_value(v).unwrap_or_default())
        }
        "whiteboard.list" => {
            let r = room_of(p, params)?;
            Ok(serde_json::to_value(r.core.whiteboards()).unwrap_or_default())
        }
        "approvals.list" => {
            let r = room_of(p, params)?;
            Ok(serde_json::to_value(r.core.approvals.values().collect::<Vec<_>>()).unwrap_or_default())
        }
        "approve" | "deny" => {
            #[derive(Deserialize)]
            struct P {
                room: String,
                id: String,
                #[serde(default)]
                task: bool,
                note: Option<String>,
            }
            let q: P = param(params)?;
            let r = room(p, &q.room)?;
            let id = r
                .core
                .approvals
                .keys()
                .find(|k| k.to_hex().starts_with(&q.id.to_lowercase()))
                .copied()
                .ok_or_else(|| PhoneError::NotFound("no such approval".into()))?;
            let decision = if method == "approve" {
                Decision::Approve { scope: if q.task { ApprovalScope::Task } else { ApprovalScope::Once } }
            } else {
                Decision::Deny { note: q.note }
            };
            let what = r.core.approvals.get(&id).map(|a| (a.agent.label.clone(), approval_title(&a.kind)));
            let approved = matches!(decision, Decision::Approve { .. });
            let fx = r.core.answer_approval(id, decision, now)?;
            apply(r, fx, out);
            if let Some((agent, title)) = what {
                log(
                    r,
                    "approval",
                    json!({ "id": id.to_hex(), "approved": approved, "title": title, "agent": agent }),
                    now,
                    out,
                );
            }
            Ok(json!({}))
        }
        "agent.list" => {
            let r = room_of(p, params)?;
            Ok(Value::Array(
                r.transcripts
                    .iter()
                    .filter_map(|(id, log)| {
                        let first = log.front()?;
                        Some(json!({
                            "id": id.to_hex(),
                            "label": first.agent.label,
                            "kind": first.agent.kind,
                            "member": first.agent.member.to_hex(),
                            "mine": false,
                            "state": agent_state(log),
                        }))
                    })
                    .collect(),
            ))
        }
        "agent.transcript" => {
            #[derive(Deserialize)]
            struct P {
                room: String,
                agent: String,
                after: Option<u64>,
            }
            let q: P = param(params)?;
            let r = room(p, &q.room)?;
            let id = r
                .transcripts
                .keys()
                .find(|k| k.to_hex().starts_with(&q.agent.to_lowercase()))
                .copied()
                .ok_or_else(|| PhoneError::NotFound("no such agent".into()))?;
            let events: Vec<_> =
                r.transcripts[&id].iter().filter(|e| q.after.is_none_or(|a| e.seq > a)).cloned().collect();
            Ok(json!({ "events": events, "summary": Value::Null }))
        }
        "agent.hosts" => {
            let r = room_of(p, params)?;
            Ok(Value::Array(hosts(r, now)))
        }
        "agent.start" => {
            // A phone runs no agents itself, but can start them on its
            // person's agent hosts (their own servers).
            #[derive(Deserialize)]
            struct P {
                room: String,
                kind: String,
                on: Option<String>,
                preset: Option<String>,
                task: Option<String>,
                prompt: Option<String>,
            }
            // Without a host, the agent would run here: a phone runs none.
            if params.get("on").and_then(Value::as_str).is_none_or(str::is_empty) {
                return Err(needs_daemon("agent.start"));
            }
            let q: P = param(params)?;
            let on = q.on.unwrap_or_default();
            let r = room(p, &q.room)?;
            let list = hosts(r, now);
            let ql = on.to_lowercase();
            let pick = list
                .iter()
                .filter(|h| h["online"] == true)
                .filter(|h| ql == "cloud" || h["device"].as_str().is_some_and(|d| d.starts_with(&ql)))
                .min_by_key(|h| h["agents"].as_u64().unwrap_or(0))
                .and_then(|h| h["device"].as_str())
                .and_then(|d| DeviceId::from_hex(d).ok())
                .ok_or_else(|| {
                    PhoneError::NotFound(if list.is_empty() {
                        "you have no agent host in this room; add one with `polyph agent-host invite`".into()
                    } else {
                        "none of your agent hosts is online".into()
                    })
                })?;
            let task = match q.task.as_deref() {
                Some(t) => Some(ObjectId::from_hex(t).map_err(|_| PhoneError::Bad("task must be a card id".into()))?),
                None => None,
            };
            let req = ObjectId::random()?;
            let msg = HostMsg::Start { req, host: pick, kind: q.kind, preset: q.preset, task, prompt: q.prompt };
            apply(r, vec![polyph_core::Effect::Send(RoomMessage::Host(msg))], out);
            Ok(json!({ "pending": true, "host": pick.to_hex() }))
        }
        "agent.prompt" | "agent.stop" => {
            #[derive(Deserialize)]
            struct P {
                agent: String,
                text: Option<String>,
            }
            let q: P = param(params)?;
            let r = p.room.as_mut().ok_or_else(|| PhoneError::NotFound("no such agent".into()))?;
            let ql = q.agent.to_lowercase();
            let a = r
                .transcripts
                .values()
                .filter_map(|l| l.front().map(|e| e.agent.clone()))
                .find(|a| a.id.to_hex().starts_with(&ql) || a.label.to_lowercase() == ql)
                .ok_or_else(|| PhoneError::NotFound("no such agent".into()))?;
            if a.member != r.core.my_member || !r.core.is_agent_host(&a.device) {
                return Err(needs_daemon(method));
            }
            let msg = match (method, q.text) {
                ("agent.prompt", Some(text)) => HostMsg::Prompt { host: a.device, agent: a.id, text },
                ("agent.prompt", None) => return Err(PhoneError::Bad("a prompt needs text".into())),
                _ => HostMsg::Stop { host: a.device, agent: a.id },
            };
            apply(r, vec![polyph_core::Effect::Send(RoomMessage::Host(msg))], out);
            Ok(json!({ "forwarded": true }))
        }
        "inbox.send" => {
            #[derive(Deserialize)]
            struct P {
                room: String,
                agent: String,
                text: String,
            }
            let q: P = param(params)?;
            let r = room(p, &q.room)?;
            let id = r
                .transcripts
                .keys()
                .find(|k| k.to_hex().starts_with(&q.agent.to_lowercase()))
                .copied()
                .ok_or_else(|| PhoneError::NotFound("no such agent".into()))?;
            let to = r.transcripts[&id]
                .front()
                .map(|e| e.agent.clone())
                .ok_or_else(|| PhoneError::NotFound("no such agent".into()))?;
            let (m, fx) = r.core.send_inbox(to, None, None, None, &q.text, 0, now)?;
            apply(r, fx, out);
            Ok(json!({ "id": m.id.to_hex() }))
        }
        "status" => {
            let theme = match params.get("theme").and_then(Value::as_str) {
                Some("light") => Theme::Light,
                _ => Theme::Dark,
            };
            Ok(serde_json::to_value(status(p, theme, now)).unwrap_or_default())
        }
        // Lists with nothing on a phone: empty rather than an error, so the
        // views show their honest empty state.
        "merge.queue" | "term.list" | "claims.list" | "memory.list" => Ok(json!([])),
        other => Err(needs_daemon(other)),
    }
}

/// This member's agent hosts, like the daemon's `agent.hosts`.
fn hosts(r: &Room, now: i64) -> Vec<Value> {
    let me = r.core.my_member;
    r.core
        .peers
        .iter()
        .filter(|(d, x)| x.member() == Some(me) && r.core.is_agent_host(d))
        .map(|(d, x)| {
            let agents = r
                .transcripts
                .values()
                .filter(|l| l.front().is_some_and(|e| e.agent.device == *d))
                .filter(|l| !matches!(agent_state(l), Some(AgentState::Stopped)))
                .count();
            json!({ "device": d.to_hex(), "name": x.name(), "online": x.online(now), "agents": agents })
        })
        .collect()
}

/// Appends an activity entry, like the daemon's `feed::log_locked`.
fn log(r: &mut Room, kind: &str, args: Value, now: i64, out: &mut Output) {
    let e = ActivityEntry {
        id: ObjectId::random().map(|o| o.to_hex()).unwrap_or_default(),
        at: now,
        kind: kind.to_string(),
        actor: r.core.my_member.to_hex(),
        agent: None,
        summary_key: format!("activity-{}", kind.replace('_', "-")),
        args,
        checkpoint: None,
    };
    if let Ok(fx) = r.core.change_doc(DocName::Activity, |d| {
        let _ = Activity::new(d).append(&e);
    }) {
        apply(r, fx, out);
    }
}

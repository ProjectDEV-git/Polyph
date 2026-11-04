//! Keys to actions. Actions that call the daemon run as their own tasks
//! ([`perform`]) so a long call (prompting an agent) never freezes the screen.

use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::{Value, json};

use crate::conn::Backend;
use crate::model::{App, Focus, Parts, View, s};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// A daemon call, then refresh `parts`; `done` is a message key on success.
    Call { method: String, params: Value, parts: Parts, done: Option<String> },
    /// Bytes for a shared terminal.
    TermInput { room: String, term: String, bytes: Vec<u8> },
    /// Rebuild the board at a time.
    Replay { room: String, at: i64 },
    /// Switch the room shown.
    Room(String),
    /// Fetch parts again (for example the transcript of a newly selected agent).
    Refresh(Parts),
}

/// What an action produced, applied by the loop.
#[derive(Debug, Default)]
pub struct Outcome {
    pub parts: Parts,
    pub message: Option<String>,
    pub replay: Option<(i64, Value)>,
    pub room: Option<String>,
}

fn call(method: &str, params: Value, parts: Parts, done: Option<&str>) -> Action {
    Action::Call { method: method.into(), params, parts, done: done.map(str::to_string) }
}

/// Bytes a terminal expects for a key.
pub fn key_bytes(k: &KeyEvent) -> Vec<u8> {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    match k.code {
        KeyCode::Char(c) if ctrl && c.is_ascii_alphabetic() => vec![(c.to_ascii_lowercase() as u8) - b'a' + 1],
        KeyCode::Char(c) => {
            let mut b = [0u8; 4];
            let mut v = c.encode_utf8(&mut b).as_bytes().to_vec();
            if k.modifiers.contains(KeyModifiers::ALT) {
                v.insert(0, 0x1b);
            }
            v
        }
        KeyCode::Enter => b"\r".to_vec(),
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Tab => b"\t".to_vec(),
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => b"\x1b[A".to_vec(),
        KeyCode::Down => b"\x1b[B".to_vec(),
        KeyCode::Right => b"\x1b[C".to_vec(),
        KeyCode::Left => b"\x1b[D".to_vec(),
        KeyCode::Home => b"\x1b[H".to_vec(),
        KeyCode::End => b"\x1b[F".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        _ => vec![],
    }
}

fn count(app: &App, v: View) -> usize {
    match v {
        View::Chat => {
            if app.show_threads {
                app.threads.len() + 1
            } else {
                0
            }
        }
        View::Board => app.columns.get(app.board_col).map(|c| app.column_cards(&c.0).len()).unwrap_or(0),
        View::Agents => app.agents.len(),
        View::Activity => app.activity.len(),
        View::Merges => app.merges.len(),
        View::Terminals => app.terms.len(),
    }
}

fn move_sel(app: &mut App, delta: isize) {
    let v = app.view;
    let n = count(app, v);
    if n == 0 {
        return;
    }
    let cur = app.sel(v) as isize;
    let next = (cur + delta).clamp(0, n as isize - 1) as usize;
    app.sel.insert(v, next);
}

/// Can this person type in the terminal right now?
pub fn can_type(app: &App, term: &str) -> bool {
    let me = app.me().map(|m| s(m, "member").to_string()).unwrap_or_default();
    app.terms.iter().any(|t| s(t, "id") == term && t["closed"] != true && (s(t, "owner") == me || s(t, "writer") == me))
}

pub fn on_key(app: &mut App, k: KeyEvent) -> Vec<Action> {
    let room = app.room.clone().unwrap_or_default();
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl && k.code == KeyCode::Char('c') && !matches!(app.focus, Focus::Attached(_)) {
        app.quit = true;
        return vec![];
    }
    match app.focus.clone() {
        Focus::Attached(term) => {
            // Ctrl-] is byte 0x1d, which crossterm reports as Ctrl-5.
            if ctrl && matches!(k.code, KeyCode::Char(']') | KeyCode::Char('5')) {
                app.focus = Focus::Normal;
                return vec![];
            }
            if can_type(app, &term) {
                let bytes = key_bytes(&k);
                return if bytes.is_empty() { vec![] } else { vec![Action::TermInput { room, term, bytes }] };
            }
            if k.code == KeyCode::Char('a') {
                return vec![call(
                    "term.ask",
                    json!({ "room": room, "term": term }),
                    Parts::TERMS,
                    Some("tui-term-asked"),
                )];
            }
            app.say(app.l.t("tui-term-no-keyboard"));
            vec![]
        }
        Focus::Input | Focus::Command | Focus::DenyNote(_) => text_key(app, k, &room),
        Focus::Normal => normal_key(app, k, &room),
    }
}

fn text_key(app: &mut App, k: KeyEvent, room: &str) -> Vec<Action> {
    match k.code {
        KeyCode::Esc => {
            app.focus = Focus::Normal;
            app.input.clear();
            vec![]
        }
        KeyCode::Backspace => {
            app.input.pop();
            vec![]
        }
        KeyCode::Char(c) => {
            app.input.push(c);
            vec![]
        }
        KeyCode::Enter => {
            let text = std::mem::take(&mut app.input);
            let focus = std::mem::replace(&mut app.focus, Focus::Normal);
            match focus {
                Focus::Input if text.trim().is_empty() => vec![],
                Focus::Input => {
                    if app.view == View::Agents
                        && let Some(a) = app.selected_agent()
                    {
                        if a["mine"] != true {
                            // Someone else's agent: the message goes to its inbox.
                            return vec![call(
                                "inbox.send",
                                json!({ "room": room, "agent": s(a, "id"), "text": text }),
                                Parts::default(),
                                Some("tui-sent-inbox"),
                            )];
                        }
                        return vec![call(
                            "agent.prompt",
                            json!({ "agent": s(a, "id"), "text": text }),
                            Parts::TRANSCRIPT | Parts::AGENTS,
                            None,
                        )];
                    }
                    vec![call(
                        "chat.send",
                        json!({ "room": room, "text": text, "thread": app.open_thread }),
                        Parts::CHAT,
                        None,
                    )]
                }
                Focus::DenyNote(id) => {
                    let note = (!text.trim().is_empty()).then_some(text);
                    vec![call(
                        "deny",
                        json!({ "room": room, "id": id, "note": note }),
                        Parts::APPROVALS,
                        Some("tui-denied"),
                    )]
                }
                Focus::Command => command(app, text.trim(), room),
                _ => vec![],
            }
        }
        _ => vec![],
    }
}

/// `:` commands.
fn command(app: &mut App, line: &str, room: &str) -> Vec<Action> {
    let (cmd, rest) = line.split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((line, ""));
    let cwd = std::env::current_dir().unwrap_or_default();
    match cmd {
        "q" | "quit" => {
            app.quit = true;
            vec![]
        }
        "help" => {
            app.help = true;
            vec![]
        }
        "join" if !rest.is_empty() => vec![call("join", json!({ "code": rest }), Parts::ALL, Some("tui-joined"))],
        "thread" if !rest.is_empty() => {
            vec![call("thread.create", json!({ "room": room, "title": rest }), Parts::CHAT, Some("tui-thread-created"))]
        }
        "card" if !rest.is_empty() => vec![call(
            "board.add",
            json!({ "room": room, "title": rest, "column": "backlog" }),
            Parts::BOARD,
            Some("tui-card-added"),
        )],
        "plan" if !rest.is_empty() => vec![call(
            "plan.create",
            json!({ "room": room, "goal": rest, "cwd": cwd }),
            Parts::BOARD,
            Some("tui-planned"),
        )],
        "term" => {
            let command: Vec<&str> = rest.split_whitespace().collect();
            vec![call("term.open", json!({ "room": room, "cwd": cwd, "command": command }), Parts::TERMS, None)]
        }
        "agent" if !rest.is_empty() => vec![call(
            "agent.start",
            json!({ "room": room, "kind": rest, "cwd": cwd }),
            Parts::AGENTS,
            Some("tui-agent-started"),
        )],
        "room" if !rest.is_empty() => {
            let want = rest.to_lowercase();
            match app.rooms.iter().find(|r| s(r, "name").to_lowercase() == want || s(r, "id").starts_with(&want)) {
                Some(r) => vec![Action::Room(s(r, "id").to_string())],
                None => {
                    app.say(app.l.t("tui-no-such-room"));
                    vec![]
                }
            }
        }
        "" => vec![],
        _ => {
            app.say(app.l.t("tui-unknown-command"));
            vec![]
        }
    }
}

fn normal_key(app: &mut App, k: KeyEvent, room: &str) -> Vec<Action> {
    let mine = app.my_approvals().len();
    match k.code {
        KeyCode::Char('q') => app.quit = true,
        KeyCode::Char('?') => app.help = !app.help,
        KeyCode::Esc => {
            app.help = false;
            app.replay = None;
            app.message = None;
        }
        KeyCode::Char(c @ '1'..='6') => app.view = View::ALL[(c as u8 - b'1') as usize],
        KeyCode::Tab => app.view = View::ALL[(app.view.index() + 1) % View::ALL.len()],
        KeyCode::BackTab => app.view = View::ALL[(app.view.index() + View::ALL.len() - 1) % View::ALL.len()],
        KeyCode::Char('j') | KeyCode::Down => {
            move_sel(app, 1);
            if app.view == View::Agents {
                return vec![Action::Refresh(Parts::TRANSCRIPT)];
            }
        }
        KeyCode::Char('k') | KeyCode::Up => {
            move_sel(app, -1);
            if app.view == View::Agents {
                return vec![Action::Refresh(Parts::TRANSCRIPT)];
            }
        }
        KeyCode::Char('h') | KeyCode::Left => match app.view {
            View::Board => {
                app.board_col = app.board_col.saturating_sub(1);
                app.sel.insert(View::Board, 0);
            }
            View::Activity => return replay_step(app, room, -1),
            _ => {}
        },
        KeyCode::Char('l') | KeyCode::Right => match app.view {
            View::Board => {
                app.board_col = (app.board_col + 1).min(app.columns.len().saturating_sub(1));
                app.sel.insert(View::Board, 0);
            }
            View::Activity => return replay_step(app, room, 1),
            _ => {}
        },
        KeyCode::Char(c @ ('H' | 'L')) if app.view == View::Board => {
            let Some(card) = app.selected_card().map(|c| s(c, "id").to_string()) else { return vec![] };
            let to = if c == 'H' {
                app.board_col.checked_sub(1)
            } else {
                Some(app.board_col + 1).filter(|i| *i < app.columns.len())
            };
            if let Some(to) = to {
                let col = app.columns[to].0.clone();
                app.board_col = to;
                let n = app.column_cards(&col).len();
                app.sel.insert(View::Board, n);
                return vec![call(
                    "board.move",
                    json!({ "room": room, "card": card, "column": col }),
                    Parts::BOARD | Parts::ACTIVITY,
                    None,
                )];
            }
        }
        KeyCode::Enter => match app.view {
            View::Chat if app.show_threads => {
                let i = app.sel(View::Chat);
                app.open_thread =
                    if i == 0 { None } else { app.threads.get(i - 1).map(|t| s(&t["thread"], "id").to_string()) };
            }
            View::Agents | View::Chat => app.focus = Focus::Input,
            View::Terminals => {
                if let Some(t) = app.selected_term().map(|t| s(t, "id").to_string()) {
                    app.focus = Focus::Attached(t);
                }
            }
            View::Activity => return replay_step(app, room, 0),
            _ => {}
        },
        KeyCode::Char('i') => {
            if matches!(app.view, View::Chat | View::Agents) {
                app.focus = Focus::Input;
            }
        }
        KeyCode::Char(':') => app.focus = Focus::Command,
        KeyCode::Char('T') => {
            app.view = View::Chat;
            app.show_threads = !app.show_threads;
        }
        KeyCode::Char('r') => {
            if app.width >= crate::ui::RAIL_MIN_WIDTH {
                app.rail = !app.rail;
            } else {
                app.rail_narrow = !app.rail_narrow;
            }
        }
        KeyCode::Char('[') | KeyCode::Char(']') => {
            let n = app.rooms.len();
            if n > 1 {
                let cur = app.rooms.iter().position(|r| Some(s(r, "id")) == app.room.as_deref()).unwrap_or(0);
                let next = if k.code == KeyCode::Char(']') { (cur + 1) % n } else { (cur + n - 1) % n };
                return vec![Action::Room(s(&app.rooms[next], "id").to_string())];
            }
        }
        KeyCode::Char('n') if mine > 0 => app.approval_sel = (app.approval_sel + 1) % mine,
        KeyCode::Char(c @ ('a' | 't' | 'd')) if mine > 0 => {
            let id = s(app.my_approvals()[app.approval_sel.min(mine - 1)], "id").to_string();
            return match c {
                'd' => {
                    app.focus = Focus::DenyNote(id);
                    vec![]
                }
                _ => vec![call(
                    "approve",
                    json!({ "room": room, "id": id, "task": c == 't' }),
                    Parts::APPROVALS,
                    Some("tui-approved"),
                )],
            };
        }
        KeyCode::Char('s') if app.view == View::Agents => {
            if let Some(a) = app.selected_agent().filter(|a| a["mine"] == true) {
                return vec![call(
                    "agent.stop",
                    json!({ "agent": s(a, "id") }),
                    Parts::AGENTS,
                    Some("tui-agent-stopping"),
                )];
            }
        }
        KeyCode::Char('x') if app.view == View::Terminals => {
            if let Some(t) = app.selected_term() {
                return vec![call("term.close", json!({ "room": room, "term": s(t, "id") }), Parts::TERMS, None)];
            }
        }
        _ => {}
    }
    vec![]
}

/// Moves the replay cursor through the activity feed (0 starts it).
fn replay_step(app: &mut App, room: &str, delta: isize) -> Vec<Action> {
    if app.activity.is_empty() {
        return vec![];
    }
    let n = app.activity.len() as isize;
    let cur = if app.replay.is_none() { n - 1 } else { app.sel(View::Activity) as isize };
    let i = (cur + delta).clamp(0, n - 1) as usize;
    app.sel.insert(View::Activity, i);
    let at = app.activity[i]["at"].as_i64().unwrap_or(0);
    vec![Action::Replay { room: room.to_string(), at }]
}

/// Runs an action against the daemon.
pub async fn perform(b: Arc<dyn Backend>, a: Action) -> Outcome {
    match a {
        Action::Refresh(parts) => Outcome { parts, ..Default::default() },
        Action::Call { method, params, parts, done } => match b.call(&method, params).await {
            Ok(_) => Outcome { parts, message: done, ..Default::default() },
            Err(e) => Outcome { parts, message: Some(format!("!{e}")), ..Default::default() },
        },
        Action::TermInput { room, term, bytes } => {
            use base64::Engine as _;
            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            match b.call("term.input", json!({ "room": room, "term": term, "data": data })).await {
                Ok(_) => Outcome::default(),
                Err(e) => Outcome { message: Some(format!("!{e}")), ..Default::default() },
            }
        }
        Action::Replay { room, at } => match b.call("activity.replay", json!({ "room": room, "at": at })).await {
            Ok(v) => Outcome { replay: Some((at, v)), ..Default::default() },
            Err(e) => Outcome { message: Some(format!("!{e}")), ..Default::default() },
        },
        Action::Room(id) => Outcome { room: Some(id), parts: Parts::ALL, ..Default::default() },
    }
}

impl App {
    /// Applies an outcome: messages (keys, or `!` + raw daemon text), replay,
    /// room switch. The caller refreshes `parts`.
    pub fn apply_outcome(&mut self, o: &Outcome) {
        if let Some(m) = &o.message {
            let text = match m.strip_prefix('!') {
                Some(raw) => raw.to_string(),
                None => self.l.t(m),
            };
            self.say(text);
        }
        if let Some(r) = &o.replay {
            self.replay = Some(r.clone());
        }
        if let Some(room) = &o.room {
            self.room = Some(room.clone());
            self.open_thread = None;
            self.replay = None;
            self.sel.clear();
            self.board_col = 0;
            self.screens.clear();
        }
    }
}

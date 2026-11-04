//! The app state: what the screen shows, refreshed from the daemon in parts
//! when events say something changed.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use polyph_i18n::Localizer;
use serde_json::{Value, json};

use crate::conn::Backend;
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Chat,
    Board,
    Agents,
    Activity,
    Merges,
    Terminals,
}

impl View {
    pub const ALL: [View; 6] = [View::Chat, View::Board, View::Agents, View::Activity, View::Merges, View::Terminals];

    pub fn key(self) -> &'static str {
        match self {
            View::Chat => "tui-view-chat",
            View::Board => "tui-view-board",
            View::Agents => "tui-view-agents",
            View::Activity => "tui-view-activity",
            View::Merges => "tui-view-merges",
            View::Terminals => "tui-view-terminals",
        }
    }

    pub fn index(self) -> usize {
        View::ALL.iter().position(|v| *v == self).unwrap_or(0)
    }
}

/// What the keyboard is talking to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Focus {
    Normal,
    /// Typing a chat message or an agent prompt.
    Input,
    /// After `:`.
    Command,
    /// Typing the note for a denial.
    DenyNote(String),
    /// Keys go to a shared terminal.
    Attached(String),
}

/// Parts of the state to fetch again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Parts(pub u32);

impl Parts {
    pub const ROOMS: Parts = Parts(1);
    pub const MEMBERS: Parts = Parts(1 << 1);
    pub const CHAT: Parts = Parts(1 << 2);
    pub const BOARD: Parts = Parts(1 << 3);
    pub const AGENTS: Parts = Parts(1 << 4);
    pub const APPROVALS: Parts = Parts(1 << 5);
    pub const ACTIVITY: Parts = Parts(1 << 6);
    pub const MERGES: Parts = Parts(1 << 7);
    pub const TERMS: Parts = Parts(1 << 8);
    pub const TRANSCRIPT: Parts = Parts(1 << 9);
    pub const ALL: Parts = Parts(u32::MAX);

    pub fn has(self, p: Parts) -> bool {
        self.0 & p.0 != 0
    }
}

impl std::ops::BitOr for Parts {
    type Output = Parts;
    fn bitor(self, o: Parts) -> Parts {
        Parts(self.0 | o.0)
    }
}

impl std::ops::BitOrAssign for Parts {
    fn bitor_assign(&mut self, o: Parts) {
        self.0 |= o.0;
    }
}

/// How long a moved card stays highlighted (DESIGN.md).
pub const FLASH: Duration = Duration::from_millis(600);

pub struct App {
    pub l: Localizer,
    pub theme: Theme,
    pub rooms: Vec<Value>,
    pub room: Option<String>,
    pub members: Vec<Value>,
    pub agents: Vec<Value>,
    pub chat: Vec<Value>,
    pub threads: Vec<Value>,
    pub columns: Vec<(String, String)>,
    pub cards: Vec<Value>,
    pub activity: Vec<Value>,
    pub merges: Vec<Value>,
    pub approvals: Vec<Value>,
    pub terms: Vec<Value>,
    pub screens: HashMap<String, vt100::Parser>,
    pub transcript: Vec<Value>,
    /// Activity replay: (time, board at that time).
    pub replay: Option<(i64, Value)>,
    pub view: View,
    pub sel: HashMap<View, usize>,
    pub board_col: usize,
    pub approval_sel: usize,
    pub focus: Focus,
    pub input: String,
    pub rail: bool,
    /// The rail on terminals narrower than 80 columns (off unless asked for).
    pub rail_narrow: bool,
    /// Last known screen width, for the rail toggle.
    pub width: u16,
    pub help: bool,
    pub tick: usize,
    pub flash: HashMap<String, Instant>,
    /// A one-line message (an error, a confirmation) and when it was set.
    pub message: Option<(String, Instant)>,
    pub quit: bool,
    /// The thread open in the Chat view (None: the room's main chat).
    pub open_thread: Option<String>,
    /// Shows the thread list in the Chat view.
    pub show_threads: bool,
    /// Local time offset from UTC in seconds, for clock times.
    pub utc_offset: i64,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "App({:?}, {:?})", self.room, self.view)
    }
}

pub fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or_default()
}

impl App {
    pub fn new(l: Localizer, theme: Theme) -> Self {
        Self {
            l,
            theme,
            rooms: vec![],
            room: None,
            members: vec![],
            agents: vec![],
            chat: vec![],
            threads: vec![],
            columns: vec![],
            cards: vec![],
            activity: vec![],
            merges: vec![],
            approvals: vec![],
            terms: vec![],
            screens: HashMap::new(),
            transcript: vec![],
            replay: None,
            view: View::Chat,
            sel: HashMap::new(),
            board_col: 0,
            approval_sel: 0,
            focus: Focus::Normal,
            input: String::new(),
            rail: true,
            rail_narrow: false,
            width: 120,
            help: false,
            tick: 0,
            flash: HashMap::new(),
            message: None,
            quit: false,
            open_thread: None,
            show_threads: false,
            utc_offset: 0,
        }
    }

    pub fn sel(&self, v: View) -> usize {
        self.sel.get(&v).copied().unwrap_or(0)
    }

    pub fn say(&mut self, text: impl Into<String>) {
        self.message = Some((text.into(), Instant::now()));
    }

    pub fn me(&self) -> Option<&Value> {
        self.members.iter().find(|m| m["me"] == true)
    }

    pub fn room_info(&self) -> Option<&Value> {
        let id = self.room.as_deref()?;
        self.rooms.iter().find(|r| s(r, "id") == id)
    }

    /// Hue of a member (u16::MAX when unknown).
    pub fn hue_of(&self, member: &str) -> u16 {
        self.members
            .iter()
            .find(|m| s(m, "member") == member)
            .and_then(|m| m["hue"].as_u64())
            .map(|h| h as u16)
            .unwrap_or(u16::MAX)
    }

    /// An agent's (hue, step): its owner's hue and its place among their agents.
    pub fn agent_voice(&self, label: &str) -> (u16, u8) {
        let Some(a) = self.agents.iter().find(|a| s(a, "label") == label) else { return (u16::MAX, 0) };
        let member = s(a, "member");
        let n =
            self.agents.iter().filter(|x| s(x, "member") == member).position(|x| s(x, "label") == label).unwrap_or(0);
        (self.hue_of(member), n as u8)
    }

    /// Approvals this person is the one to answer now.
    pub fn my_approvals(&self) -> Vec<&Value> {
        let me = self.me().map(|m| s(m, "member").to_string()).unwrap_or_default();
        self.approvals.iter().filter(|a| a["agent"]["member"].as_str() == Some(me.as_str())).collect()
    }

    pub fn selected_agent(&self) -> Option<&Value> {
        self.agents.get(self.sel(View::Agents))
    }

    pub fn selected_term(&self) -> Option<&Value> {
        self.terms.get(self.sel(View::Terminals))
    }

    pub fn column_cards(&self, col: &str) -> Vec<&Value> {
        let mut v: Vec<&Value> = self.cards.iter().filter(|c| s(c, "column") == col && c["draft"] != true).collect();
        v.sort_by(|a, b| a["order"].as_f64().unwrap_or(0.0).total_cmp(&b["order"].as_f64().unwrap_or(0.0)));
        v
    }

    pub fn selected_card(&self) -> Option<&Value> {
        let col = self.columns.get(self.board_col)?;
        self.column_cards(&col.0).get(self.sel(View::Board)).copied()
    }

    /// Which parts an event touches.
    pub fn parts_for(&self, ev: &Value) -> Parts {
        if let (Some(room), Some(mine)) = (ev["room"].as_str(), self.room.as_deref())
            && room != mine
        {
            return if ev["event"]["type"] == "connection" { Parts::ROOMS } else { Parts::default() };
        }
        let e = &ev["event"];
        match e["type"].as_str().unwrap_or_default() {
            "chat" => Parts::CHAT,
            "doc_changed" => match e["doc"].as_str() {
                Some("board") => Parts::BOARD,
                Some("activity") => Parts::ACTIVITY,
                Some("threads") => Parts::CHAT,
                _ => Parts::default(),
            },
            "agent" => {
                let mut p = Parts::default();
                if matches!(e["event"]["kind"]["t"].as_str(), Some("started" | "state" | "stopped")) {
                    p |= Parts::AGENTS;
                }
                if self.selected_agent().is_some_and(|a| a["id"] == e["event"]["agent"]["id"]) {
                    p |= Parts::TRANSCRIPT;
                }
                p
            }
            "approval_pending" | "approval_resolved" => Parts::APPROVALS,
            "member_online" | "member_offline" | "member_joined" | "member_removed" => Parts::MEMBERS | Parts::ROOMS,
            "connection" | "keys_rotated" => Parts::ROOMS,
            "merge_queue" => Parts::MERGES,
            "terminal_opened" | "terminal_closed" | "terminal_control" | "terminal_resized" => Parts::TERMS,
            _ => Parts::default(),
        }
    }

    /// Applies what an event carries directly (terminal bytes, notices).
    pub fn apply_event(&mut self, ev: &Value) {
        let e = &ev["event"];
        match e["type"].as_str().unwrap_or_default() {
            "terminal_output" => {
                use base64::Engine as _;
                let id = s(e, "id").to_string();
                if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(s(e, "data")) {
                    let (rows, cols) = self.term_size(&id);
                    self.screens.entry(id).or_insert_with(|| vt100::Parser::new(rows, cols, 1000)).process(&bytes);
                }
            }
            "notification" => {
                let t = e["notification"]["text"].as_str().unwrap_or_default().to_string();
                self.say(t);
            }
            _ => {}
        }
    }

    fn term_size(&self, id: &str) -> (u16, u16) {
        self.terms
            .iter()
            .find(|t| s(t, "id") == id)
            .map(|t| (t["rows"].as_u64().unwrap_or(30) as u16, t["cols"].as_u64().unwrap_or(100) as u16))
            .unwrap_or((30, 100))
    }

    /// Fetches the given parts again.
    pub async fn refresh(&mut self, b: &dyn Backend, parts: Parts) {
        if parts.has(Parts::ROOMS) {
            if let Ok(v) = b.call("room.list", json!({})).await {
                self.rooms = v.as_array().cloned().unwrap_or_default();
            }
            if self.room.is_none() || !self.rooms.iter().any(|r| Some(s(r, "id")) == self.room.as_deref()) {
                // The daemon's current room, else the first open one.
                let current =
                    b.call("status", json!({})).await.ok().and_then(|v| v["room"].as_str().map(str::to_string));
                self.room =
                    current.or_else(|| self.rooms.iter().find(|r| r["open"] == true).map(|r| s(r, "id").to_string()));
                if self.room.is_some() {
                    return Box::pin(self.refresh(b, Parts(Parts::ALL.0 & !Parts::ROOMS.0))).await;
                }
            }
        }
        let Some(room) = self.room.clone() else { return };
        let r = json!({ "room": room });
        if parts.has(Parts::MEMBERS)
            && let Ok(v) = b.call("room.members", r.clone()).await
        {
            self.members = v.as_array().cloned().unwrap_or_default();
        }
        if parts.has(Parts::CHAT) {
            if let Ok(v) = b.call("chat.list", json!({ "room": room, "limit": 500 })).await {
                self.chat = v.as_array().cloned().unwrap_or_default();
            }
            if let Ok(v) = b.call("thread.list", r.clone()).await {
                self.threads = v.as_array().cloned().unwrap_or_default();
            }
        }
        if parts.has(Parts::BOARD)
            && let Ok(v) = b.call("board.get", r.clone()).await
        {
            let before: HashMap<String, String> =
                self.cards.iter().map(|c| (s(c, "id").to_string(), s(c, "column").to_string())).collect();
            self.columns = v["columns"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| (s(c, "id").to_string(), s(c, "title").to_string()))
                .collect();
            self.cards = v["cards"].as_array().cloned().unwrap_or_default();
            let now = Instant::now();
            for c in &self.cards {
                if before.get(s(c, "id")).is_some_and(|col| col != s(c, "column")) {
                    self.flash.insert(s(c, "id").to_string(), now);
                }
            }
        }
        if parts.has(Parts::AGENTS)
            && let Ok(v) = b.call("agent.list", r.clone()).await
        {
            let mut list = v.as_array().cloned().unwrap_or_default();
            list.retain(|a| a["state"] != "stopped");
            list.sort_by(|a, b| s(a, "label").cmp(s(b, "label")));
            self.agents = list;
        }
        if parts.has(Parts::APPROVALS)
            && let Ok(v) = b.call("approvals.list", r.clone()).await
        {
            self.approvals = v.as_array().cloned().unwrap_or_default();
            self.approval_sel = self.approval_sel.min(self.my_approvals().len().saturating_sub(1));
        }
        if parts.has(Parts::ACTIVITY)
            && let Ok(v) = b.call("activity.list", json!({ "room": room, "limit": 300 })).await
        {
            self.activity = v.as_array().cloned().unwrap_or_default();
        }
        if parts.has(Parts::MERGES)
            && let Ok(v) = b.call("merge.queue", r.clone()).await
        {
            self.merges = v.as_array().cloned().unwrap_or_default();
        }
        if parts.has(Parts::TERMS)
            && let Ok(v) = b.call("term.list", r.clone()).await
        {
            self.terms = v.as_array().cloned().unwrap_or_default();
            // A terminal seen for the first time starts from its scrollback.
            for t in self.terms.clone() {
                let id = s(&t, "id").to_string();
                if !self.screens.contains_key(&id)
                    && let Ok(rd) = b.call("term.read", json!({ "room": room, "term": id })).await
                {
                    use base64::Engine as _;
                    let mut p = vt100::Parser::new(
                        t["rows"].as_u64().unwrap_or(30) as u16,
                        t["cols"].as_u64().unwrap_or(100) as u16,
                        1000,
                    );
                    if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(s(&rd, "data")) {
                        p.process(&bytes);
                    }
                    self.screens.insert(id, p);
                }
            }
        }
        if parts.has(Parts::TRANSCRIPT) || parts.has(Parts::AGENTS) {
            let id = self.selected_agent().map(|a| s(a, "id").to_string());
            self.transcript = match id {
                Some(id) => b
                    .call("agent.transcript", json!({ "room": room, "agent": id }))
                    .await
                    .ok()
                    .and_then(|v| v["events"].as_array().cloned())
                    .unwrap_or_default(),
                None => vec![],
            };
        }
    }

    /// Advances the spinner; true when something on screen animates.
    pub fn animate(&mut self) -> bool {
        self.tick = self.tick.wrapping_add(1);
        let now = Instant::now();
        let flashing = !self.flash.is_empty();
        self.flash.retain(|_, t| now.duration_since(*t) < FLASH);
        if self.message.as_ref().is_some_and(|(_, t)| now.duration_since(*t) > Duration::from_secs(6)) {
            self.message = None;
            return true;
        }
        flashing || self.agents.iter().any(|a| matches!(s(a, "state"), "running" | "starting"))
    }
}

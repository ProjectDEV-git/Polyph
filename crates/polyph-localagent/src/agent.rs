//! One session's conversation and the tool loop.

use std::path::PathBuf;

use polyph_agents::rpc::Peer;
use serde_json::{Value, json};

use crate::llm::{Client, Message, ToolCallOut, Usage};
use crate::{textproto, tools};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolMode {
    Native,
    Text,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
    /// The model's context window in tokens.
    pub context: usize,
    pub tools: ToolMode,
    pub max_steps: usize,
}

impl Config {
    pub fn from_env() -> Self {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Self {
            endpoint: get("POLYPH_LOCAL_ENDPOINT").unwrap_or_else(|| "http://127.0.0.1:11434/v1".into()),
            model: get("POLYPH_LOCAL_MODEL").unwrap_or_else(|| "qwen2.5-coder:7b".into()),
            api_key: get("POLYPH_LOCAL_API_KEY"),
            context: get("POLYPH_LOCAL_CONTEXT").and_then(|v| v.parse().ok()).unwrap_or(32_768),
            tools: if get("POLYPH_LOCAL_TOOLS").as_deref() == Some("text") { ToolMode::Text } else { ToolMode::Native },
            max_steps: get("POLYPH_LOCAL_MAX_STEPS").and_then(|v| v.parse().ok()).unwrap_or(30),
        }
    }
}

const SYSTEM: &str = "You are a careful coding agent working in a git repository shared with teammates and their agents. \
Read before you edit, keep changes small, and run the project's checks when you change code. \
Some files may be claimed by other agents; if an edit is refused because of a claim, pick other work or ask.";

/// Marker placed at the start of a compaction summary.
pub const SUMMARY_MARKER: &str = "[Summary of earlier conversation]";

pub struct Session {
    pub id: String,
    pub cwd: PathBuf,
    pub cfg: Config,
    pub messages: Vec<Message>,
    client: Client,
    /// Switched to the text protocol after the endpoint rejected native tools.
    text_fallback: bool,
    /// Counter for unique tool call ids (models reuse "call_0" every reply).
    calls_made: u64,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Session({})", self.id)
    }
}

async fn update(peer: &Peer, session: &str, u: Value) {
    let _ = peer.notify("session/update", &json!({ "sessionId": session, "update": u })).await;
}

fn tokens(msgs: &[Message]) -> usize {
    msgs.iter().map(|m| m.tokens()).sum()
}

impl Session {
    pub fn new(id: String, cwd: PathBuf, cfg: Config) -> anyhow::Result<Self> {
        let client = Client::new(&cfg.endpoint, &cfg.model, cfg.api_key.clone())?;
        let mut system = SYSTEM.to_string();
        if cfg.tools == ToolMode::Text {
            system = format!("{system}\n\n{}", textproto::INSTRUCTIONS);
        }
        Ok(Self {
            id,
            cwd,
            messages: vec![Message::system(&system)],
            client,
            text_fallback: cfg.tools == ToolMode::Text,
            calls_made: 0,
            cfg,
        })
    }

    fn mode(&self) -> ToolMode {
        if self.text_fallback { ToolMode::Text } else { self.cfg.tools }
    }

    /// Keeps the conversation under 75% of the context window by folding the
    /// oldest turns into a summary. The model writes the summary; if that
    /// fails, the oldest turns are dropped with a note.
    async fn compact(&mut self, usage: &mut Usage) {
        let budget = self.cfg.context * 3 / 4;
        if tokens(&self.messages) <= budget || self.messages.len() < 4 {
            return;
        }
        // Keep the system prompt and the most recent quarter of the budget.
        let keep_tail = budget / 4;
        let mut split = self.messages.len();
        let mut tail = 0;
        while split > 2 && tail + self.messages[split - 1].tokens() <= keep_tail {
            split -= 1;
            tail += self.messages[split].tokens();
        }
        // Never start the kept tail with a tool result whose call was summarized.
        while split < self.messages.len() && self.messages[split].role == "tool" {
            split += 1;
        }
        let old: Vec<Message> = self.messages[1..split].to_vec();
        if old.is_empty() {
            return;
        }
        let transcript: String = old
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content.clone().unwrap_or_default()))
            .collect::<Vec<_>>()
            .join("\n")
            .chars()
            .take(self.cfg.context * 2)
            .collect();
        let ask = vec![
            Message::system(
                "Summarize this coding session so far in under 200 words: goals, decisions, files changed, open problems.",
            ),
            Message::user(&transcript),
        ];
        let summary = match self.client.chat(&ask, None, &mut |_| {}).await {
            Ok(r) => {
                if let Some(u) = r.usage {
                    usage.input += u.input;
                    usage.output += u.output;
                }
                r.text
            }
            Err(_) => format!("{} earlier messages were dropped to fit the context window.", old.len()),
        };
        let mut next = vec![self.messages[0].clone(), Message::user(&format!("{SUMMARY_MARKER}\n{summary}"))];
        next.extend_from_slice(&self.messages[split..]);
        self.messages = next;
    }

    /// Runs one prompt to completion. Returns the stop reason and usage.
    pub async fn prompt(
        &mut self,
        peer: &Peer,
        text: &str,
        cancel: &tokio::sync::watch::Receiver<bool>,
    ) -> (String, Option<Usage>) {
        self.messages.push(Message::user(text));
        let mut usage = Usage::default();
        let mut reported = false;
        let mut retried = false;
        for _ in 0..self.cfg.max_steps {
            if *cancel.borrow() {
                return ("cancelled".into(), Some(usage));
            }
            self.compact(&mut usage).await;
            let schema = tools::schema();
            let native = self.mode() == ToolMode::Native;
            let (p, sid) = (peer.clone(), self.id.clone());
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
            let fwd = tokio::spawn(async move {
                while let Some(t) = rx.recv().await {
                    update(
                        &p,
                        &sid,
                        json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": t } }),
                    )
                    .await;
                }
            });
            let mut on_text = move |t: &str| {
                if !t.is_empty() {
                    let _ = tx.send(t.to_string());
                }
            };
            let reply = self.client.chat(&self.messages, if native { Some(&schema) } else { None }, &mut on_text).await;
            drop(on_text);
            let _ = fwd.await;
            let reply = match reply {
                Ok(r) => r,
                Err(e) if native && e.to_string().contains("tool") => {
                    // The endpoint does not do native tools; switch to the text protocol.
                    self.text_fallback = true;
                    if let Some(m) = self.messages.first_mut() {
                        m.content = Some(format!("{SYSTEM}\n\n{}", textproto::INSTRUCTIONS));
                    }
                    continue;
                }
                Err(e) => {
                    update(peer, &self.id, json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": format!("\nThe model endpoint failed: {e}\n") } })).await;
                    return ("end_turn".into(), reported.then_some(usage));
                }
            };
            if let Some(u) = &reply.usage {
                usage.input += u.input;
                usage.output += u.output;
                reported = true;
            }
            let calls: Vec<ToolCallOut> = if native { reply.calls.clone() } else { textproto::parse(&reply.text) };
            self.messages.push(Message::assistant(&reply.text, if native { &calls } else { &[] }));
            if calls.is_empty() {
                return ("end_turn".into(), reported.then_some(usage));
            }
            // Validate every call first; ask once for a corrected call.
            let mut errors = vec![];
            let mut valid = vec![];
            for c in &calls {
                match tools::validate(&c.name, &c.arguments) {
                    Ok(v) => valid.push((c.clone(), v)),
                    Err(e) => errors.push((c.clone(), e)),
                }
            }
            if !errors.is_empty() {
                if retried {
                    update(peer, &self.id, json!({ "sessionUpdate": "agent_message_chunk", "content": { "type": "text", "text": "\nThe model kept producing invalid tool calls; stopping.\n" } })).await;
                    return ("end_turn".into(), reported.then_some(usage));
                }
                retried = true;
                for (c, e) in &errors {
                    let msg = format!("Error: {e}");
                    if native {
                        self.messages.push(Message::tool(&c.id, &msg))
                    } else {
                        self.messages.push(Message::user(&msg))
                    }
                }
                for (c, _) in &valid {
                    let msg = "Not run because another call in the same reply was invalid. Send it again.";
                    if native {
                        self.messages.push(Message::tool(&c.id, msg))
                    } else {
                        self.messages.push(Message::user(msg))
                    }
                }
                continue;
            }
            retried = false;
            for (c, args) in valid {
                self.calls_made += 1;
                let tool_id = format!("{}-{}", self.id, self.calls_made);
                update(peer, &self.id, json!({ "sessionUpdate": "tool_call", "toolCallId": tool_id, "title": c.name, "kind": kind_of(&c.name), "status": "in_progress", "rawInput": args })).await;
                let out = tools::run(peer, &self.id, &self.cwd, &c.name, &args).await;
                let mut content = vec![];
                if let Some((old, new)) = &out.diff {
                    content.push(json!({ "type": "diff", "path": out.path.clone().unwrap_or_default(), "oldText": old, "newText": new }));
                }
                if let Some(t) = &out.terminal {
                    content.push(json!({ "type": "terminal", "terminalId": t }));
                }
                let mut upd = json!({ "sessionUpdate": "tool_call_update", "toolCallId": tool_id, "title": out.title, "status": if out.ok { "completed" } else { "failed" }, "content": content });
                if let Some(p) = &out.path {
                    upd["locations"] = json!([{ "path": p }]);
                }
                if out.kind == "execute" {
                    upd["rawOutput"] = json!({ "stdout": out.text });
                }
                update(peer, &self.id, upd).await;
                if native {
                    self.messages.push(Message::tool(&c.id, &out.text));
                } else {
                    self.messages.push(Message::user(&format!("Result of {}:\n{}", c.name, out.text)));
                }
            }
        }
        ("max_turn_requests".into(), reported.then_some(usage))
    }
}

fn kind_of(name: &str) -> &'static str {
    match name {
        "read" | "list" => "read",
        "write" | "edit" => "edit",
        "glob" | "grep" => "search",
        "shell" => "execute",
        _ => "other",
    }
}

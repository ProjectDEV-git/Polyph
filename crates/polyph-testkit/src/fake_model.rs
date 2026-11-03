//! A scripted OpenAI-compatible endpoint for testing `polyph-localagent`
//! without a real model. Each request gets the next scripted reply, streamed
//! as server-sent events; every request body is recorded.

use std::collections::VecDeque;
use std::sync::Arc;

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use parking_lot::Mutex;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub enum Reply {
    Text(String),
    /// (tool name, raw arguments text)
    Tools(Vec<(String, String)>),
}

#[derive(Default)]
struct Inner {
    replies: VecDeque<Reply>,
    requests: Vec<Value>,
}

#[derive(Clone)]
pub struct FakeModel {
    inner: Arc<Mutex<Inner>>,
    pub url: String,
    _task: Arc<tokio::task::JoinHandle<()>>,
}

impl std::fmt::Debug for FakeModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FakeModel({})", self.url)
    }
}

impl FakeModel {
    pub async fn start(replies: Vec<Reply>) -> anyhow::Result<Self> {
        let inner = Arc::new(Mutex::new(Inner { replies: replies.into(), requests: vec![] }));
        let app = Router::new().route("/v1/chat/completions", post(chat)).with_state(inner.clone());
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let url = format!("http://{}/v1", l.local_addr()?);
        let task = tokio::spawn(async move {
            let _ = axum::serve(l, app).await;
        });
        Ok(Self { inner, url, _task: Arc::new(task) })
    }

    pub fn requests(&self) -> Vec<Value> {
        self.inner.lock().requests.clone()
    }
}

fn sse(chunks: Vec<Value>) -> Response {
    let mut body = String::new();
    for c in chunks {
        body.push_str(&format!("data: {c}\n\n"));
    }
    body.push_str("data: [DONE]\n\n");
    ([("content-type", "text/event-stream")], body).into_response()
}

async fn chat(State(inner): State<Arc<Mutex<Inner>>>, body: axum::Json<Value>) -> Response {
    let is_summary = body.0.get("tools").is_none()
        && body.0.pointer("/messages/0/content").and_then(|c| c.as_str()).is_some_and(|c| c.starts_with("Summarize"));
    let reply = {
        let mut g = inner.lock();
        g.requests.push(body.0.clone());
        if is_summary {
            Reply::Text("SUMMARY-OF-EARLIER-WORK".into())
        } else {
            g.replies.pop_front().unwrap_or(Reply::Text("Done.".into()))
        }
    };
    let usage = json!({ "choices": [], "usage": { "prompt_tokens": 50, "completion_tokens": 10 } });
    match reply {
        Reply::Text(t) => {
            // Split text across chunks like a real stream.
            let mid = t.char_indices().nth(t.chars().count() / 2).map(|(i, _)| i).unwrap_or(0);
            sse(vec![
                json!({ "choices": [{ "index": 0, "delta": { "content": &t[..mid] } }] }),
                json!({ "choices": [{ "index": 0, "delta": { "content": &t[mid..] } }] }),
                usage,
            ])
        }
        Reply::Tools(calls) => {
            let mut chunks = vec![];
            for (i, (name, args)) in calls.iter().enumerate() {
                let mid = args.len() / 2;
                chunks.push(json!({ "choices": [{ "index": 0, "delta": { "tool_calls": [{ "index": i, "id": format!("call_{i}"), "type": "function", "function": { "name": name, "arguments": &args[..mid] } }] } }] }));
                chunks.push(json!({ "choices": [{ "index": 0, "delta": { "tool_calls": [{ "index": i, "function": { "arguments": &args[mid..] } }] } }] }));
            }
            chunks.push(usage);
            sse(chunks)
        }
    }
}

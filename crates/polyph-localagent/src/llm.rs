//! OpenAI-compatible chat completions with streaming, native tool calls and
//! token usage.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ToolCallOut {
    pub id: String,
    pub name: String,
    /// Raw JSON text of the arguments, as the model produced it.
    pub arguments: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    pub fn system(t: &str) -> Self {
        Self { role: "system".into(), content: Some(t.into()), ..Default::default() }
    }
    pub fn user(t: &str) -> Self {
        Self { role: "user".into(), content: Some(t.into()), ..Default::default() }
    }
    pub fn assistant(t: &str, calls: &[ToolCallOut]) -> Self {
        Self {
            role: "assistant".into(),
            content: if t.is_empty() { None } else { Some(t.into()) },
            tool_calls: calls.iter().map(|c| json!({ "id": c.id, "type": "function", "function": { "name": c.name, "arguments": c.arguments } })).collect(),
            tool_call_id: None,
        }
    }
    pub fn tool(id: &str, t: &str) -> Self {
        Self { role: "tool".into(), content: Some(t.into()), tool_call_id: Some(id.into()), ..Default::default() }
    }
    /// Rough size in tokens (four characters per token).
    pub fn tokens(&self) -> usize {
        let c = self.content.as_ref().map(|c| c.len()).unwrap_or(0)
            + self.tool_calls.iter().map(|t| t.to_string().len()).sum::<usize>();
        c / 4 + 4
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Reply {
    pub text: String,
    pub calls: Vec<ToolCallOut>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone)]
pub struct Client {
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
    http: reqwest::Client,
}

impl Client {
    pub fn new(endpoint: &str, model: &str, api_key: Option<String>) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(600)).build()?;
        Ok(Self { endpoint: endpoint.trim_end_matches('/').to_string(), model: model.to_string(), api_key, http })
    }

    /// Streams a completion, calling `on_text` for each piece of text.
    pub async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&Value>,
        on_text: &mut (dyn FnMut(&str) + Send),
    ) -> anyhow::Result<Reply> {
        let mut body = json!({ "model": self.model, "messages": messages, "stream": true, "stream_options": { "include_usage": true } });
        if let Some(t) = tools {
            body["tools"] = t.clone();
        }
        let mut req = self.http.post(format!("{}/chat/completions", self.endpoint)).json(&body);
        if let Some(k) = &self.api_key {
            req = req.bearer_auth(k);
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("model endpoint returned {status}: {}", text.chars().take(300).collect::<String>());
        }
        let is_sse = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.contains("event-stream"));
        if !is_sse {
            let v: Value = resp.json().await?;
            let mut r = parse_full(&v);
            on_text(&r.text);
            r.usage = r.usage.or_else(|| usage_of(&v));
            return Ok(r);
        }
        let mut reply = Reply::default();
        let mut partial: Vec<(String, String, String)> = vec![];
        let mut buf = String::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            buf.push_str(&String::from_utf8_lossy(&chunk?));
            while let Some(pos) = buf.find('\n') {
                let line = buf[..pos].trim().to_string();
                buf.drain(..=pos);
                let Some(data) = line.strip_prefix("data:") else { continue };
                let data = data.trim();
                if data == "[DONE]" {
                    continue;
                }
                let Ok(v) = serde_json::from_str::<Value>(data) else { continue };
                if let Some(u) = usage_of(&v) {
                    reply.usage = Some(u);
                }
                let Some(delta) = v.pointer("/choices/0/delta") else { continue };
                if let Some(t) = delta.get("content").and_then(|c| c.as_str()) {
                    reply.text.push_str(t);
                    on_text(t);
                }
                for tc in delta.get("tool_calls").and_then(|t| t.as_array()).into_iter().flatten() {
                    let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                    while partial.len() <= idx {
                        partial.push((String::new(), String::new(), String::new()));
                    }
                    if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                        partial[idx].0 = id.to_string();
                    }
                    if let Some(n) = tc.pointer("/function/name").and_then(|n| n.as_str()) {
                        partial[idx].1.push_str(n);
                    }
                    if let Some(a) = tc.pointer("/function/arguments").and_then(|a| a.as_str()) {
                        partial[idx].2.push_str(a);
                    }
                }
            }
        }
        reply.calls = partial
            .into_iter()
            .enumerate()
            .filter(|(_, (_, n, _))| !n.is_empty())
            .map(|(i, (id, name, arguments))| ToolCallOut {
                id: if id.is_empty() { format!("call_{i}") } else { id },
                name,
                arguments,
            })
            .collect();
        Ok(reply)
    }
}

fn usage_of(v: &Value) -> Option<Usage> {
    let u = v.get("usage")?;
    if u.is_null() {
        return None;
    }
    Some(Usage {
        input: u.get("prompt_tokens").and_then(|x| x.as_u64()).unwrap_or(0),
        output: u.get("completion_tokens").and_then(|x| x.as_u64()).unwrap_or(0),
    })
}

fn parse_full(v: &Value) -> Reply {
    let msg = v.pointer("/choices/0/message").cloned().unwrap_or(Value::Null);
    let text = msg.get("content").and_then(|c| c.as_str()).unwrap_or_default().to_string();
    let calls = msg
        .get("tool_calls")
        .and_then(|t| t.as_array())
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, tc)| ToolCallOut {
            id: tc.get("id").and_then(|x| x.as_str()).map(str::to_string).unwrap_or_else(|| format!("call_{i}")),
            name: tc.pointer("/function/name").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
            arguments: match tc.pointer("/function/arguments") {
                Some(Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => "{}".into(),
            },
        })
        .collect();
    Reply { text, calls, usage: None }
}

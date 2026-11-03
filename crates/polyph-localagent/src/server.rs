//! ACP server for the local agent: an ACP agent for OpenAI-compatible endpoints
//! (Ollama, llama.cpp server, LM Studio, vLLM). Configured with
//! `POLYPH_LOCAL_ENDPOINT`, `POLYPH_LOCAL_MODEL`, `POLYPH_LOCAL_API_KEY`,
//! `POLYPH_LOCAL_CONTEXT` (tokens) and `POLYPH_LOCAL_TOOLS` (`native` or `text`).

use std::collections::HashMap;
use std::sync::Arc;

use polyph_agents::rpc;
use serde_json::{Value, json};
use tokio::sync::{Mutex, watch};

/// Serves ACP on stdin/stdout until the client disconnects.
pub async fn run() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cfg = crate::agent::Config::from_env();
    let (peer, mut incoming, _tasks) = rpc::connect(tokio::io::stdin(), tokio::io::stdout());
    let sessions: Arc<Mutex<HashMap<String, Arc<Mutex<crate::agent::Session>>>>> = Arc::new(Mutex::new(HashMap::new()));
    let cancels: Arc<Mutex<HashMap<String, watch::Sender<bool>>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut next = 0u64;
    while let Some(req) = incoming.recv().await {
        let method = req.method.clone();
        match method.as_str() {
            "initialize" => req.respond(Ok(json!({
                "protocolVersion": 1,
                "agentCapabilities": { "loadSession": false, "promptCapabilities": { "image": false, "audio": false, "embeddedContext": false } },
                "authMethods": [],
                "agentInfo": { "name": "polyph-localagent", "version": env!("CARGO_PKG_VERSION") }
            }))),
            "session/new" => {
                next += 1;
                let id = format!("local-{next}");
                let cwd = req.params.get("cwd").and_then(|c| c.as_str()).unwrap_or(".").into();
                match crate::agent::Session::new(id.clone(), cwd, cfg.clone()) {
                    Ok(s) => {
                        sessions.lock().await.insert(id.clone(), Arc::new(Mutex::new(s)));
                        req.respond(Ok(json!({ "sessionId": id })));
                    }
                    Err(e) => req.respond(Err((-32000, e.to_string()))),
                }
            }
            "session/set_model" => {
                let id = req.params.get("sessionId").and_then(|s| s.as_str()).unwrap_or_default().to_string();
                let model = req.params.get("modelId").and_then(|s| s.as_str()).unwrap_or_default().to_string();
                if let Some(s) = sessions.lock().await.get(&id).cloned() {
                    let mut g = s.lock().await;
                    let mut c = g.cfg.clone();
                    c.model = model;
                    if let Ok(n) = crate::agent::Session::new(g.id.clone(), g.cwd.clone(), c) {
                        let msgs = std::mem::take(&mut g.messages);
                        *g = n;
                        g.messages = msgs;
                    }
                }
                req.respond(Ok(Value::Null));
            }
            "session/prompt" => {
                let id = req.params.get("sessionId").and_then(|s| s.as_str()).unwrap_or_default().to_string();
                let text: String = req.params.get("prompt").and_then(|p| p.as_array()).into_iter().flatten().filter_map(|b| b.get("text").and_then(|t| t.as_str())).collect::<Vec<_>>().join("\n");
                let Some(s) = sessions.lock().await.get(&id).cloned() else {
                    req.respond(Err((-32602, "unknown session".into())));
                    continue;
                };
                let (tx, rx) = watch::channel(false);
                cancels.lock().await.insert(id.clone(), tx);
                let peer = peer.clone();
                tokio::spawn(async move {
                    let (stop, usage) = s.lock().await.prompt(&peer, &text, &rx).await;
                    let mut resp = json!({ "stopReason": stop });
                    if let Some(u) = usage {
                        resp["usage"] = json!({ "totalTokens": u.input + u.output, "inputTokens": u.input, "outputTokens": u.output });
                    }
                    req.respond(Ok(resp));
                });
            }
            "session/cancel" => {
                let id = req.params.get("sessionId").and_then(|s| s.as_str()).unwrap_or_default().to_string();
                if let Some(tx) = cancels.lock().await.get(&id) {
                    let _ = tx.send(true);
                }
            }
            other => {
                if !req.is_notification() {
                    req.respond(Err((-32601, format!("{other} is not supported"))));
                }
            }
        }
    }
}

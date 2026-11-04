//! Milestone 8: linking another device (a phone) to a person already in the
//! room, and refusing joins that would take over someone else's identity.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use polyph_testkit::harness::{AgentBins, Harness};
use serde_json::{Value, json};

fn bins() -> AgentBins {
    AgentBins {
        fake_agent: env!("CARGO_BIN_EXE_polyph-fake-agent").into(),
        sandbox_helper: env!("CARGO_BIN_EXE_polyph-test-sandbox-exec").into(),
        mcp: env!("CARGO_BIN_EXE_polyph-test-mcp").into(),
    }
}

/// Ann (owner), Bo (member) and a third, not yet joined node.
async fn room() -> (Harness, String) {
    let mut h = Harness::new().await.unwrap();
    h.agents = Some(bins());
    let a = h.add("Ann").await.unwrap();
    let b = h.add("Bo").await.unwrap();
    h.add("Third").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "phones" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    h.until("connected", || async {
        h.nodes[a].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == true
    })
    .await
    .unwrap();
    h.join(&rid, a, b, a, "member").await.unwrap();
    (h, rid)
}

async fn my_member(h: &Harness, i: usize, rid: &str) -> String {
    let m = h.nodes[i].call("room.members", json!({ "room": rid })).await.unwrap();
    m.as_array().unwrap().iter().find(|m| m["me"] == true).unwrap()["member"].as_str().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_join_code_cannot_take_an_existing_members_identity() {
    let (h, rid) = room().await;
    let ann = my_member(&h, 0, &rid).await;
    let inv = h.nodes[0].call("room.invite", json!({ "room": rid })).await.unwrap();
    let err = h.nodes[2]
        .call("join", json!({ "code": inv["code"], "member": ann, "timeout_secs": 10 }))
        .await
        .expect_err("joining as Ann must fail");
    assert!(err.to_string().contains("already in the room"), "{err}");

    // A link code links only the device of the person who made it.
    let link = h.nodes[1].call("room.link", json!({ "room": rid })).await.unwrap();
    let err = h.nodes[2]
        .call("join", json!({ "code": link["code"], "member": ann, "timeout_secs": 10 }))
        .await
        .expect_err("Bo's link code cannot link a device to Ann");
    assert!(err.to_string().contains("links another device"), "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_linked_device_answers_its_persons_approvals() {
    let (mut h, rid) = room().await;
    let (a, b, phone) = (0, 1, 2);
    let bo = my_member(&h, b, &rid).await;

    // Bo links a second device. Bo cannot admit people, so Ann approves the knock.
    let link = h.nodes[b].call("room.link", json!({ "room": rid })).await.unwrap();
    assert_eq!(link["member"], bo);
    let pd = h.nodes[phone].daemon.clone();
    let code = link["code"].clone();
    let bo2 = bo.clone();
    let join = tokio::spawn(async move {
        pd.call("join", json!({ "code": code, "member": bo2, "name": "Bo", "timeout_secs": 20 })).await
    });
    let knock = match h.nodes[a].wait_event("knock", |e| e.kind() == "knock").await {
        Ok(k) => k,
        Err(e) => panic!("{e}; join: {:?}", join.await.map(|r| r.map_err(|e| e.to_string()))),
    };
    assert_eq!(knock.event["knock"]["link"], true);
    assert_eq!(knock.event["knock"]["member"], bo);
    let kid = knock.event["knock"]["id"].as_str().unwrap().to_string();
    h.nodes[a].call("knock.decide", json!({ "room": rid, "knock": kid, "approve": true })).await.unwrap();
    join.await.unwrap().map_err(|e| e.to_string()).unwrap();
    assert_eq!(my_member(&h, phone, &rid).await, bo, "the phone is Bo");
    h.until("Bo shows two devices", || async {
        let m = h.nodes[a].call("room.members", json!({ "room": rid })).await.unwrap();
        m.as_array().unwrap().iter().any(|m| m["member"] == bo.as_str() && m["devices"] == 2)
    })
    .await
    .unwrap();

    // Bo's agent asks; Bo's phone answers at once, without waiting for escalation.
    let dir = tempfile::tempdir().unwrap();
    let script = json!({ "turns": [[ { "permission": { "kind": "execute", "title": "Run cargo publish", "command": "cargo publish" } } ]] });
    std::fs::write(dir.path().join(".fake-script.json"), script.to_string()).unwrap();
    std::fs::write(dir.path().join("polyph.toml"), "").unwrap();
    let r = h.nodes[b]
        .call("agent.start", json!({ "room": rid, "kind": "fake", "cwd": dir.path(), "preset": "default" }))
        .await
        .unwrap();
    let id = r["id"].as_str().unwrap().to_string();
    let bd = h.nodes[b].daemon.clone();
    let id2 = id.clone();
    let prompt = tokio::spawn(async move { bd.call("agent.prompt", json!({ "agent": id2, "text": "publish" })).await });
    let ev = h.nodes[phone].wait_event("approval on the phone", |e| e.kind() == "approval_pending").await.unwrap();
    let approval = ev.event["approval"]["id"].as_str().unwrap().to_string();
    h.nodes[phone].call("approve", json!({ "room": rid, "id": approval })).await.unwrap();
    let r: Value = prompt.await.unwrap().map_err(|e| e.to_string()).unwrap();
    assert_eq!(r["stop_reason"], "end_turn");
}

/// Drives a browser member (`polyph-web`) over a real WebSocket, the way the
/// phone PWA's JavaScript does.
struct Phone {
    m: polyph_web::Member,
    ws: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
}

impl Phone {
    async fn connect(url: &str, name: &str) -> Phone {
        use futures_util::SinkExt;
        let (ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();
        let id = polyph_proto::identity::DeviceIdentity::generate().unwrap();
        let mut p = Phone { m: polyph_web::Member::new(id, name, url), ws };
        p.m.connected();
        for f in p.m.take_frames() {
            p.ws.send(tokio_tungstenite::tungstenite::Message::Binary(f.into())).await.unwrap();
        }
        p
    }

    /// Exchanges frames until `done` sees a matching event (or 15 s pass).
    async fn until(&mut self, now: &dyn Fn() -> i64, done: impl Fn(&Value) -> bool) -> Vec<Value> {
        use futures_util::{SinkExt, StreamExt};
        let mut seen = vec![];
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
        loop {
            for f in self.m.take_frames() {
                self.ws.send(tokio_tungstenite::tungstenite::Message::Binary(f.into())).await.unwrap();
            }
            let evs = self.m.take_events();
            let hit = evs.iter().any(&done);
            seen.extend(evs);
            if hit {
                return seen;
            }
            let left = deadline.saturating_duration_since(tokio::time::Instant::now());
            assert!(!left.is_zero(), "timed out; events: {seen:?}");
            match tokio::time::timeout(left.min(std::time::Duration::from_millis(200)), self.ws.next()).await {
                Ok(Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(b)))) => self.m.on_frame(&b, now()),
                Ok(Some(Ok(_))) | Err(_) => self.m.tick(now()),
                Ok(_) => panic!("relay closed"),
            }
        }
    }

    /// Exchanges frames for `ms` milliseconds.
    async fn pump(&mut self, now: &dyn Fn() -> i64, ms: u64) {
        use futures_util::{SinkExt, StreamExt};
        let end = tokio::time::Instant::now() + std::time::Duration::from_millis(ms);
        while tokio::time::Instant::now() < end {
            for f in self.m.take_frames() {
                self.ws.send(tokio_tungstenite::tungstenite::Message::Binary(f.into())).await.unwrap();
            }
            if let Ok(Some(Ok(tokio_tungstenite::tungstenite::Message::Binary(b)))) =
                tokio::time::timeout(std::time::Duration::from_millis(50), self.ws.next()).await
            {
                self.m.on_frame(&b, now());
            }
        }
    }

    fn call(&mut self, method: &str, params: Value, now: i64) -> Value {
        self.m.call(method, &params, now).unwrap()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_phone_with_no_daemon_links_chats_and_approves() {
    let (h, rid) = room().await;
    let b = 1;
    let bo = my_member(&h, b, &rid).await;
    let clock = h.clock.clone();
    let now = move || clock.now();
    let url = format!("ws://{}/ws", h.relay.addr);

    // Bo links his phone; Ann admits it.
    let link = h.nodes[b].call("room.link", json!({ "room": rid })).await.unwrap();
    let mut phone = Phone::connect(&url, "Bo").await;
    phone.pump(&now, 500).await;
    phone.m.start_join(link["code"].as_str().unwrap(), Some(&bo)).unwrap();
    let ann = h.nodes[0].daemon.clone();
    let rid2 = rid.clone();
    let admit = tokio::spawn(async move {
        let mut rx = ann.subscribe();
        loop {
            let e = rx.recv().await.unwrap();
            if e.kind() == "knock" {
                let kid = e.event["knock"]["id"].as_str().unwrap().to_string();
                ann.call("knock.decide", json!({ "room": rid2, "knock": kid, "approve": true })).await.unwrap();
                return;
            }
        }
    });
    phone.until(&now, |e| e["event"]["type"] == "joined" || e["event"]["type"] == "join_failed").await;
    admit.await.unwrap();
    assert!(phone.m.room.is_some(), "the phone joined");
    phone.pump(&now, 300).await;
    let who = phone.call("whoami", json!({}), now());
    assert_eq!(who["member"], bo, "the phone is Bo");

    // Chat both ways.
    h.nodes[0].call("chat.send", json!({ "room": rid, "text": "hello phone" })).await.unwrap();
    phone.until(&now, |e| e["event"]["type"] == "chat").await;
    let chat = phone.call("chat.list", json!({}), now());
    assert!(chat.as_array().unwrap().iter().any(|m| m["text"] == "hello phone"), "{chat}");
    phone.call("chat.send", json!({ "text": "from the train" }), now());
    phone.pump(&now, 500).await;
    h.until("Ann sees the phone's message", || async {
        let c = h.nodes[0].call("chat.list", json!({ "room": rid })).await.unwrap();
        c.as_array().unwrap().iter().any(|m| m["text"] == "from the train" && m["name"] == "Bo")
    })
    .await
    .unwrap();

    // Bo's agent asks; the phone answers.
    let dir = tempfile::tempdir().unwrap();
    let script = json!({ "turns": [[ { "permission": { "kind": "execute", "title": "Run cargo publish", "command": "cargo publish" } } ]] });
    std::fs::write(dir.path().join(".fake-script.json"), script.to_string()).unwrap();
    std::fs::write(dir.path().join("polyph.toml"), "").unwrap();
    let r = h.nodes[b]
        .call("agent.start", json!({ "room": rid, "kind": "fake", "cwd": dir.path(), "preset": "default" }))
        .await
        .unwrap();
    let id = r["id"].as_str().unwrap().to_string();
    let bd = h.nodes[b].daemon.clone();
    let prompt = tokio::spawn(async move { bd.call("agent.prompt", json!({ "agent": id, "text": "publish" })).await });
    phone.until(&now, |e| e["event"]["type"] == "approval_pending").await;
    let list = phone.call("approvals.list", json!({}), now());
    let aid = list[0]["id"].as_str().unwrap().to_string();
    assert_eq!(list[0]["kind"]["c"]["command"], "cargo publish");
    phone.call("approve", json!({ "id": aid }), now());
    phone.pump(&now, 500).await;
    let r: Value = tokio::time::timeout(std::time::Duration::from_secs(20), prompt)
        .await
        .expect("the agent continued")
        .unwrap()
        .map_err(|e| e.to_string())
        .unwrap();
    assert_eq!(r["stop_reason"], "end_turn");

    // The room survives a save and reload.
    let saved = phone.m.save().unwrap();
    let id2 = polyph_proto::identity::DeviceIdentity::generate().unwrap();
    assert!(polyph_web::Member::load(id2, "Bo", &saved, now()).is_ok());
}

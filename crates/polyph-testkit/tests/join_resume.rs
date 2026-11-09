//! Join codes outlive a host restart, and a joiner waiting for approval
//! survives a dropped connection.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use polyph_proto::codec::{from_cbor, to_cbor};
use polyph_proto::identity::{DeviceIdentity, DevicePublic};
use polyph_proto::ids::MemberId;
use polyph_proto::join::{JoinReply, JoinRequest};
use polyph_proto::joincode::JoinCode;
use polyph_proto::pake::{PakeRole, PakeStart, SecureChannel};
use polyph_proto::relay_wire::{ClientFrame, RelayFrame};
use polyph_relay::client::{RelayClient, TlsMode};
use polyph_testkit::harness::Harness;
use serde_json::json;

async fn hosted_room(h: &mut Harness) -> (usize, usize, String) {
    let a = h.add("Ann").await.unwrap();
    let b = h.add("Bo").await.unwrap();
    let room = h.nodes[a].call("room.create", json!({ "name": "payments" })).await.unwrap();
    let rid = room["id"].as_str().unwrap().to_string();
    connected(h, a, &rid, true).await;
    (a, b, rid)
}

async fn connected(h: &Harness, i: usize, rid: &str, want: bool) {
    h.until("connection state", || async {
        h.nodes[i].call("room.get", json!({ "room": rid })).await.unwrap()["connected"] == want
    })
    .await
    .unwrap();
}

/// The reusable code `polyph start` prints keeps working after the host's
/// daemon restarts, and is kept sealed on disk.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_code_still_works_after_the_host_restarts() {
    let mut h = Harness::new().await.unwrap();
    let (a, b, rid) = hosted_room(&mut h).await;
    let inv = h.nodes[a].call("room.invite", json!({ "room": rid, "reusable": true })).await.unwrap();
    let code = inv["code"].as_str().unwrap().to_string();
    let file = h.nodes[a].daemon.shared.paths.room_codes_file(&rid);
    h.until("codes saved", || async { file.exists() }).await.unwrap();
    let saved = std::fs::read(&file).unwrap();
    let plain = code.replace('-', "");
    assert!(!saved.windows(plain.len()).any(|w| w == plain.as_bytes()), "the code is stored in the clear");

    h.restart(a).await.unwrap();
    connected(&h, a, &rid, true).await;
    // The relay forgot the code when the old daemon left; the new one hosts it.
    h.until("code hosted again", || async { h.relay.hub.stats().2 == 1 }).await.unwrap();

    let bd = h.nodes[b].daemon.clone();
    let mut joining =
        tokio::spawn(async move { bd.call("join", json!({ "code": code, "name": "Bo", "timeout_secs": 30 })).await });
    let knock = tokio::select! {
        k = h.nodes[a].wait_event("knock", |e| e.kind() == "knock") => k.unwrap(),
        r = &mut joining => panic!("join ended early: {:?}", r.unwrap().map_err(|e| e.to_string())),
    };
    let kid = knock.event["knock"]["id"].as_str().unwrap().to_string();
    h.nodes[a].call("knock.decide", json!({ "room": rid, "knock": kid, "approve": true })).await.unwrap();
    let joined = joining.await.unwrap().map_err(|e| e.to_string()).unwrap();
    assert_eq!(joined["id"], rid.as_str());
}

/// The host's connection drops while Bo waits: Bo reconnects with the same
/// one-time code, takes his knock back and gets the answer Ann gave while
/// the host was away.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_waiting_joiner_gets_an_answer_given_while_the_host_was_away() {
    let mut h = Harness::new().await.unwrap();
    let (a, b, rid) = hosted_room(&mut h).await;
    let inv = h.nodes[a].call("room.invite", json!({ "room": rid })).await.unwrap();
    let code = inv["code"].as_str().unwrap().to_string();
    let bd = h.nodes[b].daemon.clone();
    let joining =
        tokio::spawn(async move { bd.call("join", json!({ "code": code, "name": "Bo", "timeout_secs": 60 })).await });
    let knock = h.nodes[a].wait_event("knock", |e| e.kind() == "knock").await.unwrap();
    h.nodes[b].wait_event("pending", |e| e.kind() == "join_pending").await.unwrap();
    let kid = knock.event["knock"]["id"].as_str().unwrap().to_string();

    h.nodes[a].call("room.offline", json!({ "room": rid, "offline": true })).await.unwrap();
    connected(&h, a, &rid, false).await;
    h.nodes[a].call("knock.decide", json!({ "room": rid, "knock": kid, "approve": true })).await.unwrap();
    h.nodes[a].call("room.offline", json!({ "room": rid, "offline": false })).await.unwrap();

    let joined = joining.await.unwrap().map_err(|e| e.to_string()).unwrap();
    assert_eq!(joined["id"], rid.as_str());
}

/// The host's connection drops while Bo waits and comes back before anyone
/// answers: Bo's knock is still the one waiting, and approving it lets him in.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_waiting_joiner_keeps_its_knock_across_a_host_reconnect() {
    let mut h = Harness::new().await.unwrap();
    let (a, b, rid) = hosted_room(&mut h).await;
    let inv = h.nodes[a].call("room.invite", json!({ "room": rid })).await.unwrap();
    let code = inv["code"].as_str().unwrap().to_string();
    let bd = h.nodes[b].daemon.clone();
    let joining =
        tokio::spawn(async move { bd.call("join", json!({ "code": code, "name": "Bo", "timeout_secs": 60 })).await });
    let knock = h.nodes[a].wait_event("knock", |e| e.kind() == "knock").await.unwrap();
    h.nodes[b].wait_event("pending", |e| e.kind() == "join_pending").await.unwrap();
    let kid = knock.event["knock"]["id"].as_str().unwrap().to_string();

    h.nodes[a].call("room.offline", json!({ "room": rid, "offline": true })).await.unwrap();
    connected(&h, a, &rid, false).await;
    h.nodes[a].call("room.offline", json!({ "room": rid, "offline": false })).await.unwrap();
    connected(&h, a, &rid, true).await;
    // Bo is back on his knock: a second "pending" answer reaches him.
    h.nodes[b].wait_event("pending again", |e| e.kind() == "join_pending").await.unwrap();

    let listed = h.nodes[a].call("knock.list", json!({ "room": rid })).await.unwrap();
    let ids: Vec<&str> = listed.as_array().unwrap().iter().filter_map(|k| k["id"].as_str()).collect();
    assert_eq!(ids, vec![kid.as_str()], "one knock, the original");
    h.nodes[a].call("knock.decide", json!({ "room": rid, "knock": kid, "approve": true })).await.unwrap();
    let joined = joining.await.unwrap().map_err(|e| e.to_string()).unwrap();
    assert_eq!(joined["id"], rid.as_str());
}

/// A join made by hand, so a test chooses every field of the request.
struct RawJoin {
    client: RelayClient,
    channel: SecureChannel,
    session: u64,
}

async fn pake(client: &mut RelayClient, session: u64) -> Option<Vec<u8>> {
    loop {
        match client.recv_timeout(Duration::from_secs(10)).await? {
            RelayFrame::Pake { session: s, data } if s == session => return Some(data),
            RelayFrame::PakeEnded { session: s, .. } if s == session => return None,
            _ => continue,
        }
    }
}

impl RawJoin {
    /// Proves `code` from `identity`'s connection and sends a request naming
    /// `device`, which need not be `identity`'s own keys.
    async fn start(
        relay: &str,
        identity: &DeviceIdentity,
        code: &str,
        device: DevicePublic,
        member: MemberId,
        name_salt: &[u8],
    ) -> Self {
        let code = JoinCode::parse(code).unwrap();
        let mut client = RelayClient::connect_with(relay, identity, TlsMode::Public).await.unwrap();
        client.send(ClientFrame::JoinCode { rendezvous: code.rendezvous() }).await.unwrap();
        let session = loop {
            match client.recv_timeout(Duration::from_secs(10)).await.expect("join started") {
                RelayFrame::JoinStarted { session } => break session,
                RelayFrame::Error { detail, .. } => panic!("relay refused the code: {detail}"),
                _ => continue,
            }
        };
        let start = PakeStart::new(&code, PakeRole::Joiner, &[]);
        client.send(ClientFrame::Pake { session, data: start.message().to_vec() }).await.unwrap();
        let fin = start.finish(&pake(&mut client, session).await.unwrap()).unwrap();
        client.send(ClientFrame::Pake { session, data: fin.confirmation().to_vec() }).await.unwrap();
        let mut channel = fin.verify_peer(&pake(&mut client, session).await.unwrap()).unwrap();
        let req = JoinRequest {
            device,
            member,
            display_name: "Bo".into(),
            name_salt: name_salt.to_vec(),
            surface: "cli".into(),
            link_device: false,
            capabilities: vec![],
        };
        let body = channel.seal(&to_cbor(&req).unwrap()).unwrap();
        client.send(ClientFrame::Pake { session, data: body }).await.unwrap();
        Self { client, channel, session }
    }

    /// The host's next answer, or `None` when the session ended.
    async fn reply(&mut self) -> Option<JoinReply> {
        let data = pake(&mut self.client, self.session).await?;
        Some(from_cbor(&self.channel.open(&data).unwrap()).unwrap())
    }
}

/// A one-time code Bo used is held for Bo's exact request while he
/// reconnects. Nobody else gets in through it: not Chai, not Eve naming Bo's
/// (public) device id with her own key, not even a fresh request from Bo
/// with another name salt. None of them disturbs Bo's waiting knock, and Bo,
/// back with the same request, gets the answer.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_held_one_time_code_lets_only_its_exact_joiner_back() {
    let mut h = Harness::new().await.unwrap();
    let (a, _b, rid) = hosted_room(&mut h).await;
    let relay = h.relay.url();
    let inv = h.nodes[a].call("room.invite", json!({ "room": rid })).await.unwrap();
    let code = inv["code"].as_str().unwrap().to_string();
    let bo = DeviceIdentity::generate().unwrap();
    let bo_member = MemberId::random().unwrap();
    let salt = [7u8; 16];

    let mut first = RawJoin::start(&relay, &bo, &code, bo.public(), bo_member, &salt).await;
    assert_eq!(first.reply().await, Some(JoinReply::Pending));
    let knock = h.nodes[a].wait_event("knock", |e| e.kind() == "knock").await.unwrap();
    let kid = knock.event["knock"]["id"].as_str().unwrap().to_string();

    // The host drops while Bo waits; the relay ends Bo's session.
    h.nodes[a].call("room.offline", json!({ "room": rid, "offline": true })).await.unwrap();
    connected(&h, a, &rid, false).await;
    assert_eq!(first.reply().await, None);
    drop(first);
    h.nodes[a].call("room.offline", json!({ "room": rid, "offline": false })).await.unwrap();
    connected(&h, a, &rid, true).await;
    h.until("code held for Bo", || async { h.relay.hub.stats().2 == 1 }).await.unwrap();

    let denied = Some(JoinReply::Denied { reason: "this code was already used".into() });
    let chai = DeviceIdentity::generate().unwrap();
    let mut j = RawJoin::start(&relay, &chai, &code, chai.public(), MemberId::random().unwrap(), &salt).await;
    assert_eq!(j.reply().await, denied, "Chai");
    let eve = DeviceIdentity::generate().unwrap();
    let posing = DevicePublic { sign: bo.device_id(), kx: eve.public().kx };
    h.until("code held again", || async { h.relay.hub.stats().2 == 1 }).await.unwrap();
    let mut j = RawJoin::start(&relay, &eve, &code, posing, bo_member, &salt).await;
    assert_eq!(j.reply().await, denied, "Eve with Bo's device id");
    h.until("code held again", || async { h.relay.hub.stats().2 == 1 }).await.unwrap();
    let mut j = RawJoin::start(&relay, &bo, &code, bo.public(), bo_member, &[8u8; 16]).await;
    assert_eq!(j.reply().await, denied, "Bo with another salt");

    let listed = h.nodes[a].call("knock.list", json!({ "room": rid })).await.unwrap();
    let ids: Vec<&str> = listed.as_array().unwrap().iter().filter_map(|k| k["id"].as_str()).collect();
    assert_eq!(ids, vec![kid.as_str()], "Bo's knock still waits, alone");

    h.until("code held again", || async { h.relay.hub.stats().2 == 1 }).await.unwrap();
    let mut back = RawJoin::start(&relay, &bo, &code, bo.public(), bo_member, &salt).await;
    assert_eq!(back.reply().await, Some(JoinReply::Pending), "Bo takes his knock back");
    h.nodes[a].call("knock.decide", json!({ "room": rid, "knock": kid, "approve": true })).await.unwrap();
    match back.reply().await {
        Some(JoinReply::Approved(ap)) => {
            assert_eq!(ap.room.to_hex(), rid);
        }
        other => panic!("expected approval, got {other:?}"),
    }
}

/// Bo reconnects before the relay notices his old connection is gone (a
/// phone that switched networks): the same request moves his knock to the
/// new connection instead of knocking twice, and the answer reaches him there.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_joiner_back_before_its_old_connection_closed_keeps_one_knock() {
    let mut h = Harness::new().await.unwrap();
    let (a, _b, rid) = hosted_room(&mut h).await;
    let relay = h.relay.url();
    let inv = h.nodes[a].call("room.invite", json!({ "room": rid, "reusable": true })).await.unwrap();
    let code = inv["code"].as_str().unwrap().to_string();
    let bo = DeviceIdentity::generate().unwrap();
    let member = MemberId::random().unwrap();
    let salt = [3u8; 16];

    let mut old = RawJoin::start(&relay, &bo, &code, bo.public(), member, &salt).await;
    assert_eq!(old.reply().await, Some(JoinReply::Pending));
    let knock = h.nodes[a].wait_event("knock", |e| e.kind() == "knock").await.unwrap();
    let kid = knock.event["knock"]["id"].as_str().unwrap().to_string();

    let mut new = RawJoin::start(&relay, &bo, &code, bo.public(), member, &salt).await;
    assert_eq!(new.reply().await, Some(JoinReply::Pending));
    let listed = h.nodes[a].call("knock.list", json!({ "room": rid })).await.unwrap();
    let ids: Vec<&str> = listed.as_array().unwrap().iter().filter_map(|k| k["id"].as_str()).collect();
    assert_eq!(ids, vec![kid.as_str()], "one knock");

    h.nodes[a].call("knock.decide", json!({ "room": rid, "knock": kid, "approve": true })).await.unwrap();
    match new.reply().await {
        Some(JoinReply::Approved(ap)) => assert_eq!(ap.room.to_hex(), rid),
        other => panic!("expected approval on the new connection, got {other:?}"),
    }
    drop(old);
}

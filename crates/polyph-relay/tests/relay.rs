//! Relay acceptance tests (milestone 1).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use polyph_proto::cert::{CertChain, MembershipCert, Role, RoomGenesis, name_commitment};
use polyph_proto::codec::{from_cbor, to_cbor};
use polyph_proto::delivery::{open_epoch_key, seal_epoch_key};
use polyph_proto::envelope::Envelope;
use polyph_proto::group::{EpochKey, Keyring};
use polyph_proto::identity::{DeviceIdentity, ctx};
use polyph_proto::ids::{MemberId, ObjectId, RoomId};
use polyph_proto::join::{JoinApproved, JoinReply, JoinRequest};
use polyph_proto::joincode::JoinCode;
use polyph_proto::messages::{ChatMessage, RoomMessage};
use polyph_proto::pake::{PakeRole, PakeStart};
use polyph_proto::relay_wire::{ClientFrame, CodeClosed, CodeKind, ErrorCode, RelayFrame, end_room_message};
use polyph_relay::client::RelayClient;
use polyph_relay::{Limits, RelayConfig, RelayHandle, ServeOptions, serve};

const T: Duration = Duration::from_secs(5);

struct Member {
    id: DeviceIdentity,
    member: MemberId,
    chain: CertChain,
}

struct Room {
    owner: Member,
    room: RoomId,
}

fn now() -> i64 {
    polyph_proto::now()
}

fn new_room() -> Room {
    let id = DeviceIdentity::generate().unwrap();
    let member = MemberId::random().unwrap();
    let (g, room) = RoomGenesis::create(&id, member, now(), false).unwrap();
    let chain = CertChain::for_owner(&id, g, member, name_commitment("Owner", &[0; 16]), now()).unwrap();
    Room { owner: Member { id, member, chain }, room }
}

fn add_member(r: &Room, role: Role) -> Member {
    let id = DeviceIdentity::generate().unwrap();
    let member = MemberId::random().unwrap();
    let cert = MembershipCert {
        room: r.room,
        member,
        device: id.public(),
        name_commit: name_commitment("M", &[1; 16]),
        role,
        issued_at: now(),
        expires_at: None,
        capabilities: vec![],
    };
    let chain = r.owner.chain.issue(&r.owner.id, &cert).unwrap();
    Member { id, member, chain }
}

async fn start(cfg: RelayConfig, capture: Option<polyph_relay::Capture>) -> RelayHandle {
    serve(cfg, ServeOptions { capture, ..Default::default() }).await.unwrap()
}

async fn client(h: &RelayHandle, m: &DeviceIdentity) -> RelayClient {
    RelayClient::connect(&h.url(), m, None).await.unwrap()
}

/// Receives until `pred` matches, skipping other frames.
async fn expect<F: Fn(&RelayFrame) -> bool>(c: &mut RelayClient, what: &str, pred: F) -> RelayFrame {
    let deadline = tokio::time::Instant::now() + T;
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        match c.recv_timeout(left).await {
            Some(f) if pred(&f) => return f,
            Some(_) => continue,
            None => panic!("timed out waiting for {what}"),
        }
    }
}

async fn subscribe(c: &mut RelayClient, room: RoomId, chain: Option<&CertChain>, after: Option<u64>) -> bool {
    c.send(ClientFrame::Subscribe { room, chain: chain.map(|c| Box::new(c.clone())), after }).await.unwrap();
    match expect(c, "subscribed", |f| matches!(f, RelayFrame::Subscribed { .. } | RelayFrame::Error { .. })).await {
        RelayFrame::Subscribed { writer, .. } => {
            expect(c, "caught up", |f| matches!(f, RelayFrame::CaughtUp { .. })).await;
            writer
        }
        RelayFrame::Error { code, detail } => panic!("subscribe failed: {code:?} {detail}"),
        _ => unreachable!(),
    }
}

fn chat(text: &str) -> Vec<u8> {
    RoomMessage::Chat(ChatMessage {
        id: ObjectId::random().unwrap(),
        thread: None,
        author: MemberId([0; 16]),
        agent: None,
        text: text.into(),
        at: now(),
    })
    .encode()
    .unwrap()
}

async fn publish(c: &RelayClient, key: &EpochKey, id: &DeviceIdentity, seq: u64, text: &str) -> Envelope {
    let env = key.seal(id, seq, &chat(text)).unwrap();
    c.send(ClientFrame::Publish { envelope: env.encode() }).await.unwrap();
    env
}

fn text_of(pt: &[u8]) -> String {
    match RoomMessage::decode(pt).unwrap() {
        RoomMessage::Chat(c) => c.text,
        other => panic!("expected chat, got {other:?}"),
    }
}

#[tokio::test]
async fn members_exchange_messages_and_relay_never_sees_plaintext() {
    let capture: polyph_relay::Capture = Arc::new(Mutex::new(Vec::new()));
    let h = start(RelayConfig::ephemeral(), Some(capture.clone())).await;
    let r = new_room();
    let b = add_member(&r, Role::Member);
    let key = EpochKey::generate(r.room, 1).unwrap();

    let mut ca = client(&h, &r.owner.id).await;
    let mut cb = client(&h, &b.id).await;
    assert!(subscribe(&mut ca, r.room, Some(&r.owner.chain), None).await);
    assert!(subscribe(&mut cb, r.room, Some(&b.chain), None).await);

    // Key delivered over HPKE through the relay too.
    let delivery = seal_epoch_key(&r.owner.id, &b.id.public(), &key).unwrap();
    let d_bytes = to_cbor(&delivery).unwrap();
    assert!(!d_bytes.windows(32).any(|w| w == key.secret_bytes()));

    let marker = "PLAINTEXT-MARKER-the-launch-codes-are-hunter2";
    publish(&ca, &key, &r.owner.id, 1, marker).await;
    let f = expect(&mut cb, "delivery", |f| matches!(f, RelayFrame::Deliver { .. })).await;
    let RelayFrame::Deliver { envelope, .. } = f else { unreachable!() };
    let got = open_epoch_key(&b.id, &from_cbor(&d_bytes).unwrap()).unwrap();
    let mut ring = Keyring::default();
    ring.insert(got);
    let env = Envelope::decode(&envelope).unwrap();
    assert_eq!(text_of(&ring.open(&env).unwrap()), marker);

    // A replay of the same envelope is forwarded by the relay but rejected by the client.
    ca.send(ClientFrame::Publish { envelope: env.encode() }).await.unwrap();
    let f = expect(&mut cb, "replayed delivery", |f| matches!(f, RelayFrame::Deliver { .. })).await;
    let RelayFrame::Deliver { envelope, .. } = f else { unreachable!() };
    assert!(ring.open(&Envelope::decode(&envelope).unwrap()).is_err());

    // Every byte the relay received, inspected for the plaintext and the key.
    let frames = capture.lock().clone();
    assert!(frames.len() >= 4);
    for fr in &frames {
        assert!(!fr.windows(marker.len()).any(|w| w == marker.as_bytes()), "plaintext reached the relay");
        assert!(!fr.windows(32).any(|w| w == key.secret_bytes()), "epoch key reached the relay");
    }
    h.shutdown().await;
}

#[tokio::test]
async fn spectators_read_but_cannot_publish_and_senders_cannot_be_forged() {
    let h = start(RelayConfig::ephemeral(), None).await;
    let r = new_room();
    let key = EpochKey::generate(r.room, 1).unwrap();
    let mut ca = client(&h, &r.owner.id).await;
    subscribe(&mut ca, r.room, Some(&r.owner.chain), None).await;

    let spectator = DeviceIdentity::generate().unwrap();
    let mut cs = client(&h, &spectator).await;
    assert!(!subscribe(&mut cs, r.room, None, None).await);

    publish(&ca, &key, &r.owner.id, 1, "hello").await;
    expect(&mut cs, "spectator delivery", |f| matches!(f, RelayFrame::Deliver { .. })).await;

    let env = key.seal(&spectator, 1, &chat("spam")).unwrap();
    cs.send(ClientFrame::Publish { envelope: env.encode() }).await.unwrap();
    expect(&mut cs, "not member", |f| matches!(f, RelayFrame::Error { code: ErrorCode::NotMember, .. })).await;

    // A member publishing an envelope that claims another sender.
    let env = key.seal(&spectator, 2, &chat("forged")).unwrap();
    ca.send(ClientFrame::Publish { envelope: env.encode() }).await.unwrap();
    expect(&mut ca, "forbidden", |f| matches!(f, RelayFrame::Error { code: ErrorCode::Forbidden, .. })).await;

    // Certificate for another room is refused.
    let other = new_room();
    let mut cx = client(&h, &other.owner.id).await;
    cx.send(ClientFrame::Subscribe { room: r.room, chain: Some(Box::new(other.owner.chain.clone())), after: None })
        .await
        .unwrap();
    expect(&mut cx, "not member", |f| matches!(f, RelayFrame::Error { code: ErrorCode::NotMember, .. })).await;
    h.shutdown().await;
}

#[tokio::test]
async fn removed_member_cannot_decrypt_after_rotation() {
    let h = start(RelayConfig::ephemeral(), None).await;
    let r = new_room();
    let b = add_member(&r, Role::Member);
    let c = add_member(&r, Role::Member);
    let k1 = EpochKey::generate(r.room, 1).unwrap();
    let mut ca = client(&h, &r.owner.id).await;
    let mut cb = client(&h, &b.id).await;
    let mut cc = client(&h, &c.id).await;
    for (cl, m) in [(&mut ca, &r.owner), (&mut cb, &b), (&mut cc, &c)] {
        subscribe(cl, r.room, Some(&m.chain), None).await;
    }
    let mut ring_b = Keyring::default();
    let mut ring_c = Keyring::default();
    ring_b.insert(open_epoch_key(&b.id, &seal_epoch_key(&r.owner.id, &b.id.public(), &k1).unwrap()).unwrap());
    ring_c.insert(open_epoch_key(&c.id, &seal_epoch_key(&r.owner.id, &c.id.public(), &k1).unwrap()).unwrap());

    // C is removed: new epoch delivered to B only.
    let k2 = EpochKey::generate(r.room, 2).unwrap();
    let d = seal_epoch_key(&r.owner.id, &b.id.public(), &k2).unwrap();
    assert!(open_epoch_key(&c.id, &d).is_err());
    ring_b.insert(open_epoch_key(&b.id, &d).unwrap());

    publish(&ca, &k2, &r.owner.id, 1, "after removal").await;
    for (cl, ring, should) in [(&mut cb, &mut ring_b, true), (&mut cc, &mut ring_c, false)] {
        let f = expect(cl, "delivery", |f| matches!(f, RelayFrame::Deliver { .. })).await;
        let RelayFrame::Deliver { envelope, .. } = f else { unreachable!() };
        let res = ring.open(&Envelope::decode(&envelope).unwrap());
        assert_eq!(res.is_ok(), should);
    }
    h.shutdown().await;
}

#[tokio::test]
async fn reconnecting_member_catches_up_from_buffer_or_gets_gap() {
    let h = start(RelayConfig::ephemeral(), None).await;
    let r = new_room();
    let b = add_member(&r, Role::Member);
    let key = EpochKey::generate(r.room, 1).unwrap();
    let mut ca = client(&h, &r.owner.id).await;
    subscribe(&mut ca, r.room, Some(&r.owner.chain), None).await;
    let mut cb = client(&h, &b.id).await;
    subscribe(&mut cb, r.room, Some(&b.chain), None).await;
    publish(&ca, &key, &r.owner.id, 1, "one").await;
    let RelayFrame::Deliver { cursor, .. } =
        expect(&mut cb, "first", |f| matches!(f, RelayFrame::Deliver { .. })).await
    else {
        unreachable!()
    };
    drop(cb);

    for (i, t) in ["two", "three", "four"].iter().enumerate() {
        publish(&ca, &key, &r.owner.id, 2 + i as u64, t).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;

    let mut cb = client(&h, &b.id).await;
    cb.send(ClientFrame::Subscribe { room: r.room, chain: Some(Box::new(b.chain.clone())), after: Some(cursor) })
        .await
        .unwrap();
    let mut texts = vec![];
    while let RelayFrame::Deliver { envelope, .. } =
        expect(&mut cb, "replay", |f| matches!(f, RelayFrame::Deliver { .. } | RelayFrame::CaughtUp { .. })).await
    {
        texts.push(text_of(&key.open(&Envelope::decode(&envelope).unwrap()).unwrap()));
    }
    assert_eq!(texts, vec!["two", "three", "four"]);

    // A cursor from the future (the relay restarted) means a gap.
    let mut cb2 = client(&h, &b.id).await;
    cb2.send(ClientFrame::Subscribe { room: r.room, chain: Some(Box::new(b.chain.clone())), after: Some(9999) })
        .await
        .unwrap();
    expect(&mut cb2, "gap", |f| matches!(f, RelayFrame::Gap { .. })).await;
    h.shutdown().await;
}

#[tokio::test]
async fn buffer_is_bounded_by_count() {
    let mut cfg = RelayConfig::ephemeral();
    cfg.buffer_max_envelopes = 3;
    let h = start(cfg, None).await;
    let r = new_room();
    let key = EpochKey::generate(r.room, 1).unwrap();
    let mut ca = client(&h, &r.owner.id).await;
    subscribe(&mut ca, r.room, Some(&r.owner.chain), None).await;
    for i in 1..=10 {
        publish(&ca, &key, &r.owner.id, i, "x").await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(h.hub.room_buffer_len(&r.room), 3);
    h.shutdown().await;
}

#[tokio::test]
async fn snapshots_survive_everyone_leaving_and_only_members_write() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = RelayConfig::ephemeral();
    cfg.database = dir.path().join("relay.db").to_string_lossy().into_owned();
    let h = start(cfg.clone(), None).await;
    let r = new_room();
    let mut ca = client(&h, &r.owner.id).await;
    subscribe(&mut ca, r.room, Some(&r.owner.chain), None).await;
    ca.send(ClientFrame::PutSnapshot { room: r.room, epoch: 1, blob: vec![7; 1000] }).await.unwrap();

    let stranger = DeviceIdentity::generate().unwrap();
    let mut cs = client(&h, &stranger).await;
    subscribe(&mut cs, r.room, None, None).await;
    cs.send(ClientFrame::PutSnapshot { room: r.room, epoch: 1, blob: vec![0; 10] }).await.unwrap();
    expect(&mut cs, "not member", |f| matches!(f, RelayFrame::Error { code: ErrorCode::NotMember, .. })).await;
    drop(ca);
    drop(cs);
    tokio::time::sleep(Duration::from_millis(200)).await;
    h.shutdown().await;

    // Relay restarts; the snapshot is still there.
    let h = start(cfg, None).await;
    let mut ca = client(&h, &r.owner.id).await;
    subscribe(&mut ca, r.room, Some(&r.owner.chain), None).await;
    ca.send(ClientFrame::GetSnapshots { room: r.room }).await.unwrap();
    let RelayFrame::Snapshots { items, .. } =
        expect(&mut ca, "snapshots", |f| matches!(f, RelayFrame::Snapshots { .. })).await
    else {
        unreachable!()
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].blob, vec![7; 1000]);
    assert_eq!(items[0].from, r.owner.id.device_id());
    h.shutdown().await;
}

#[tokio::test]
async fn plan_member_limit_is_enforced() {
    let mut cfg = RelayConfig::ephemeral();
    cfg.limits = Limits { max_members: Some(2), retention_days: Some(7) };
    let h = start(cfg, None).await;
    let r = new_room();
    let b = add_member(&r, Role::Member);
    let c = add_member(&r, Role::Member);
    let mut ca = client(&h, &r.owner.id).await;
    subscribe(&mut ca, r.room, Some(&r.owner.chain), None).await;
    let mut cb = client(&h, &b.id).await;
    subscribe(&mut cb, r.room, Some(&b.chain), None).await;
    let mut cc = client(&h, &c.id).await;
    cc.send(ClientFrame::Subscribe { room: r.room, chain: Some(Box::new(c.chain.clone())), after: None })
        .await
        .unwrap();
    expect(&mut cc, "room full", |f| matches!(f, RelayFrame::Error { code: ErrorCode::RoomFull, .. })).await;
    // Existing members can reconnect.
    let mut cb2 = client(&h, &b.id).await;
    subscribe(&mut cb2, r.room, Some(&b.chain), None).await;
    h.shutdown().await;
}

#[tokio::test]
async fn only_owner_can_end_a_room() {
    let h = start(RelayConfig::ephemeral(), None).await;
    let owner = DeviceIdentity::generate().unwrap();
    let member = MemberId::random().unwrap();
    let (g, room) = RoomGenesis::create(&owner, member, now(), true).unwrap();
    let chain = CertChain::for_owner(&owner, g.clone(), member, [0; 32], now()).unwrap();
    let r = Room { owner: Member { id: owner, member, chain }, room };
    let b = add_member(&r, Role::Maintainer);

    let mut ca = client(&h, &r.owner.id).await;
    ca.send(ClientFrame::RegisterRoom { genesis: g }).await.unwrap();
    subscribe(&mut ca, room, Some(&r.owner.chain), None).await;
    ca.send(ClientFrame::PutSnapshot { room, epoch: 1, blob: vec![1; 10] }).await.unwrap();
    let mut cb = client(&h, &b.id).await;
    subscribe(&mut cb, room, Some(&b.chain), None).await;

    let at = now();
    let sig = b.id.sign(ctx::ROOM_END, &end_room_message(&room, at));
    cb.send(ClientFrame::EndRoom { room, at, sig }).await.unwrap();
    expect(&mut cb, "forbidden", |f| matches!(f, RelayFrame::Error { code: ErrorCode::Forbidden, .. })).await;

    let sig = r.owner.id.sign(ctx::ROOM_END, &end_room_message(&room, at));
    ca.send(ClientFrame::EndRoom { room, at, sig }).await.unwrap();
    expect(&mut cb, "room ended", |f| matches!(f, RelayFrame::RoomEnded { .. })).await;
    assert!(h.hub.store.snapshots(&room).await.unwrap().is_empty());
    let _ = r.owner.member;
    h.shutdown().await;
}

/// Runs the host side of the join handshake for one session. Returns whether
/// the PAKE succeeded, and replies with `reply` when it did.
async fn host_one_join(host: &mut RelayClient, code: &JoinCode, reply: Option<JoinReply>) -> bool {
    let RelayFrame::JoinerArrived { session, .. } =
        expect(host, "joiner", |f| matches!(f, RelayFrame::JoinerArrived { .. })).await
    else {
        unreachable!()
    };
    let p = PakeStart::new(code, PakeRole::Host, b"");
    host.send(ClientFrame::Pake { session, data: p.message().to_vec() }).await.unwrap();
    let RelayFrame::Pake { data, .. } =
        expect(host, "joiner pake", |f| matches!(f, RelayFrame::Pake { .. } | RelayFrame::PakeEnded { .. })).await
    else {
        return false;
    };
    let Ok(fin) = p.finish(&data) else {
        host.send(ClientFrame::PakeResult { session, ok: false }).await.unwrap();
        return false;
    };
    host.send(ClientFrame::Pake { session, data: fin.confirmation().to_vec() }).await.unwrap();
    let RelayFrame::Pake { data: tag, .. } =
        expect(host, "joiner confirm", |f| matches!(f, RelayFrame::Pake { .. } | RelayFrame::PakeEnded { .. })).await
    else {
        return false;
    };
    match fin.verify_peer(&tag) {
        Ok(mut ch) => {
            host.send(ClientFrame::PakeResult { session, ok: true }).await.unwrap();
            let RelayFrame::Pake { data: req, .. } =
                expect(host, "join request", |f| matches!(f, RelayFrame::Pake { .. })).await
            else {
                unreachable!()
            };
            let req: JoinRequest = from_cbor(&ch.open(&req).unwrap()).unwrap();
            assert_eq!(req.display_name, "Mai");
            if let Some(reply) = reply {
                let ct = ch.seal(&to_cbor(&reply).unwrap()).unwrap();
                host.send(ClientFrame::Pake { session, data: ct }).await.unwrap();
            }
            true
        }
        Err(_) => {
            host.send(ClientFrame::PakeResult { session, ok: false }).await.unwrap();
            false
        }
    }
}

/// Joiner side. Returns the reply on success, or None when the PAKE failed.
async fn join_with(j: &mut RelayClient, me: &DeviceIdentity, typed: &JoinCode) -> Result<Option<JoinReply>, ErrorCode> {
    j.send(ClientFrame::JoinCode { rendezvous: typed.rendezvous() }).await.unwrap();
    let session =
        match expect(j, "join started", |f| matches!(f, RelayFrame::JoinStarted { .. } | RelayFrame::Error { .. }))
            .await
        {
            RelayFrame::JoinStarted { session } => session,
            RelayFrame::Error { code, .. } => return Err(code),
            _ => unreachable!(),
        };
    let p = PakeStart::new(typed, PakeRole::Joiner, b"");
    j.send(ClientFrame::Pake { session, data: p.message().to_vec() }).await.unwrap();
    let RelayFrame::Pake { data, .. } = expect(j, "host pake", |f| matches!(f, RelayFrame::Pake { .. })).await else {
        unreachable!()
    };
    let fin = p.finish(&data).unwrap();
    j.send(ClientFrame::Pake { session, data: fin.confirmation().to_vec() }).await.unwrap();
    let tag = match expect(j, "host confirm", |f| matches!(f, RelayFrame::Pake { .. } | RelayFrame::PakeEnded { .. }))
        .await
    {
        RelayFrame::Pake { data, .. } => data,
        _ => return Ok(None),
    };
    let Ok(mut ch) = fin.verify_peer(&tag) else { return Ok(None) };
    let req = JoinRequest {
        device: me.public(),
        member: MemberId::random().unwrap(),
        display_name: "Mai".into(),
        name_salt: vec![2; 16],
        surface: "cli".into(),
        link_device: false,
    };
    j.send(ClientFrame::Pake { session, data: ch.seal(&to_cbor(&req).unwrap()).unwrap() }).await.unwrap();
    let RelayFrame::Pake { data, .. } = expect(j, "reply", |f| matches!(f, RelayFrame::Pake { .. })).await else {
        unreachable!()
    };
    Ok(Some(from_cbor(&ch.open(&data).unwrap()).unwrap()))
}

#[tokio::test]
async fn joining_with_the_right_code_delivers_certificate_and_key() {
    let capture: polyph_relay::Capture = Arc::new(Mutex::new(Vec::new()));
    let h = start(RelayConfig::ephemeral(), Some(capture.clone())).await;
    let r = new_room();
    let code = JoinCode::generate().unwrap();
    let mut host = client(&h, &r.owner.id).await;
    host.send(ClientFrame::HostCode {
        rendezvous: code.rendezvous(),
        kind: CodeKind::OneTime,
        expires_at: now() + 900,
    })
    .await
    .unwrap();
    expect(&mut host, "hosted", |f| matches!(f, RelayFrame::CodeHosted { .. })).await;

    let joiner_id = DeviceIdentity::generate().unwrap();
    let key = EpochKey::generate(r.room, 1).unwrap();
    let cert = MembershipCert {
        room: r.room,
        member: MemberId::random().unwrap(),
        device: joiner_id.public(),
        name_commit: name_commitment("Mai", &[2; 16]),
        role: Role::Member,
        issued_at: now(),
        expires_at: None,
        capabilities: vec![],
    };
    let reply = JoinReply::Approved(Box::new(JoinApproved {
        room: r.room,
        room_name: Some("payments".into()),
        role: Role::Member,
        chain: r.owner.chain.issue(&r.owner.id, &cert).unwrap(),
        key: seal_epoch_key(&r.owner.id, &joiner_id.public(), &key).unwrap(),
        relay: None,
    }));

    let mut j = client(&h, &joiner_id).await;
    let (host_ok, joined) =
        tokio::join!(host_one_join(&mut host, &code, Some(reply)), join_with(&mut j, &joiner_id, &code));
    assert!(host_ok);
    let Some(JoinReply::Approved(a)) = joined.unwrap() else { panic!("not approved") };
    a.chain.verify(now(), &|_| false).unwrap();
    assert_eq!(open_epoch_key(&joiner_id, &a.key).unwrap().secret_bytes(), key.secret_bytes());

    // One-time code is gone after use.
    expect(&mut host, "code used", |f| matches!(f, RelayFrame::CodeClosed { reason: CodeClosed::Used, .. })).await;
    let mut j2 = client(&h, &DeviceIdentity::generate().unwrap()).await;
    assert_eq!(join_with(&mut j2, &joiner_id, &code).await.unwrap_err(), ErrorCode::CodeUnknown);

    // The relay saw neither the code's secret half nor the display name.
    let secret_half = &code.as_str()[4..];
    for f in capture.lock().iter() {
        assert!(
            !f.windows(4).any(|w| w == secret_half.as_bytes()) || f.len() > 200,
            "code secret leaked in a small frame"
        );
        assert!(!f.windows(8).any(|w| w == b"payments"));
    }
    h.shutdown().await;
}

fn wrong_code(code: &JoinCode, i: u8) -> JoinCode {
    let s = code.as_str();
    let alphabet = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut last = alphabet[(i as usize) % 32];
    if last == s.as_bytes()[7] {
        last = alphabet[(i as usize + 1) % 32];
    }
    JoinCode::parse(&format!("{}{}", &s[..7], last as char)).unwrap()
}

#[tokio::test]
async fn one_time_code_dies_after_five_failures() {
    let h = start(RelayConfig::ephemeral(), None).await;
    let r = new_room();
    let code = JoinCode::generate().unwrap();
    let mut host = client(&h, &r.owner.id).await;
    host.send(ClientFrame::HostCode {
        rendezvous: code.rendezvous(),
        kind: CodeKind::OneTime,
        expires_at: now() + 900,
    })
    .await
    .unwrap();
    expect(&mut host, "hosted", |f| matches!(f, RelayFrame::CodeHosted { .. })).await;
    let attacker = DeviceIdentity::generate().unwrap();
    let mut j = client(&h, &attacker).await;
    for i in 0..5u8 {
        let bad = wrong_code(&code, i);
        let (ok, res) = tokio::join!(host_one_join(&mut host, &code, None), join_with(&mut j, &attacker, &bad));
        assert!(!ok);
        assert!(matches!(res, Ok(None)));
    }
    expect(&mut host, "code failed", |f| matches!(f, RelayFrame::CodeClosed { reason: CodeClosed::Failed, .. })).await;
    // Even the right code no longer works.
    assert_eq!(join_with(&mut j, &attacker, &code).await.unwrap_err(), ErrorCode::CodeUnknown);
    h.shutdown().await;
}

#[tokio::test]
async fn reusable_code_locks_after_five_failures_and_owner_is_told() {
    let h = start(RelayConfig::ephemeral(), None).await;
    let r = new_room();
    let code = JoinCode::generate().unwrap();
    let mut host = client(&h, &r.owner.id).await;
    host.send(ClientFrame::HostCode {
        rendezvous: code.rendezvous(),
        kind: CodeKind::Reusable { max_uses: None },
        expires_at: now() + 86_400,
    })
    .await
    .unwrap();
    expect(&mut host, "hosted", |f| matches!(f, RelayFrame::CodeHosted { .. })).await;
    let attacker = DeviceIdentity::generate().unwrap();
    let mut j = client(&h, &attacker).await;
    for i in 0..5u8 {
        let bad = wrong_code(&code, i);
        let (ok, _) = tokio::join!(host_one_join(&mut host, &code, None), join_with(&mut j, &attacker, &bad));
        assert!(!ok);
    }
    let RelayFrame::CodeLocked { until, .. } =
        expect(&mut host, "locked", |f| matches!(f, RelayFrame::CodeLocked { .. })).await
    else {
        unreachable!()
    };
    assert!(until >= now() + 590);
    assert_eq!(join_with(&mut j, &attacker, &code).await.unwrap_err(), ErrorCode::CodeLocked);
    h.shutdown().await;
}

#[tokio::test]
async fn abandoned_join_attempts_count_as_failures() {
    let mut cfg = RelayConfig::ephemeral();
    cfg.code_max_failures = 2;
    let h = start(cfg, None).await;
    let r = new_room();
    let code = JoinCode::generate().unwrap();
    let mut host = client(&h, &r.owner.id).await;
    host.send(ClientFrame::HostCode {
        rendezvous: code.rendezvous(),
        kind: CodeKind::OneTime,
        expires_at: now() + 900,
    })
    .await
    .unwrap();
    expect(&mut host, "hosted", |f| matches!(f, RelayFrame::CodeHosted { .. })).await;
    for _ in 0..2 {
        let mut j = client(&h, &DeviceIdentity::generate().unwrap()).await;
        j.send(ClientFrame::JoinCode { rendezvous: code.rendezvous() }).await.unwrap();
        expect(&mut j, "started", |f| matches!(f, RelayFrame::JoinStarted { .. })).await;
        drop(j);
        expect(&mut host, "ended", |f| matches!(f, RelayFrame::PakeEnded { .. })).await;
    }
    expect(&mut host, "code failed", |f| matches!(f, RelayFrame::CodeClosed { reason: CodeClosed::Failed, .. })).await;
    h.shutdown().await;
}

#[tokio::test]
async fn per_ip_join_attempts_are_rate_limited() {
    let mut cfg = RelayConfig::ephemeral();
    cfg.join_attempts_per_ip = 3;
    let h = start(cfg, None).await;
    let mut j = client(&h, &DeviceIdentity::generate().unwrap()).await;
    for i in 0..3 {
        let code = JoinCode::generate().unwrap();
        j.send(ClientFrame::JoinCode { rendezvous: code.rendezvous() }).await.unwrap();
        let f = expect(&mut j, "answer", |f| matches!(f, RelayFrame::Error { .. })).await;
        assert!(matches!(f, RelayFrame::Error { code: ErrorCode::CodeUnknown, .. }), "attempt {i}: {f:?}");
    }
    let code = JoinCode::generate().unwrap();
    j.send(ClientFrame::JoinCode { rendezvous: code.rendezvous() }).await.unwrap();
    expect(&mut j, "rate limited", |f| matches!(f, RelayFrame::Error { code: ErrorCode::RateLimited, .. })).await;
    h.shutdown().await;
}

#[tokio::test]
async fn second_host_cannot_take_a_live_code() {
    let h = start(RelayConfig::ephemeral(), None).await;
    let code = JoinCode::generate().unwrap();
    let mut a = client(&h, &DeviceIdentity::generate().unwrap()).await;
    let mut b = client(&h, &DeviceIdentity::generate().unwrap()).await;
    a.send(ClientFrame::HostCode { rendezvous: code.rendezvous(), kind: CodeKind::OneTime, expires_at: now() + 900 })
        .await
        .unwrap();
    expect(&mut a, "hosted", |f| matches!(f, RelayFrame::CodeHosted { .. })).await;
    b.send(ClientFrame::HostCode { rendezvous: code.rendezvous(), kind: CodeKind::OneTime, expires_at: now() + 900 })
        .await
        .unwrap();
    expect(&mut b, "in use", |f| matches!(f, RelayFrame::Error { code: ErrorCode::CodeInUse, .. })).await;
    h.shutdown().await;
}

#[tokio::test]
async fn bad_auth_signature_is_rejected() {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let h = start(RelayConfig::ephemeral(), None).await;
    let (mut ws, _) = tokio_tungstenite::connect_async(h.url()).await.unwrap();
    let me = DeviceIdentity::generate().unwrap();
    let hello =
        ClientFrame::Hello { protocol: polyph_proto::PROTOCOL_VERSION, device: me.device_id(), client: "test".into() };
    ws.send(Message::Binary(hello.encode().unwrap().into())).await.unwrap();
    let Some(Ok(Message::Binary(_challenge))) = ws.next().await else { panic!("no challenge") };
    // Sign the wrong nonce.
    let sig = me.sign(ctx::RELAY_AUTH, b"not the nonce");
    ws.send(Message::Binary(ClientFrame::Auth { sig }.encode().unwrap().into())).await.unwrap();
    let Some(Ok(Message::Binary(b))) = ws.next().await else { panic!("no reply") };
    assert!(matches!(RelayFrame::decode(&b).unwrap(), RelayFrame::Error { code: ErrorCode::Unauthenticated, .. }));
    h.shutdown().await;
}

#[tokio::test]
async fn lan_relay_tls_is_pinned_by_fingerprint() {
    let ss = polyph_relay::tls::self_signed(&["polyph.local".into()]).unwrap();
    let h = serve(RelayConfig::ephemeral(), ServeOptions { tls: Some(ss.config.clone()), ..Default::default() })
        .await
        .unwrap();
    assert!(h.url().starts_with("wss://"));
    let me = DeviceIdentity::generate().unwrap();
    RelayClient::connect(&h.url(), &me, Some(ss.fingerprint)).await.unwrap();
    let mut wrong = ss.fingerprint;
    wrong[0] ^= 1;
    assert!(RelayClient::connect(&h.url(), &me, Some(wrong)).await.is_err());
    h.shutdown().await;
}

#[tokio::test]
async fn health_and_metrics_endpoints() {
    let mut cfg = RelayConfig::ephemeral();
    cfg.metrics = true;
    let h = start(cfg, None).await;
    let body = http_get(h.addr, "/healthz").await;
    assert!(body.contains("\"status\":\"ok\""));
    let m = http_get(h.addr, "/metrics").await;
    assert!(m.contains("polyph_relay_connections_total"));
    h.shutdown().await;
    let h = start(RelayConfig::ephemeral(), None).await;
    assert!(http_get(h.addr, "/metrics").await.starts_with("HTTP/1.1 404"));
    h.shutdown().await;
}

async fn http_get(addr: std::net::SocketAddr, path: &str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    s.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).await.unwrap();
    out
}

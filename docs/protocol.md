# Polyph protocol

Version 1.0 (`PROTOCOL_VERSION = 0x0100`). The reference implementation is
`crates/polyph-proto`; where this document and the code disagree, the code is
a bug or this document is, and both get fixed together.

## 1. Versioning

A version is `major << 8 | minor`. A peer accepts messages whose major equals
its own and whose minor equals its own or the one before it. Every public
protocol change bumps the minor version and keeps decoding the previous one.
Unknown room message kinds decode as `Unknown` and are ignored, so new message
kinds do not need a major bump.

## 2. Encodings

- **Envelope**: fixed binary layout (section 5), the only structure the relay
  inspects besides relay frames.
- **Everything else**: CBOR (RFC 8949) via serde. Decoders enforce a size
  limit before parsing and a nesting limit of 64.
- **JSON** (browser bridge, status feed, local API): ids are lowercase hex,
  byte strings are standard base64.

## 3. Identities and signatures

Each device has:

- an Ed25519 signing key; its 32-byte public key **is** the `DeviceId`;
- an X25519 key for receiving HPKE-sealed room keys.

Keys live in the OS keychain (`keyring` crate) or, where no keychain service
exists, in a file encrypted with a key derived from a local secret (M2).
Browsers keep non-extractable WebCrypto keys in IndexedDB where the algorithm
allows; Ed25519/X25519 are generated in the WASM module and stored wrapped by
a non-extractable AES-GCM key.

All signatures are domain separated:

```
Ed25519.sign( "polyph/v1/sig/" || context || 0x00 || message )
```

Contexts: `envelope`, `genesis`, `cert`, `key-delivery`, `relay-auth`,
`room-end`, `usage-summary`, `spectator`. Verification uses `verify_strict`.

`Signed<T>` stores the CBOR body bytes, the signer and the signature. The
signature covers the stored bytes; receivers verify before decoding.

## 4. Rooms, genesis and membership

### 4.1 Genesis and room id

The owner creates a `RoomGenesis { protocol, owner: DevicePublic, owner_member,
created_at, quick, nonce }`, signs it, and derives

```
room_id = BLAKE3-derive_key("polyph v1 room id", signed_genesis.body)[0..16]
```

Because the genesis names the owner device and the id is its hash, the root of
trust for a room id cannot be replaced.

### 4.2 Certificates

`MembershipCert { room, member, device, name_commit, role, issued_at,
expires_at, capabilities }` signed with context `cert`.

- `name_commit = BLAKE3-derive_key("polyph v1 display name", salt || name)`.
  The display name and 16-byte salt travel only in the encrypted `Profile`
  message; clients check them against the commitment.
- `capabilities` marks special devices. The one defined is `agent-host`: a
  server that runs its member's agents (section 9.7). A joiner asks for
  capabilities in its `JoinRequest`; the knock shows them, and the approving
  device copies only capabilities it knows into the certificate.

A `CertChain` is the signed genesis plus certificates `c[0..n]`:

1. `1 ≤ n ≤ 8`.
2. `c[0]` is signed by the genesis owner device.
3. `c[i]` (i > 0) is signed by the device certified in `c[i-1]`, whose role
   must be Maintainer or Owner.
4. Each `c[i].role` is at most its issuer's role (the owner counts as Owner).
5. Every certificate names this room, is unexpired, was not issued more than
   five minutes in the future, and no device in the chain has been removed.
6. `c[i]` (i > 0) does not certify the device that signed it: only the
   owner's first certificate is self-issued.

The member's role is `c[n-1].role`. Clients drop envelopes from any sender
whose chain they have not verified.

Clients keep one chain per device and replace it only with a chain whose last
certificate has a later `issued_at` (or the same `issued_at` and no higher
role), names the same member id and encryption key (`kx`), and was issued
by a device whose role is at least the role held, whether it arrives in
`Hello`, `RoleChanged` or a snapshot (this device's own chain included). An
Owner's certificate is replaced only by a certificate the room owner's device
issued, and the room owner's device (named in the genesis) is certified only
by its own first certificate: a chain with another certificate for it is
refused. A role
change re-issues the certificate of every device of the member, with an
`issued_at` later than the one it replaces, so a demoted device cannot restore
its old role by presenting its previous chain.

Clients also refuse a new chain (in `Hello`, `RoleChanged`, `MemberAdded` or
a snapshot) when any issuing link `c[i]` (i < n-1) is older than the
certificate they hold for that link's device: the issuer was demoted or
re-issued since, so it cannot extend its old chain, whatever `issued_at` it
writes on the new link. A device already held keeps the chain held for it.
For the same reason a role change also re-issues, from the changer's own
chain and keeping their role, every device whose chain runs through a
device being re-issued, and a device whose own chain runs through the member
being changed cannot change that member's role. After any replacement, and
after taking a device it did not hold (in `Hello`, `RoleChanged`,
`MemberAdded` or a snapshot, whatever the order of the snapshot's members), a
client checks every chain it holds again and drops those now issued through
a superseded certificate. For the room owner's device only its genesis
self-certificate counts in that check.

Rooms saved by 0.1.2 can hold chains this version refuses. On loading one, a
client puts the room owner's device back on its genesis chain (a 0.1.2 role
change of the owner left a newer certificate from another device), and on
the first connect after that an Owner or Maintainer re-issues held devices
whose chain runs through a superseded certificate and whose last certificate
predates the supersession (0.1.2 role changes re-issued only the member's
own devices). Those re-issues give the role held, at most Member, and are
logged in the activity feed (`certificates_reissued`). This is local saved
state; nothing on the wire changes.

### 4.3 Roles

| Role | Can |
|---|---|
| Owner | everything; issue Owner certificates; end the room |
| Maintainer | approve joins, issue certificates up to Maintainer, remove members, rotate keys, change settings, approve anyone's agent after escalation |
| Member | chat, edit the board, run agents, approve agents after escalation |
| Viewer | read; messages that change state are ignored by every client |

Removal is a `MemberRemoved` message from an Owner or Maintainer. Every client
adds the member's devices to the room's revocation set (stored in room state)
and the acting host rotates the epoch (section 6.3).

## 5. Envelopes and group encryption

### 5.1 Layout

```
offset size
0      2   magic "PY"
2      2   version (u16 BE)
4      16  room id
20     32  sender device id
52     8   epoch (u64 BE)
60     8   sequence number (u64 BE)
68     24  nonce
92     4   ciphertext length (u32 BE)
96     n   ciphertext
96+n   64  Ed25519 signature
```

Header = bytes `0..92`. Maximum envelope size: 1 MiB.

### 5.2 Sealing

```
k_msg = HKDF-SHA256(salt = "polyph/v1/msg-key", ikm = epoch_key,
                    info = room_id || epoch_be, L = 32)
ct    = XChaCha20-Poly1305(k_msg, nonce = random 24 bytes, aad = header, pt)
sig   = sign("envelope", header || ct)
```

### 5.3 Opening

1. Parse with size checks; reject unknown major versions.
2. Verify the signature against `sender` (strict Ed25519).
3. Check that `sender` has a verified, unrevoked certificate chain in this room.
4. Look up the epoch key; decrypt with the header as associated data.
5. Replay check: per `(sender, epoch)` a 1024-entry sliding window over `seq`.
   Duplicates and anything older than the window are dropped. Reordering
   inside the window is accepted.

Senders use a strictly increasing `seq` per epoch, persisted across restarts.

## 6. Room keys

### 6.1 Epochs

Each room has a 32-byte key per epoch. Epoch 1 is created by the owner at
genesis. A new epoch starts when a member is removed, and after a network
split heals with diverged epochs (6.4).

### 6.2 Delivery

The acting host seals the epoch key to each member device with HPKE (RFC
9180), base mode, `DHKEM(X25519, HKDF-SHA256)`, `HKDF-SHA256`,
`ChaCha20Poly1305`:

```
info = "polyph/v1/epoch-key" || room_id || epoch_be
aad  = recipient device id
```

The result is wrapped in `Signed<EpochKeyDelivery>` (context `key-delivery`).
A receiver accepts a delivery only if the signer has a verified chain with
role Maintainer or Owner. New members receive their first delivery inside the
join channel (section 7).

### 6.3 Rotation

On removal: the acting host (if Owner or Maintainer; otherwise the
highest-ranked online Owner or Maintainer) generates epoch `e+1` and sends one
`EpochRotated { epoch, deliveries }` under epoch `e`, with a delivery for
every remaining device and none for removed devices. Senders switch to the new
epoch as soon as they hold its key. Old keys are kept for 10 minutes to read
in-flight messages, then discarded.

### 6.4 Epochs after a split

If two partitions each rotated, epoch numbers may collide with different keys.
Keys are stored per `(epoch, issuer)`. After the split heals, the acting host
rotates to `max(epoch) + 1` for everyone, and CRDT sync (section 9) re-sends
any state one side could not read.

## 7. Joining

### 7.1 Codes

8 characters from Crockford base32 (`0-9 A-Z` without I, L, O, U), shown as
`XXXX-XXXX`. Input is case-insensitive; `I`/`L` read as `1`, `O` as `0`.

```
rendezvous_id = BLAKE3-derive_key("polyph v1 rendezvous", first 4 characters)[0..16]
```

The relay only ever sees `rendezvous_id`. The full code is the PAKE password.

### 7.2 Handshake

Symmetric SPAKE2 over Ed25519 (`spake2` crate), identity
`"polyph/v1/join/" || rendezvous_id`, password = the 8 normalized characters.

```
transcript = lp(rendezvous_id) || lp(host_msg) || lp(joiner_msg) || lp(binding)
K          = SPAKE2 shared secret
HK         = HKDF-SHA256(salt = transcript, ikm = K)
confirm_h  = HMAC-SHA256(HK("polyph/v1/join/confirm/host"), transcript)
confirm_j  = HMAC-SHA256(HK("polyph/v1/join/confirm/joiner"), transcript)
k_h2j, k_j2h = HK("polyph/v1/join/channel/h2j"), HK(".../j2h")
```

`lp(x)` is a 4-byte big-endian length followed by `x`. `binding` is empty for
relays with public certificates and is the SHA-256 fingerprint of the LAN
relay's self-signed certificate in LAN mode, so a joiner verifies the LAN
relay's identity with the code alone.

The joiner's `JoinRequest { device, member, display_name, name_salt, surface,
link_device, capabilities }` travels over that channel. A joiner may name an
existing member id (a person's phone or agent host joining as another device
of theirs); it is still a knock that an Owner or Maintainer must approve.
The request carries no proof that it comes from that member, so clients
refuse to approve a knock whose member id already belongs to someone in the
room, except in two cases:

- The code was made by `phone.pair` or `host.invite` on the approving device,
  for the approver's own member id, and the knock names that member id. The
  host daemon records this locally when it makes the code; nothing on the
  wire marks such a code. Plain invites (`room.invite`, one-time or reusable)
  never qualify. Only that device can approve the knock, and its approval
  prompt says that a new device is joining as you.
- The knocking device is already one of that member's devices and its knock
  carries the same encryption key (`kx`) as the certificate held for it.

Clients also refuse a knock that names this device, the room owner's device,
a device they hold or a removed device, unless it is that same device
re-joining (same member id and same `kx`), and do not let a `MemberAdded`
chain for a held device under another member id or key replace it. A
knock that names an existing member id or device is never approved
automatically, even with "Require approval to join" off.

Both sides exchange confirmation tags and compare in constant time. The
joiner sends its tag as soon as it has the host's SPAKE2 message. The host
sends its own tag only after the joiner's has verified, so a joiner learns
nothing it could test a guess against offline without spending a counted
attempt. Then they use an ordered XChaCha20-Poly1305 channel with counter
nonces (`nonce = 0^16 || counter_be`, aad `"polyph/v1/join"`).

### 7.3 Flow over the relay

```
host   → relay  HostCode { rendezvous, kind, expires_at }
joiner → relay  JoinCode { rendezvous }
relay  → host   JoinerArrived { session }      relay → joiner  JoinStarted { session }
host  ⇄ joiner  Pake { session, data }  (SPAKE2 messages, then confirmation tags)
host   → relay  PakeResult { session, ok }
joiner → host   Pake: JoinRequest  (sealed in the channel)
host   → joiner Pake: JoinReply::Pending
  ... an Owner or Maintainer approves the knock (it may be another device) ...
host   → joiner Pake: JoinReply::Approved { chain, key delivery, room name, relay }
```

With "Require approval to join" on (the default) the host publishes a `Knock`
to the room and waits for `KnockDecided` from an Owner or Maintainer device.
The approver issues the certificate; the host forwards it.

### 7.4 Attempt limits

Enforced by the relay, and independently by the host daemon:

- Each `JoinCode` on a code is one attempt. It fails when the host reports
  `ok = false`, the joiner disconnects before the host reports a result, or
  the host has not reported a result within 2 minutes (the session is ended
  with `PakeEnded`).
- A code accepts a new `JoinCode` only while its failures plus attempts still
  in progress stay under the limit; otherwise the relay answers `RateLimited`.
- The host daemon counts its own failures per code: a malformed SPAKE2
  message, a wrong tag, or a session that ends (`PakeEnded`) before the
  joiner's tag verified. After 5 in a row it refuses the code, whatever the
  relay does.
- After 5 failures a one-time code is deleted (`CodeClosed::Failed`); a
  reusable code is locked for 10 minutes and the host is told (`CodeLocked`),
  which the daemon shows to the room owner.
- Per IP: 20 join attempts per 10 minutes across all codes; 120 connections per minute.
- One-time codes expire after 15 minutes by default. Reusable codes have the
  owner's expiry and use limit. A host revokes a code with `RevokeCode`.
- A second device cannot host a rendezvous id already in use (`CodeInUse`);
  the daemon generates a new code.

## 8. Relay protocol

WebSocket at `/ws`, binary frames, CBOR `ClientFrame` / `RelayFrame`
(`crates/polyph-proto/src/relay_wire.rs`). Maximum frame 8 MiB.

1. Client sends `Hello { protocol, device }`.
2. Relay sends `Challenge { nonce }`.
3. Client sends `Auth { sig = sign("relay-auth", nonce || device) }`.
4. Relay sends `Welcome`.

Then:

- `Subscribe { room, chain?, after? }`. With a chain that verifies for this
  room and this device the subscription may publish (`writer = true`);
  without one it is read-only (spectators). The relay replays buffered
  envelopes after cursor `after` and ends with `CaughtUp`. If `after` is older
  than the buffer, or newer than anything the relay has (it restarted), it
  sends `Gap` and the client fetches snapshots and runs CRDT sync.
- `Publish { envelope }`: the relay checks the envelope parses, that
  `sender` is the authenticated device, and that this connection is a writer
  in the room; assigns the next cursor; fans out `Deliver { cursor }` to other
  subscribers; and buffers it.
- Buffer: 10 minutes, at most 5,000 envelopes and 2 MiB per room, and 64 MiB
  across all rooms (the room being written to sheds oldest first).
- `PutSnapshot { epoch, blob }` (Member role or above): one opaque snapshot per device
  per room; the 10 newest devices' snapshots are kept. `GetSnapshots` returns
  them newest first. Retention is per plan; unlimited when self-hosted.
- `RegisterRoom { genesis }` tells the relay the owner device;
  `EndRoom { room, at, sig = sign("room-end", room || at) }` from that device
  deletes the room's buffer, snapshots and member records and sends
  `RoomEnded` to subscribers. Quick sessions end this way.
- Plan limits: when a member limit applies, the relay counts distinct
  `MemberId`s from verified chains and refuses new ones with `RoomFull`.

## 9. Room state

Shared state is a set of Yjs documents (`yrs` in Rust, `yjs` in TypeScript):
`Board`, `Threads`, `Activity`, `Memory`, `Settings`, and one per whiteboard.
Layouts are in `crates/polyph-proto/src/crdt.rs`.

- Local changes are broadcast as `Crdt { doc, update }`.
- On connect, after a `Gap`, and every 60 s while online, a daemon sends
  `CrdtSyncRequest { doc, state_vector }`; peers answer with the diff since
  that state vector as a `Crdt` message.
- Each daemon keeps the full state encrypted at rest and uploads an encrypted
  snapshot (all documents plus room metadata, sealed under the current epoch
  with the same construction as envelopes) at most every 5 minutes and when
  the last local client leaves.
- A member returning when nobody is online fetches snapshots, decrypts the
  newest one it can, merges it, then syncs with peers when they appear.
- A snapshot gets the checks a live message would: it is dropped if its
  sender has been removed, cannot write (Viewer), or sealed it under an epoch
  older than the previous one. Its list of removed devices is applied only
  if the sender may remove members (Maintainer or Owner).

### 9.1 Memory notes

The `Memory` document holds a Y.Map `notes`: note id (hex) to a JSON string
with `id`, `path` (repository-relative, normalized; a trailing `/` pins a
directory), `line` (1-based, or null for the whole file), `anchor`, `text`,
`author` (member id hex), `agent` (label or null) and `at`. The anchor keeps
the trimmed text of the pinned line and of the nearest non-blank lines above
and below it. Readers re-place a note against their own copy of the file:
the line itself must match, neighbors and distance from the old line break
ties, and a note whose line is gone is shown as possibly stale. Layout and
placement are in `crates/polyph-core/src/memory.rs`.

### 9.2 Inboxes

`Inbox { id, to, from_member, from_agent, thread, reply_to, text, at, hops }`
is a directed message to one agent, readable by every member like any room
message. Receivers accept it only if `from_member` is the sender's member and
`from_agent`, when present, runs on the sending device. Text is at most
16 KiB.

`hops` counts agent-to-agent steps since a person last spoke: 0 for a
person's message, and one more than the hops of the message that started the
sending agent's turn otherwise. The daemon running the target agent:

- wakes an idle agent for a message from another agent only while
  `hops < 4`, and at most 12 times per hour per agent;
- never wakes an agent for a person's message (that would be steering
  someone else's agent); it is delivered at the agent's next turn;
- limits each agent to 8 messages per target agent per 10 minutes.

Messages that do not wake an agent are not dropped: they are added to the
agent's next prompt. Older peers decode messages without `hops` as 0.

### 9.3 Board additions

Cards may carry `handoff` (a brief for the next agent), `plan` (the planner
run that created it) and `suggested_assignee`. The Board document also holds
`merge_queue`: a Y.Map of entry id to a JSON merge entry (branch, target,
queued commit, device that runs it, state, detail, files resolved and how,
merge commit). Only the entry's device changes it. Layouts are in
`crates/polyph-proto/src/crdt.rs` and `crates/polyph-core/src/docs.rs`.

### 9.4 Hosted mode

The Settings key `hosted` names the device holding the shared working copy.
`Mirror(MirrorMsg)` messages carry it:

- members send `IndexRequest`, `Fetch { paths }` and
  `Propose { id, agent, path, base, data, deleted }`;
- only the hosting device sends `Index { files, too_large }`, `Files`,
  `Changed { path, data, deleted, by, proposal }` and `Rejected`.

Hashes are BLAKE3 of the content. The host applies a proposal only if `base`
is the current hash (empty for a new file) and no agent on another device
holds a claim covering the path; otherwise it answers `Rejected` and the
member restores the host's version. Files over 512 KiB, gitignored files and
build directories are not mirrored. `Files` messages stay under 700 KiB.

### 9.5 Activity and whiteboards

The Activity document's `events` array holds JSON entries (`id`, `at`,
`kind`, `actor`, `agent`, `summary_key`, `args`, `checkpoint`). Each device
logs its own board changes as `card_created`, `card_moved`, `card_updated`
and `card_removed`; every 50 card events one entry's `checkpoint` holds all
cards as a JSON array. Readers sort by `at`, keeping document order for equal
times. `summary_key` names a Fluent message in `activity.ftl`.

Whiteboards are listed in the Settings map under `whiteboard:<id>` (JSON with
`id`, `name`, `created_by`, `created_at`); a device that sees a listed board
it lacks sends a `CrdtSyncRequest` for `whiteboard/<id>`.

### 9.6 Terminals

`Terminal(TerminalMsg)` messages: the owner's device sends `Opened`,
`Output`, `Resize`, `Control` and `Closed { exit }`; members send
`RequestInput { member }` and, while they hold the keyboard, `Input`.
Receivers accept `Input` and `RequestInput` only for the sender's own member,
`Output`, `Resize` and `Closed` only from the owner's device, and `Control`
from the owner's device or (to give it back) from the current holder. A
request becomes an approval of kind `TerminalInput` that only the terminal's
owner may answer.

### 9.7 Agent hosts

`Host(HostMsg)` messages run agents on a member's agent host (a device whose
certificate has the `agent-host` capability):

- `Start { req, host, kind, preset, task, prompt }`, `Prompt { host, agent,
  text }` and `Stop { host, agent }`, sent by any device of the member;
- `Started { req, agent }` and `Failed { req, error }`, sent by the host.

A host acts on a request only when it is the named `host`, its own
certificate has the capability, and the sender's verified member is its own.
A requester accepts a reply only from the device it asked, when that device
is a host of the same member and (for `Started`) the agent's device is the
sender. The host starts the agent in its configured working directory; the
agent then works like any other (transcripts, claims, approvals).

### 9.8 Browser-only members

A phone or browser can be a member with no daemon: the room engine runs in
WebAssembly (`crates/polyph-phone`) and talks to the relay directly, with
the same messages as any device. Its `Presence.surface` is `phone`, so it is
never a candidate for acting host (section 10.1). It sends `Host` requests
to its member's agent hosts but runs no agents itself.

## 10. Acting host

One online device per room is the **acting host**. It arbitrates claims,
rotates keys (if its role allows), republishes spectator views, and
announces itself.

### 10.1 Candidates and ranking

Candidates are online daemon devices, agent hosts included (not browser or
phone clients), with a
verified chain and role Member or higher. "Online" means a `Hello`,
`Presence` or `ActingHost` heartbeat was received from it in the last 30 s;
every daemon sends a presence heartbeat every 10 s.

Rank, highest first:

1. role (Owner > Maintainer > Member);
2. earliest join time: `issued_at` of the last certificate in its chain;
3. lowest `DeviceId` (bytewise), as a deterministic tiebreak.

### 10.2 Election

Every daemon computes the top-ranked candidate from its own view.

- If it is itself the top candidate and the current acting host is offline
  (no heartbeat for 30 s) or ranks lower, it takes over: `term = highest term
  seen + 1`, and publishes `ActingHost { term, device, member, role,
  joined_at }`, then repeats it every 10 s.
- A daemon accepts an `ActingHost` announcement if the announcer is a valid
  candidate and its `(term, rank)` is greater than the current acting host's.
  When two announcements share a term, the higher rank wins.
- An acting host that sees a higher-ranked candidate's announcement steps
  down immediately and sends its claim table (`Claim::Table`).

### 10.3 Claim arbitration

- Agents' daemons send `ClaimMsg::Request`; only the acting host answers with
  `Decision`. A request with no answer in 5 s is retried; after 3 tries the
  daemon treats the room as having no arbiter and grants **local provisional
  claims**, marked as such.
- The acting host broadcasts the full `Table` on election and every 30 s.
  Every daemon caches it.
- A new acting host starts from the union of cached tables. Leases continue;
  expired claims are dropped.

### 10.4 Network splits

During a split each side elects its own acting host and grants claims
independently. When the split heals, both acting hosts see each other's
announcements; the lower one steps down and sends its table. The surviving
host merges:

- Two claims conflict if they name the same file, if one is a directory that
  contains the other's file, or if they name the same `path#symbol` (a
  whole-file claim conflicts with any symbol claim in that file).
- For each conflicting pair, the claim with the **earlier `granted_at`** wins
  (ties: lower claim id). The loser is removed and its owner receives
  `ClaimMsg::Revoked { claim, winner }`. The losing agent is told at its next
  turn boundary, and the overlap goes onto the board as a warning so a human
  can decide whether work done during the split needs reconciling.
- Provisional local claims are reconciled the same way.

## 11. Spectators

A spectator link is `https://{DOMAIN}/watch/<room id>#<link id>.<view key>`.
The fragment never reaches a server. The view key is a random 32-byte key; the
acting host republishes a filtered view (default: board, activity, follow
mode; chat and terminals off) as envelopes in the same room, sealed with
`EpochKey(view key)` under the reserved epoch `2^63 + link number`. Spectators
subscribe read-only and can decrypt only those envelopes. Revoking a link
stops the republishing. See `DECISIONS.md` for why links do not carry the room
key.

## 12. Status feed

`polyph status --watch --json` and the D-Bus service `dev.polyph.Status1`
emit `StatusEvent` (`crates/polyph-proto/src/status.rs`), versioned by
`v`. See `docs/status-feed.md`.

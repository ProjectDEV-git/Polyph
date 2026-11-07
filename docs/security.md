# Security

This page describes what Polyph protects, from whom, and what it does not.
The wire details are in `docs/protocol.md`.

## Summary

- Code, chat, board, whiteboards, threads, memory notes, transcripts, diffs
  and terminal output are end-to-end encrypted between room members' devices.
- The relay (hosted, self-hosted or embedded for LAN) forwards ciphertext. It
  cannot read content, forge messages, or add members.
- Model credentials never leave members' own machines. Polyph never proxies
  model calls.
- No telemetry by default. Crash reports are opt-in.

## What the relay can see

The relay sees only this, and the hosted service's privacy page says the same:

| Data | Why |
|---|---|
| Room ids, device public keys, connection times and IP addresses | Routing and rate limiting |
| Envelope headers: room, sender device, epoch, sequence number, sizes | Routing, replay buffers |
| Which devices are in which rooms; member ids and roles from certificate chains (no names) | Refusing writes from non-members; plan member limits |
| The owner device of a registered room | Authorizing the room to be ended |
| Encrypted snapshot blobs and their sizes | Catch-up when nobody is online |
| Rendezvous id of a join code (a hash of its first half) and join attempt counts | Matching joiners to hosts; lockouts |
| **Opt-in** signed usage summaries: token counts, cost estimates, task counts | Team dashboard (accounts only, off by default) |
| **Web push** subscriptions; push payloads say only "open Polyph" | Phone notifications (accounts only) |
| Account email or GitHub id, plan and billing status | Accounts (optional; incognito use has none) |
| Device public keys linked to an account (each link signed by the device) | Applying a plan to the rooms those devices own |

## Threat model

### A malicious or compromised relay

Can:

- drop, delay, reorder or replay envelopes;
- learn who talks to whom, when, and how much (metadata);
- refuse service, or split members into partitions;
- see the rendezvous id of a join code and therefore its first four characters;
- keep ciphertext forever, regardless of stated retention.

Cannot:

- read any content: envelopes are XChaCha20-Poly1305 under epoch keys that
  only member devices hold, delivered with HPKE to each device's X25519 key;
- forge content: every envelope is signed by its sender's Ed25519 device key
  and clients drop senders without a verified certificate chain;
- replay content undetected: clients keep a per-sender sliding replay window;
- add a member: certificates chain back to the owner key that the room id
  itself is derived from, and joining needs the full code (PAKE) plus approval;
- complete a join itself: the relay knows 20 of the code's 40 bits; the PAKE
  needs all of them and confirms both sides in constant time.

Effects of what it can do: dropped or delayed messages are recovered by CRDT
sync when connectivity returns; a split causes claim conflicts that are
reconciled when it heals (earlier claim wins, loser notified, overlap shown on
the board); reordering within 1,024 messages is harmless.

### A removed member

Keeps everything they already received. Cannot read anything sent after the
rotation that follows their removal: the new epoch key is not delivered to
their devices. Their certificate is in every client's revocation set, so their
messages are dropped even under old epochs.

### A spectator

Holds a view key for one link, not the room key. Sees only what the owner's
filter republishes for that link. Anything a spectator sends is dropped by the
relay (no certificate) and by clients. Revoking the link stops new content.

### Someone guessing a join code

Online guessing only: each attempt is a full PAKE with the host, and the
host sends its confirmation tag only after the joiner's tag has verified, so
a wrong guess teaches the guesser nothing it can test offline. With the
first half known (worst case: the relay is the attacker), 20 bits remain.

- The relay counts failures: five kill a one-time code or lock a reusable
  one for 10 minutes and notify the owner. A join attempt that is abandoned
  or stalls for 2 minutes counts as a failure, and a code never has more
  attempts in progress than it has failures left. Per-IP limits slow
  spraying across codes.
- The host daemon counts its own: a wrong tag, a malformed message, or a
  session that ends before the joiner's tag verified. After five in a row it
  stops accepting the code. This is the limit that holds when the relay
  operator is the attacker.

Probability of a correct guess: about 5 in 2^20 (≈ 0.0005%) per one-time
code, and the same between successful joins on a reusable code. Hosts on
Polyph 0.1.2 and earlier send their tag before checking the joiner's, so
against a relay operator guessing through such a host this bound does not
hold. Approval to join is on by default, so a correct guess still only
produces a knock.

### Someone claiming to be an existing member

A join request names a member id, and nothing in it proves the joiner is
that person. Clients refuse to approve a knock that names a member already in
the room, with two exceptions: a code that `phone.pair` or `host.invite` made
on the approving device for its own person (recorded on that device only;
the approval says a new device is joining as you), and a device already in
the room re-joining with the same keys its certificate names. Such knocks are
never approved automatically. Plain invite codes never let a joiner take an
existing member id. A Member, who cannot approve joins, is told it cannot
pair a phone or add an agent host. A knock that names a device already in
the room (or removed from it, or the owner's device) is refused unless it is
that device re-joining with the same keys, so a joiner cannot take over a
device id. A request does not prove that the joiner holds the device key it
names, so a client that never saw that device can still accept a
certificate an older client issued for it.

### A demoted Maintainer

A role change re-issues a certificate for every device of the member, dated
after the one it replaces; clients keep the newest certificate per device.
A chain whose issuing link is older than the certificate a client holds for
that issuer is refused, and no device may certify itself, so a demoted
Maintainer cannot restore its role or certify a new device from its old
chain. A new certificate replaces the one held only when it keeps the
device's member id and encryption key and its issuer ranks at least as high
as the role held; only the room owner's device re-issues an Owner, so a
Maintainer cannot demote the Owner. After a replacement, clients drop chains
issued through the superseded certificate. A client that has not yet seen
the demotion, directly or in a snapshot, still accepts the old chain until
it does.

### A network attacker between a member and the relay

TLS to the relay (Caddy with a public certificate, or a self-signed
certificate in LAN mode whose fingerprint is bound into the join PAKE). Even
without TLS, content stays end-to-end encrypted; TLS protects metadata.

### Other local users and processes

- The daemon's local API listens on localhost only and requires a random token
  stored in the user's config directory with mode 0600 (owner-only ACL on
  Windows).
- The browser client never sees that token: `polyph web` opens a one-time
  link (two minutes, single use) that the page trades for a session token
  held in the daemon's memory. Browser-facing routes answer only to loopback
  host names (against DNS rebinding), the API refuses pages from other
  origins, and the client's pages carry a strict Content Security Policy.
- Device secret keys are stored in the OS keychain, or in a file encrypted
  with a key held in the keychain or, where none exists, derived from a local
  secret with owner-only permissions.
- Logs never contain message content, code, keys or tokens.

### Agents

Agents are untrusted code. Every agent and everything it spawns runs in a
sandbox (bubblewrap or Landlock + seccomp on Linux, Seatbelt on macOS, a
restricted token and job object on Windows, or a rootless container when
enabled) that limits writes to its worktree and temp directories. Outbound
network goes through the daemon's allowlisting proxy; anything else becomes an
approval request. `polyph sandbox test` reports which protections work on a
given machine. See `docs/sandbox.md` (M3).

### A lost phone

The phone app keeps its device key and room state in the browser's IndexedDB,
sealed with an AES-GCM key the browser generated as non-extractable. Copies
of the stored bytes (a backup, a synced profile) cannot be opened elsewhere;
someone holding the unlocked phone can use the app. Remove the phone's
membership from another device (`polyph room remove`), which rotates the
room key. The pairing link carries a one-time code, the person's member id
and the relay in the URL fragment, which browsers never send to the server
hosting the page; the page clears it from the address bar at once.

### An agent host

An agent host holds its member's agents and a checkout of the code, like
that member's laptop. It accepts work only from devices of its own member
(checked against their certificates), and its certificate capability is
visible to the room. Treat it as one of your own devices: keep its OS
patched and its disk encrypted, and remove its membership if it is lost.

### Plugins

Plugins are code a person chose to install on their own device; a
repository cannot install one. Their tool servers run inside each agent's
sandbox and their hooks in a fresh sandbox of their own, reaching only the
network hosts the manifest declares. Within those limits a plugin can do
what an agent can (read the worktree, call Polyph tools through the agent),
so install only plugins you trust. They are not signed yet (`STUBS.md`).

### Secrets in shared content

Before diffs, file mirrors, tool output, terminal streams or chat leave a
member's daemon, a redactor (gitleaks default rules plus `.env`-style
detection) replaces matches with `[redacted: <rule>]`. Paths in
`.polyphignore` (default: `.env*`, `*.pem`, `*.key`, `id_*`, `.aws/`,
`.ssh/`) are never shared. Redaction is a safety net; it cannot recognize
every secret.

## Not protected

- Metadata (who, when, how much) from the relay operator.
- Members of a room: they can read everything in it, by design. Choose members
  and roles accordingly.
- A compromised member device: it holds that member's keys.
- Denial of service by the relay or the network.

## Reporting

Report vulnerabilities privately to the maintainers (address to be set before
launch; listed in `STUBS.md`). Do not open public issues for security bugs.

## Cryptography used

| Purpose | Primitive | Crate |
|---|---|---|
| Device signatures | Ed25519 (strict verification) | `ed25519-dalek` 2 |
| Key agreement for key delivery | HPKE RFC 9180, DHKEM(X25519, HKDF-SHA256), ChaCha20-Poly1305 | `hpke` 0.13 |
| Message encryption | XChaCha20-Poly1305 | `chacha20poly1305` 0.10 |
| Key derivation | HKDF-SHA256, BLAKE3 derive_key | `hkdf`, `blake3` |
| Join | SPAKE2 (Ed25519 group), HMAC-SHA256 key confirmation | `spake2` 0.4 |
| Transport | TLS 1.2/1.3 | `rustls` |

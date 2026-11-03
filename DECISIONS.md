# Decisions

Choices made during the build where the brief left room, or where the brief
could not be followed exactly. Newest at the bottom of each section.

## Toolchain and dependencies

- **Rust 1.97 stable, edition 2024, workspace resolver 3.** Minimum supported
  Rust is 1.94 because `sqlx 0.9` (relay storage) requires it.
- **Crypto crates pinned to one generation:** `ed25519-dalek 2.2`,
  `x25519-dalek 2`, `curve25519-dalek 4`, `chacha20poly1305 0.10`,
  `hpke 0.13`, `spake2 0.4`, `hkdf 0.12`, `sha2 0.10`. The newest RustCrypto
  generation (dalek 3, chacha20poly1305 0.11, spake2 0.5-pre) is partly
  pre-release and does not yet interoperate cleanly; mixing generations
  multiplies `rand_core` versions. `hpke 0.13` needs `rand_core 0.9`, which we
  depend on under the alias `rand_core_09` only for HPKE's RNG.
- **`yrs 0.28`** for CRDTs. Wire compatible with `yjs 13.6`.
- **CBOR (`ciborium`) for every structured payload**, decoded with a size limit
  and a recursion limit of 64. The envelope itself is a fixed binary layout so
  the relay parses it without a general-purpose decoder.

## Protocol and security

- **Device id = Ed25519 public key (32 bytes).** Simpler than a hash and lets
  any party verify an envelope signature without a lookup.
- **Room id = first 16 bytes of `BLAKE3-derive_key("polyph v1 room id", signed genesis body)`.**
  The genesis names the owner device, so nobody can forge an alternative root
  of trust for an existing room id.
- **`Signed<T>` carries the exact signed bytes.** Verification checks the
  signature over the stored CBOR body before decoding it, so re-encoding
  differences can never break or forge a signature.
- **All signatures are domain separated** (`polyph/v1/sig/<context>\0`).
- **Display names are not in membership certificates.** Certificates carry a
  salted BLAKE3 commitment to the name; the name and salt travel in the
  encrypted `Profile` message and clients check them against the commitment.
  This lets the hosted relay verify certificate chains (to enforce plan size
  limits by member count and to refuse writes from non-members) without
  learning anyone's name. The approver still endorses the name, as the brief
  intends.
- **Spectator links do not carry the room epoch key.** The brief says a
  spectator link carries "the epoch key without a membership certificate", but
  with the full epoch key a spectator could decrypt chat and terminals, which
  the owner's filter is supposed to hide. Instead each spectator link has its
  own view key (in the URL fragment, never sent to a server); the acting host
  republishes only the filtered view (board, activity, follow mode by default)
  under that key to a separate spectator stream. Revoking a link stops the
  republishing. Same read-only property, and the filter is enforced
  cryptographically.
- **Key rotation requires Owner or Maintainer.** Any member with role Member or
  higher can be acting host for claim arbitration; if the elected acting host
  is a Member, rotation waits until an Owner or Maintainer is online (removing
  someone already requires one of them). See `docs/protocol.md`.
- **Join code entropy.** The relay learns the first half (20 bits) through the
  rendezvous hash, so the PAKE protects 20 unknown bits. With the 5-attempt
  limit an online guesser succeeds with probability about 5 in a million per
  code, the same order as magic-wormhole's default. Codes are short-lived and
  join approval is on by default, which adds a human check.
- **Rendezvous collisions.** 20-bit rendezvous ids can collide across rooms on a
  busy relay. The relay refuses a second host on a live rendezvous id with
  `CodeInUse`, and the daemon simply generates a new code.
- **Protocol versions are `major << 8 | minor`.** A peer accepts its own minor
  and the one before it.

## Daemon

- **One storage key per Polyph home** in the OS keychain encrypts the device
  identity and every room file (XChaCha20-Poly1305). Where no keychain is
  reachable, the key is a 0600 file, like an SSH key; deriving it from a
  passphrase would make a background daemon prompt, which it cannot. The
  `keyring` crate uses the pure-Rust Secret Service backend so Linux builds
  need no libdbus.
- **Local API is JSON-RPC 2.0** over a localhost WebSocket (and single calls
  over `POST /rpc`), authenticated by a bearer token, query parameter
  accepted for webviews that cannot set headers. Events stream on the same
  socket.
- **The remover rotates keys.** `docs/protocol.md` 6.3 lets "the acting host"
  rotate; in practice whoever removes a member (an Owner or Maintainer by
  definition) rotates immediately, which is simpler and closes the window.
- **Room key delivery to a new member** reuses `EpochRotated` with the
  current epoch and one delivery, instead of a separate message type.
- **Invites wait for the relay** to confirm the code (`CodeHosted`) before
  returning, and wait up to 10 s for a just-opened room to connect, so a code
  never reaches a person before it works.
- **A device that reconnects forgets it was acting host** and resends its
  claim table, so splits reconcile and stale hosts never keep an old term.
- **Chat lives in the Threads CRDT** (`chat` array), not as transient
  messages, so history survives with the room.
- **Hue choice happens on the joiner** two seconds after connecting, once
  other members' hues are known.
- **CLI help is localized at runtime** by walking clap's command tree and
  replacing `about` texts from `help-*` Fluent keys.

## Agents and sandbox

- **ACP types are our own lean serde structs**, checked against the official
  `agent-client-protocol-schema` crate in both directions in
  `crates/polyph-agents/tests/acp_compat.rs`. The official types are
  builder-based and `#[non_exhaustive]`, which makes them heavy to construct
  on both the client and agent side.
- **Notifications are processed inline by the JSON-RPC reader** so every
  `session/update` sent before a prompt's response is handled before the
  response resolves; `agent.prompt` also waits for the daemon to publish the
  turn's events before returning.
- **Linux sandbox follows Claude Code's approach**: bubblewrap with a network
  namespace and an in-namespace forwarder to the daemon's proxy over a Unix
  socket. Without bubblewrap, Landlock limits writes and TCP connects to the
  proxy port. The helper is the `polyph` binary itself (`polyph __sandbox`),
  so no extra executable ships.
- **Agents get a private TMPDIR**; shared `/tmp` is not writable, because a
  writable `/tmp` would let one agent write into another's worktree when
  worktrees live there.
- **`NO_PROXY` is empty** in sandboxes so local services also pass the
  allowlist.
- **One proxy per agent**, so network approvals are attributed to the agent
  that asked.
- **Network approvals apply in every preset**, including `auto`: the
  allowlist is a sandbox boundary, not an autonomy setting.
- **No default prices.** Model prices change and must not be invented; the
  table is empty until configured, and costs show as unestimated.
- **Usage records and budgets live in shared documents** (Activity `usage`,
  Settings `budgets`) so every member computes the same ledger.
- **Agents found next to `polyph`** (`resolve_command`) so installers can ship
  `polyph-localagent` beside the main binary without touching `PATH`.
- **The local agent writes and runs commands only through ACP** client
  requests, so it gets claims, approvals and the sandbox like any other agent.
  It retries once after invalid tool JSON, switches to a text protocol when
  the endpoint rejects native tools, and summarizes old turns at 75% of the
  context window.

## Process

- **Design skills.** Impeccable, Emil Kowalski's design-engineering skill,
  design-taste-frontend, the GSAP skills and Anthropic's frontend-design were
  installed into `.claude/skills/`. The Apple design skill is installed with a
  Claude Code `/plugin marketplace add` slash command, which cannot be run from
  a headless build session; its guidance (macOS menus, shortcuts, settings
  window conventions, HIG clarity and deference) is applied from the brief and
  recorded in `DESIGN.md`. Listed in `STUBS.md` so a later interactive session
  installs it.

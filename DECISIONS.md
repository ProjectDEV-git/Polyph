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

## Coordination

- **The MCP server reaches the daemon through the agent's own proxy.** Plain
  HTTP requests to the reserved host `polyph.internal` are answered by the
  daemon instead of being forwarded. Every sandbox mechanism already lets the
  agent reach its proxy (bubblewrap through the forwarder, Landlock by TCP
  port, Seatbelt, Windows), so no new socket or sandbox rule is needed, and
  because each agent has its own proxy the daemon knows which agent called. A
  per-agent token is checked as well, since loopback ports are reachable by
  other local processes.
- **MCP is hand-rolled, not `rmcp`.** The server is a stdio relay with four
  methods (`initialize`, `ping`, `tools/list`, `tools/call`) and static tool
  definitions; like the ACP types, our own lean JSON handling is smaller and
  does not churn with the SDK. It speaks protocol revisions 2025-06-18,
  2025-03-26 and 2024-11-05.
- **People's messages never wake someone else's agent.** Waking an agent with
  a person's words is steering, which needs the agent's owner; those messages
  are added to the agent's next prompt instead. Messages from agents wake an
  idle agent, within the loop guard (`docs/protocol.md` 9.2).
- **Inbox `hops`** was added to the existing `Inbox` message with a default
  of 0, so older peers stay compatible.
- **Memory notes re-anchor by line text** plus the nearest non-blank lines
  around it, not by git blame: it works on uncommitted files and in every
  member's own copy, and needs no repository access.

- **Git through the `git` command**, not libgit2: it behaves exactly like the
  person's own git (config, hooks off with `--no-verify`, worktrees), and
  `merge-tree --write-tree` (git 2.38+) computes merges without a working
  copy, so the queue never touches anyone's checkout.
- **Agents never commit.** They write files; the daemon commits on the task
  branch with an agent identity (`<label>.<id>@agents.polyph.invalid`). The
  sandbox therefore never needs write access to the shared `.git`.
- **Task worktrees live in `<repo>/.polyph/worktrees/`**, added to
  `.git/info/exclude`. Not under the Polyph home: the sandbox hides that
  directory, and a writable worktree inside it would un-hide it.
- **Finishing a task releases its claims.** The work is committed; overlap
  from then on is the merge queue's to settle, and holding claims through
  review would block everyone else for no benefit.
- **Merge order of conflict handling:** line-level diff3, then a symbol-level
  merge for source files (each function merged on its own; additions on both
  sides kept, the already-merged task first), then keep-both for prose, and
  only then a resolver agent given both tasks' intent. Resolutions are
  recorded per file on the queue entry.
- **The target branch's `polyph.toml` configures merging** (reviewer,
  resolver, approval, checks): a task branch cannot change how it is merged.
- **Checks run on the merged result** in a detached worktree inside the
  sandbox, with allowlisted hosts only and no approvals asked.
- **The queue runs where the work is**: on the device that holds the task's
  worktree, one merge at a time per repository; the queue itself is shared in
  the Board document so everyone sees it.
- **The reviewer answers with a `VERDICT:` line**, parsed from its last
  message; no verdict counts as "changes requested".
- **The planner works on a scratch worktree** and its cards start as drafts
  until a person accepts the plan.
- **Layer-3 watcher attribution:** a change counts as an agent's only if the
  agent was mid-turn when it happened and it did not come through the file
  tools; otherwise it is a person's and is left alone.
- **Hosted mode judges changes per device**: a member's change applies if the
  file is free or held by an agent on that member's device, and only if it
  was made on the host's current version. Agents finish turns in
  milliseconds, so attributing by "agent mid-turn" alone rejected their own
  writes.
- **Secrets are redacted with gitleaks-style rules** (high-signal token shapes
  plus an entropy-gated generic rule) in the daemon before encryption.
  Test fixtures are assembled from pieces so secret scanners do not flag the
  source.
- **`finish_task` goes through a daemon worker** over a channel: finishing can
  start reviewer agents, whose MCP handlers lead back to `finish_task`, and
  that cycle cannot be expressed as one `Send` future.

## Board, terminals and integrations

- **The room engine logs board changes itself**: every local change to the
  Board document is diffed into card events in the Activity document, so
  agents' tools, the merge queue, the planner and people are all covered
  without logging at each call site. Agent tools pass their label.
- **Board replay uses checkpoints**: every 50 card events an entry carries
  the whole board (cards as JSON), and a replay folds later events onto the
  latest checkpoint before the requested time.
- **Activity entries keep document order for equal times** (stable sort), so
  every member sees the same order.
- **The room engine gets the daemon's clock** for activity timestamps; it
  still performs no I/O.
- **Whiteboards are indexed in Settings** (`whiteboard:<id>`), and the engine
  requests listed boards it does not have, so late joiners get them.
- **Agents draw diagrams, not raw Excalidraw**: `draw_diagram` takes boxes
  and arrows and Polyph lays them out; drawing again with the same id
  replaces the diagram.
- **Terminal input is an approval only the terminal's owner can answer**; it
  never escalates, so a requester can never approve their own access. The
  keyboard holder can give it back; the owner can take it back.
- **Notifications are derived per device** from daemon events and rendered in
  the person's language; webhooks carry the same text and metadata, never
  chat, code or file contents.
- **Integration tokens are sealed with the storage key** on the device that
  uses them and never enter a room.
- **Pull requests replace the local merge** with `[merge] via = "github"`;
  the PR's state is then the truth for the card.
- **The D-Bus status service is read-only**: the session bus is reachable by
  every process of the user, including sandboxed agents.
- **`reqwest` 0.13 with rustls (ring)**, the same as the local agent, for all
  outgoing HTTPS.

## Terminal UI

- **A separate crate (`polyph-tui`)** with a small `Backend` trait: the CLI
  passes a WebSocket connection, tests pass the daemon in-process, so tests
  drive the real state, keys and drawing.
- **State refreshes by parts**: events name what changed (chat, board,
  agents...) and the loop refetches those parts, coalesced every 100 ms. The
  API stays the single source of truth; the TUI keeps no shadow model.
- **Calls run as their own tasks**, so prompting an agent (which waits for
  the turn) never freezes the screen; terminal keystrokes go through one
  ordered queue instead, merged into one write when they pile up.
- **Colors come from DESIGN.md's OKLCH tokens** via the same conversion as
  voices: true color with `COLORTERM=truecolor|24bit`, the nearest 256-color
  index otherwise, none with `NO_COLOR` or `TERM=dumb` (reverse video marks
  the selection).
- **Shared terminals are drawn with `vt100`**, starting from the scrollback
  the daemon keeps, so full-screen programs show correctly in previews.
- **Ctrl-] detaches** (as telnet and many consoles do); crossterm reports it
  as Ctrl-5, so both are accepted.
- **Chat keeps document order for equal times**, like activity, so messages
  sent in the same second appear in the order every member sees.
- **Approvals in the activity feed credit the person who answered**, naming
  the agent in the text.

## Desktop app, browser client and status pill

- **One React app for three surfaces.** `packages/ui` holds the whole app
  (`PolyphApp`), with `mode="pill"` for the status pill; the Tauri shell and
  the Next.js client only provide the connection and a `Platform` (notify,
  open links, toggle the pill). Brief in `docs/app.md`.
- **The browser client talks to the person's own daemon**, which serves the
  static export of `apps/web` (`POLYPH_WEB_DIR`, else next to the binary).
  A browser-only room member (the wasm engine over the relay) waits for M8,
  with the phone PWA that needs it.
- **Browsers sign in with one-time links**: `polyph web` asks for a code
  (`web.link`, API token only), the page trades it at `/web/session` for an
  in-memory session token and drops it from the address bar, so the API
  token never appears in a URL or history. Codes last two minutes and work
  once; sessions end when the daemon stops.
- **Browser-facing routes check `Host` (loopback names only) and `Origin`**
  (the daemon's own pages and the Tauri webview), against DNS rebinding and
  other sites' pages. Pages carry a strict CSP; Excalidraw's CDN fallback for
  its fonts is blocked by it, and its fonts are copied into each app.
- **State is fetched by parts and refreshed on events**, exactly like the TUI
  (`partsFor` mirrors `parts_for`), with transcript lines and terminal bytes
  applied straight from events. Replies that are not the expected shape
  become empty lists rather than errors deep in a view.
- **`voice.ts` is checked against Rust**: `polyph-core` writes
  `packages/ui/test/voice-golden.json` (with `POLYPH_UPDATE_GOLDEN=1`) and
  both test suites compare against it.
- **Locales are bundled at build time** into `locales.gen.ts` (generated,
  not committed) so Vite, Next.js and Vitest read the same Fluent files
  without loaders.
- **Fonts come from npm (`@fontsource`, OFL)** and are bundled into each app;
  nothing loads from a font CDN.
- **Dark is the default theme**; "match the system" is a choice in Settings.
- **The "Needs you" column appears only while something waits**; otherwise
  the title row's one-line finding says nothing needs you.
- **The desktop shell finds or starts the daemon itself** (the same
  `daemon.json` and `api.token` as the CLI), keeps Polyph in the tray when
  the window closes, and leaves the daemon running on Quit.
- **Test agents' scripts live outside `/tmp`** (`CARGO_TARGET_TMPDIR`):
  under bubblewrap agents get a private, empty `/tmp`, so scripts kept there
  were invisible. The merge tests only passed on machines without bubblewrap.
- **The keychain is read from a plain thread**: secret-service backends block
  on their own runtime, which panicked inside the daemon's async runtime.

## Phone app and VS Code extension

- **The phone engine is a state machine with no I/O** (`polyph-phone`),
  like `polyph-core`: the page owns the WebSocket and the timers and passes
  bytes in. The same code runs natively in `tests/m8.rs` against a real
  relay and daemon, so the protocol is tested without a browser.
- **It answers the daemon's API** (method names, parameters, reply shapes)
  and `PhoneClient` implements the app's transport over it, so the phone
  runs the same React app as the desktop and browser client; views a phone
  cannot serve are hidden through `Platform.views`.
- **A phone joins as its owner's member.** The pairing link carries the
  person's member id, so the phone is another device of theirs and can
  answer their agents' approvals at once (no escalation wait). It still
  knocks and must be approved: a member id is visible to room members and
  is not a credential; the one-time code and the approval are.
- **The link keeps everything in the fragment** (`#join=…&member=…&relay=…`),
  which browsers never send to the server hosting the page, and the page
  removes it from the address bar at once (keeping Next.js's history state,
  or the router reloads the page).
- **Phone state is sealed in IndexedDB** with an AES-GCM key generated as
  non-extractable, saved at once after joining and within a quarter second
  of later changes (a page can be killed at any time).
- **The engine loads at run time** from `/phone/engine.js` and `.wasm`
  (built by `scripts/build-phone-wasm.mjs`, not committed), so the web
  build and type-check do not need the wasm toolchain's output in source.
- **The VS Code webview talks to the daemon through the extension**
  (postMessage), not its own WebSocket: the API token stays in the
  extension host and the daemon's origin checks stay strict.
- **`@polyph/ui/core`** is the React-free part of the UI package (client,
  strings, approval sentences) for the extension host.
- **Manifest strings for VS Code** come from `locales/*/vscode.ftl` and are
  written to `package.nls*.json` at build time, keeping one source of text.

## Agent hosts and extensibility (M9)

- **"Cloud agents" are agents on the person's own machines** (agent hosts).
  The brief's open core keeps Polyph-run infrastructure proprietary, and
  PRODUCT.md asks for agents on members' own servers, so the open engine
  builds that; a hosted offering can sit on the same messages later.
- **A host is another device of the same member**, with the `agent-host`
  capability in its certificate (the field existed since M1). It needs no new
  trust model: certificates already say who a device belongs to, and the
  approver sees the capability in the knock.
- **Hosts obey only their own member**, checked against the sender's
  verified certificate, and requesters accept replies only from the host
  they asked. Prompts travel in the room like inbox messages, so members can
  read them, as they can read every agent's transcript.
- **Only known capabilities are granted**: an unknown capability in a knock
  is dropped rather than written into a certificate.
- **Skills are files, read through MCP tools**: every agent kind gets them
  the same way, with no agent-specific config, and project skills travel
  with the repository through git.
- **Plugins are per person and per device**, never installed by a
  repository, so cloning a repository cannot run code on a teammate's
  machine. Their tool servers run inside each agent's sandbox (started by
  the agent from a copy, since the Polyph home is hidden from agents) and
  their hooks in a fresh sandbox of their own; network is limited to hosts
  the manifest declares, enforced by the same allowlisting proxy as agents.
- **Hook output is data, not commands**: JSON lines with a fixed set of
  actions (chat posts for now), labeled `plugin/<name>`, and hooks never run
  on plugin posts, so plugins cannot loop on each other.

## Website, accounts and billing (M10)

- **Accounts are about who pays, never about content.** An account links
  device public keys; the hosted relay maps a room's owner device to that
  account's plan. Room ids, names and contents never reach the account
  service.
- **Devices link by signature.** The service issues a one-time challenge and
  the device signs `nonce\naccount` with its identity key under a new
  `account-link` context, so a session token alone cannot claim someone
  else's device, and no other signature can be replayed as a link.
- **Sign-in without passwords:** an 8-digit code by email (10 minutes, five
  tries, five codes per address per hour; stored hashed) or GitHub's device
  flow, which works from a terminal as well as the website. The GitHub token
  is used once to read the user id and dropped.
- **Plans live in config, prices undecided.** The defaults hold only what the
  brief fixes (Free: rooms 7 days, one spectator link; Pro: one year); Team
  keeps at least what Pro keeps (inferred). Display prices and Stripe price
  ids are optional config; without them checkout is refused and every surface
  says "Pricing coming soon".
- **Stripe, with the owner in subscription metadata** (`account:<id>` or
  `team:<id>` and the plan), so webhook events can be applied in any order and
  replays are ignored by event id. The best active plan across a person's own
  subscription and their teams' wins.
- **Mail through a Resend-compatible HTTP endpoint**, configured, rather than
  SMTP: one small JSON call, no mail library. `dev = true` logs codes instead
  and cannot be combined with a real endpoint.
- **Signing out keeps the device linked**, so rooms it owns keep their plan;
  unlinking is explicit on the account page.
- **The daemon talks to the service**, not the CLI: the device key and the
  sealed session live there, and the apps can reuse the same methods later.
- **One static export for site, client and phone app.** English at the root,
  other languages under a prefix, every word from `locales/*/site.ftl`. A
  person's daemon serves the same files and sends its root to `/app/`.
- **Website direction** from the design-taste-frontend skill ("The well-kept
  board"), with `DESIGN.md` winning where they disagree (no voice fills,
  three faces). Details and the checks that ran are in `docs/website.md`.

## Release (M11)

- **Draft releases.** A tag builds everything and opens a draft; a person
  publishes. Nothing reaches users without someone looking at it.
- **Archives with a fixed layout** (`bin/`, `share/polyph/web`) on every
  platform, so the daemon finds its web files the same way everywhere and the
  install scripts only copy two folders.
- **Install scripts install into the user's home** (`~/.local`,
  `%LOCALAPPDATA%\Programs\Polyph`), never system paths, so they work
  without root and on immutable distributions, and verify `SHA256SUMS`
  before touching anything.
- **Provenance over per-file signatures** for the command-line archives:
  GitHub build attestations (`gh attestation verify`) prove which workflow
  and commit built a file, with no key to manage. Platform code signing is
  for the desktop apps, where operating systems check it.
- **The relay image is built from the release binaries**, not compiled
  again in Docker: the same bytes that passed the smoke test, and arm64
  images without emulated compiles. `deploy/relay/Dockerfile` still builds
  from source for people who prefer that.
- **The desktop app carries `polyph`** (a Tauri sidecar) and the web files,
  so it works on a machine with nothing else installed; the overlay config
  keeps day-to-day `cargo build` of the app independent of a release binary.
- **Linux builds on Ubuntu 22.04** for a glibc 2.35 floor.
- **One version for everything**, checked against the tag before anything
  builds.

## Skipped work

- **The Nublet provider (M8) was skipped** on 2026-11-04 at the project
  owner's request: Nublet's provider interface was not available. It was
  picked up in the beta (2026-11-06): Nublet had no provider interface, so
  the beta team proposed a minimal one in Nublet (kaorii-ako/nublet#7: id,
  title, icon, a pure parse function, `start(host)` and `stop()`) and built
  the Polyph provider on it. It reads the existing read-only D-Bus service
  `org.polyph.Status1` instead of a new channel, and it does not approve or
  deny from the pill: the D-Bus service stays read-only, and the person
  decides with the request's full context in Polyph. See `docs/nublet.md`.

## Process

- **Design skills.** Impeccable, Emil Kowalski's design-engineering skill,
  design-taste-frontend, the GSAP skills and Anthropic's frontend-design were
  installed into `.claude/skills/`. The Apple design skill is installed with a
  Claude Code `/plugin marketplace add` slash command, which cannot be run from
  a headless build session; its guidance (macOS menus, shortcuts, settings
  window conventions, HIG clarity and deference) is applied from the brief and
  recorded in `DESIGN.md`. Listed in `STUBS.md` so a later interactive session
  installs it.

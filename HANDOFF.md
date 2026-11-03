# Handoff

Read this first when continuing the build. Then `PLAN.md` (checklist),
`STUBS.md` (what is not done), `DECISIONS.md` (why things are the way they
are), `PRODUCT.md` and `DESIGN.md` (product and visual truth). The original
brief is the source of all requirements.

## Current state

- **Milestone 0 (scaffold): done.**
- **Milestone 1 (protocol and relay): done.** `polyph-proto` and
  `polyph-relay` complete with tests; load test measured.
- **Milestone 2 (daemon, rooms, sessions): done**, with gaps listed in
  `STUBS.md` (LAN bridge joins, mDNS untested in CI).
- **Milestone 3 (agents and sandbox): done**, with gaps in `STUBS.md`
  (Windows isolation, seccomp, late-joiner transcripts).
- **Milestone 4 (coordination): next.**

## How to build and test

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p polyph-proto --target wasm32-unknown-unknown --no-default-features --features wasm,crdt
pnpm install && pnpm check:i18n
```

## What works

- `polyph-proto`: device identity, domain-separated signatures, `Signed<T>`,
  envelope binary format (property-tested), XChaCha20-Poly1305 group
  encryption with replay window, room genesis and certificate chains with
  role rules, HPKE epoch-key delivery, join codes, SPAKE2 join handshake with
  key confirmation and channel binding, relay frames, room messages with
  forward-compatible decoding, status feed schema, planner task-graph schema
  and validation, CRDT board/activity layouts (yrs) with convergence tests,
  wasm-bindgen exports.

- `polyph-relay`: WebSocket relay (axum) with device auth by challenge
  signature, certificate-gated publishing, read-only spectators, per-room
  replay buffers with cursors and gap detection, per-device encrypted
  snapshots in SQLite, room registration and owner-signed room ending, join
  code rendezvous with PAKE forwarding, 5-failure death/lockout, per-IP limits,
  plan member limits, health and opt-in metrics, TLS with self-signed
  certificates pinned by fingerprint (LAN mode), and an embeddable `serve()`.
  `polyph_relay::client::RelayClient` is the client used by daemons and tests.
- `polyph-testkit`: `polyph-loadtest` (see `docs/self-hosting.md`).
- Docs: `docs/protocol.md`, `docs/security.md`, `docs/self-hosting.md`.

- `polyph-core`: the room engine (no I/O): membership, knocks, key rotation,
  CRDT chat/board sync, acting-host election, claims with split
  reconciliation, approvals, voices, persistence and relay snapshots.
  `tests/sim.rs` runs multi-device simulations.
- `polyph-daemon`: identity and storage key, encrypted room files, room
  runtimes with relay connections, joins (both sides), LAN mode, local API.
- `polyph-i18n` and `polyph` (CLI): see `polyph --help` in any locale.
- `polyph-sandbox`: bubblewrap/Landlock/Seatbelt/job-object wrapping, the
  allowlisting proxy, the in-sandbox helper and `polyph sandbox test`.
- `polyph-agents`: ACP client and supervisor, policy presets, usage and
  budgets, summaries, registry and `agents doctor`.
- `polyph-localagent`: the built-in agent for OpenAI-compatible endpoints.
- Daemon methods `agent.*`, `agents.doctor`, `sandbox.test`, `usage.get`,
  `budget.set`; CLI `polyph agent start|list|stop|follow|prompt`,
  `agents doctor`, `sandbox test`, `usage`, `budget set`.
- Tests: `polyph-testkit/tests/m3.rs` (fake ACP agent through daemons and
  the sandbox) and `tests/localagent.rs` (local agent against a scripted
  model).

Smoke tests against real agents (`POLYPH_SMOKE=1`): **not run** — Claude
Code, Codex and Ollama are not installed in the build environment. Run them
on a developer machine:

```bash
POLYPH_SMOKE=1 cargo test -p polyph-testkit smoke_ -- --nocapture
```
- `polyph-testkit::harness`: relay + several daemons sharing a manual clock;
  `tests/m2.rs` holds the milestone 2 acceptance tests.

Try it locally with two homes:

```bash
cargo build -p polyph-cli
export POLYPH_KEYSTORE=file
target/debug/polyph relay serve --bind 127.0.0.1:18900 &
target/debug/polyph --home /tmp/a config set relay ws://127.0.0.1:18900/ws
target/debug/polyph --home /tmp/a config set api_port 0
target/debug/polyph --home /tmp/b config set relay ws://127.0.0.1:18900/ws
target/debug/polyph --home /tmp/b config set api_port 0
target/debug/polyph --home /tmp/a host --name payments
target/debug/polyph --home /tmp/b join <CODE> --name Bo
target/debug/polyph --home /tmp/a room knocks     # then: polyph --home /tmp/a approve <id>
```

## Next steps

1. M4: `polyph-mcp` (rmcp server with claim/memory/inbox/whiteboard tools,
   registered in `session/new`), `polyph-git` worktrees and merge queue,
   layer-3 file watcher, team-state digest, planner, memory anchoring, inboxes
   and loop guard, handoff, `polyph-merge`, reviewer, secrets redaction
   (gitleaks rules) applied before anything leaves the daemon.

## Known issues

- None in finished work.

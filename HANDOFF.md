# Handoff

Read this first when continuing the build. Then `PLAN.md` (checklist),
`STUBS.md` (what is not done), `DECISIONS.md` (why things are the way they
are), `PRODUCT.md` and `DESIGN.md` (product and visual truth). The original
brief is the source of all requirements.

## Current state

- **Milestone 0 (scaffold): done.**
- **Milestone 1 (protocol and relay): done.** `polyph-proto` and
  `polyph-relay` complete with tests; load test measured.
- **Milestone 2 (daemon): next.**

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

Run the relay locally: `cargo run -p polyph-relay -- --bind 127.0.0.1:8787`.

## Next steps

1. M2 daemon: see PLAN.md. Start with identity storage and the local API,
   then rooms and the join flow using `RelayClient`, then sync and election.
   Implement the election exactly as `docs/protocol.md` section 10 says.

## Known issues

- None in finished work.

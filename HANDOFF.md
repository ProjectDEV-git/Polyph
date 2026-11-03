# Handoff

Read this first when continuing the build. Then `PLAN.md` (checklist),
`STUBS.md` (what is not done), `DECISIONS.md` (why things are the way they
are), `PRODUCT.md` and `DESIGN.md` (product and visual truth). The original
brief is the source of all requirements.

## Current state

- **Milestone 0 (scaffold): done.**
- **Milestone 1 (protocol and relay): in progress.** `polyph-proto` is complete
  and tested; `polyph-relay` is next.

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

## Next steps

1. `polyph-relay` (M1): see PLAN.md.
2. M2 daemon.

## Known issues

- None in finished work.

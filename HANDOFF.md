# Handoff

> **2026-11-10 (Buffy/Codebuff session, Kao-authorized early start):** Step 0/1 reconciled in `docs/progress.md`. PR **#21 (SEC-14, KAO-75, `5432c6d`) merged with green CI** but Step 1 not fully confirmed (no written security/architect sign-offs found; SEC-14 status linkage to KAO-106 now updated in docs but sign-offs still missing). **Step 2 (KAO-106) partially implemented and pushed** as `beta/kao-106-sandbox-ipc-2` (`8eb7501`, PR TBD): seccomp in bwrap helper + TIOCSTI/TIOCLINUX denied, setsid in helper/agent spawn, expanded hidden paths (incl. macOS keychain folders), docs/stubs updated; `cargo test -p polyph-sandbox` and targeted clippy passed locally. **Not done:** full macOS keychain/Mach-service closure + CI proof, any person-level extra Mach services setting, required security+architect sign-off, full CI green on head, and merge. Usage tooling unavailable (ignored per Kao). 2026-11-08 `docs/progress.md` proposed edits remain unapproved/unseen here — **Kao: please review/confirm them if they exist elsewhere**.

Read this first when continuing the build. Then `PLAN.md` (checklist),
`STUBS.md` (what is not done), `DECISIONS.md` if present / `docs/decisions.md` (why things are the way they
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
- **Milestone 4 (coordination): done**, with gaps in `STUBS.md`
  ("Milestone 4 gaps": local agent without MCP tools, per-device merges,
  hosted mode not persisted, no CLI for plan/task/merge yet).
- **Milestone 5 (board, terminals, integrations): done**, with gaps in
  `STUBS.md` ("Milestone 5 gaps").
- **Milestone 6 (TUI): done**, with gaps in `STUBS.md` ("Milestone 6 gaps").
- **Milestone 7 (desktop app, browser client, status pill): done**, with
  gaps in `STUBS.md` ("Milestone 7 gaps").
- **Milestone 8 (PWA, VS Code extension, Nublet provider): done**, with the
  Nublet provider **skipped** by the project owner's decision (its interface
  is not available; see decisions records); gaps in `STUBS.md`
  ("Milestone 8 gaps").
- **Milestone 9 (cloud agents and extensibility): done**, with the scope
  derived from earlier milestones (no M9 brief in the repository; see
  `PLAN.md`) and gaps in `STUBS.md` ("Milestone 9 gaps").
- **Milestone 10 (website, accounts, billing): done**, with the scope
  derived from `PRODUCT.md`, `cloud/LICENSE` and earlier stubs (no M10 brief
  in the repository; see `PLAN.md`) and gaps in `STUBS.md` ("Milestone 10
  gaps": web push, team dashboard, prices, nothing run against real Stripe,
  GitHub or mail yet, not deployed).
- **Milestone 11 (release): done**, with the scope derived (no M11 brief
  in the repository; see `PLAN.md`) and gaps in `STUBS.md` ("Milestone 11
  gaps": nothing tagged yet, the release workflow has not run on GitHub,
  Windows signing, publishing).

**Public beta (2026-11-10):** PRs through #25 merged to `Main` (`e152bee`). PR #21 (SEC-14/KAO-75) merged with green CI; Step 1 reviewer sign-off/documentation verification and Step 2 (KAO-106, release-blocker) remain to be confirmed/advanced. See `docs/progress.md`.

## How to build and test

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build -p polyph-proto --target wasm32-unknown-unknown --no-default-features --features wasm,crdt
cargo clippy -p polyph-phone --target wasm32-unknown-unknown --features wasm -- -D warnings
pnpm install && pnpm check:i18n
pnpm --filter @polyph/ui test && pnpm -r typecheck
pnpm --filter @polyph/web build && cargo build -p polyph-cli && pnpm test:e2e
(cd apps/vscode && pnpm build && pnpm test && xvfb-run -a node test/e2e/run.mjs && node test/e2e/webview.mjs)
(cd apps/desktop && pnpm build && cd src-tauri && cargo clippy -- -D warnings)
cargo test --manifest-path cloud/Cargo.toml --target-dir target
cargo clippy --manifest-path cloud/Cargo.toml --target-dir target --all-targets -- -D warnings
```

Notes from prior handoff: build hygiene with tight disk (`CARGO_INCREMENTAL=0`, named target dirs); Playwright/Chromium for e2e; Tauri Linux deps; wasm-bindgen version must match `Cargo.lock`; VS Code smoke downloads into `apps/vscode/.vscode-test`.

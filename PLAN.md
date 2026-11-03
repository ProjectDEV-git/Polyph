# Build plan

Milestones follow section 12 of the brief. Each ends with passing tests, a
commit and an updated `HANDOFF.md`. Stubs go in `STUBS.md`.

## M0 — plan and scaffold
- [x] `PLAN.md`, `DECISIONS.md`, `HANDOFF.md`, `STUBS.md`
- [x] Cargo workspace (all crates from the layout), pnpm workspace
- [x] Lints: clippy `-D warnings` with `unwrap_used`/`expect_used` denied outside tests, rustfmt, eslint, prettier
- [x] i18n string check (`scripts/check-i18n.mjs`)
- [x] CI on Linux, macOS, Windows (`.github/workflows/ci.yml`)
- [x] Design skills installed; `PRODUCT.md` and `DESIGN.md` written

## M1 — protocol and relay
- [x] proto: ids, identity, domain-separated signatures, `Signed<T>`
- [x] proto: envelope binary format, group encryption, replay window, keyring
- [x] proto: genesis, certificates, role chains, revocation
- [x] proto: HPKE epoch-key delivery
- [x] proto: join codes, SPAKE2 handshake with confirmation and channel binding
- [x] proto: relay frames, join messages, room messages, status feed, plan schema, CRDT layouts
- [x] proto: wasm32 build with wasm-bindgen exports
- [x] relay: auth, rooms, fan-out, buffer (time, count, bytes), cursors and gaps
- [x] relay: snapshots per device with retention, quick-session GC
- [x] relay: code rendezvous, PAKE forwarding, attempt limits and lockout, per-IP limits
- [x] relay: health, Prometheus metrics (off by default), content-free logs, TOML/env config, plan limits hook
- [x] relay: embeddable API (for LAN mode and tests)
- [x] tests: plaintext never reaches the relay; forged and replayed messages; rotation; wrong codes lock

- [x] load test in testkit; results in docs/self-hosting.md (peak 71 MB normal, 118 MB worst case)
- [x] docs/protocol.md, docs/security.md

## M2 — daemon, rooms and sessions
- [x] identity storage (keyring with encrypted-file fallback), config dirs, local API token (0600 / ACL)
- [x] local API over localhost WebSocket (JSON-RPC style)
- [x] rooms: create, open, list, invite codes (one-time, reusable), members, roles, delete; quick sessions + export
- [x] join flow: PAKE, knock, approval, certificate issue, key delivery
- [x] presence, chat, CRDT sync, snapshots, catch-up
- [x] acting host election and claim arbiter hand-over, split reconciliation
- [x] LAN mode: embedded relay with TLS, mDNS, fingerprint bound into PAKE, bridging
- [x] testkit: relay + N daemons harness, split simulator
- [x] tests per brief

- [x] `polyph-i18n` (Fluent, embedded locales, negotiation) and the `polyph` CLI for M2 commands in four locales

## M3 — agents and sandbox
- [x] ACP client + supervisor; Claude Code, Codex, generic adapters; `agents doctor`
- [x] `polyph-localagent` (OpenAI-compatible, tools, text-protocol fallback, context budget)
- [x] autonomy presets and approval routing (owner first, escalation)
- [x] sandbox: bubblewrap, Landlock (fs + TCP port), Seatbelt, Windows job object; container mode; network proxy with allowlist; `sandbox test` (Windows AppContainer and seccomp: see STUBS)
- [x] usage accounting, price table, budgets; tool-call summaries
- [x] testkit fake ACP agent; tests; opt-in smoke tests

## M4 — coordination
- [ ] MCP server (rmcp) with all tools; claims three layers; worktrees; merge queue
- [ ] hosted-mode mirror; team-state digest; planner; memory anchoring
- [ ] inboxes + loop guard; handoff; intent-aware merge; reviewer; secrets redaction
- [ ] tests per brief

## M5 — board, terminals, integrations
- [ ] kanban, threads, activity with replay, whiteboards + agent tools
- [ ] shared terminals with input approval; GitHub; notifications, webhooks; Linear; status feed + D-Bus

## M6 — TUI
## M7 — desktop app, browser client, status pill
## M8 — PWA, VS Code extension, Nublet provider
## M9 — cloud agents and extensibility
## M10 — website, accounts, billing
## M11 — release

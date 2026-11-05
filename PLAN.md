# Build plan

Milestones follow section 12 of the brief. Each ends with passing tests, a
commit and an updated `HANDOFF.md`. Stubs go in `STUBS.md`.

## M0 — plan and scaffold
- [x] `PLAN.md`, `DECISIONS.md`, `HANDOFF.md`, `STUBS.md`
- [x] Cargo workspace (all crates from the layout), pnpm workspace
- [x] Lints: clippy `-D warnings` with `unwrap_used`/`expect_used` denied outside tests, rustfmt, eslint, prettier
- [x] i18n string check (`scripts/check-i18n.mjs`)
- [x] CI on Linux, macOS, Windows (`.github/workflows/test.yml`, named `ci.yml` until M11)
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
- [x] MCP server (`polyph mcp`) given to every agent in `session/new`, reaching the daemon through the agent's proxy
- [x] MCP tools: claims, team state, ask/reply/inbox, remember/recall/forget, list/create/move/finish tasks (whiteboard tools are M5)
- [x] claims: layer 1 explicit (MCP), layer 2 automatic on edits (file or symbol granularity), layer 3 worktree watcher
- [x] worktree and branch per task; daemon commits for agents (`polyph-git`)
- [x] merge queue: review agent, approval, intent-aware merge (`polyph-merge`), resolver agent, sandboxed checks, branch advance, optional push
- [x] planner (draft cards, dependencies, predicted overlaps, accept, unblock on merge)
- [x] hosted mode: shared working copy on the host, member mirrors (`polyph mirror`)
- [x] team-state digest; memory notes with anchoring
- [x] inboxes + loop guard (hops, per-pair rate, wakes per hour); delivery at turn boundaries; revoked-claim notices
- [x] handoff with brief, claims and worktree carried over
- [x] secrets redaction (gitleaks-style rules) before anything leaves the daemon
- [x] tests: `polyph-testkit/tests/m4.rs`, `m4_merge.rs`, `m4_hosted.rs`

## M5 — board, terminals, integrations
- [x] kanban edits (update/remove), threads, activity feed with replay (board checkpoints), all logged by author
- [x] multiple whiteboards (Excalidraw elements, indexed for late joiners) + agent tools (list, read as text, draw diagrams)
- [x] shared terminals (`polyph-term` PTYs) with input approval only the owner can grant
- [x] notifications (localized) and outgoing webhooks (JSON signed, Slack, Discord)
- [x] GitHub: connect, issue import, pull requests for tasks, PR state follows cards
- [x] Linear: connect, issue import, issue state follows cards
- [x] status feed (agents, budgets, task moves) + `org.polyph.Status1` D-Bus service (read-only)
- [x] CLI: term, thread, activity, whiteboard, plan, task, merge, memory, github, linear, webhook (four locales)
- [x] tests: `polyph-testkit/tests/m5.rs`, `m5_integrations.rs`

## M6 — TUI
- [x] surface brief (`docs/tui.md`) from PRODUCT.md and DESIGN.md
- [x] `polyph-tui`: theme (DESIGN tokens to true color / 256 colors / none with `NO_COLOR`), states as symbol + word, voices
- [x] views: chat with threads, board (move cards, 600 ms highlight), agents (follow transcripts, prompt, stop), activity with replay, merge queue, terminals (preview and attach)
- [x] approvals stack stating exactly what will happen; approve once / for the task / deny with a note
- [x] command line (`:join`, `:thread`, `:card`, `:plan`, `:agent`, `:term`, `:room`), rooms switcher, rail, keys overlay
- [x] live loop: coalesced refreshes, redraw on change capped at 30/s, spinner only while agents run, ordered terminal input
- [x] `polyph` with no arguments opens it in a terminal (help when piped)
- [x] four locales (`locales/*/tui.ftl`), Thai and CJK widths, narrow terminals
- [x] tests: `polyph-testkit/tests/m6.rs` (every view x locale x color depth x width; key flows; live updates)
## M7 — desktop app, browser client, status pill

- [x] `docs/app.md`: the app brief (question, voices as the face, shell, views, states, pill)
- [x] `packages/ui`: DESIGN tokens for Tailwind v4, bundled fonts, `voice.ts` checked against Rust, Fluent strings from `locales/`, daemon client, store refreshed by parts
- [x] views: chat with threads, board (drag and keyboard moves, plans), agents (live transcript, prompt, inbox, start, stop), activity with replay, merge queue, terminals (xterm.js, keyboard requests), whiteboards (Excalidraw, synced per element), settings, host and join, people waiting to join, command palette
- [x] approvals stack stating exactly what will happen; approve once / for the task / deny with a note; one-line finding under the room name
- [x] browser client (`apps/web`, Next.js static export) served by the daemon; `polyph web` one-time links; Host and Origin checks; CSP
- [x] desktop app (`apps/desktop`, Tauri 2): starts or finds the daemon, tray, always-on-top status pill window, native notifications, single instance
- [x] status pill (desktop window and `/pill/` in the browser) from the status feed, first approval answerable inline
- [x] four locales (`locales/*/app.ftl`), phone width, light and dark themes, reduced motion
- [x] tests: Vitest (voices, i18n, store, client, approvals), `pnpm test:e2e` (browser client against a real daemon), daemon unit tests for links, hosts, origins and static files
## M8 — PWA, VS Code extension, Nublet provider

- [x] `polyph-phone`: a browser-only room member with no I/O (relay auth, joiner side of the join PAKE, one room: chat, threads, board, transcripts as they arrive, approvals, knocks), the daemon's API shapes, wasm-bindgen exports
- [x] phone PWA (`apps/web` `/phone/`): engine in wasm over the relay, `PhoneClient` transport so the shared app runs unchanged, sealed IndexedDB storage, manifest, icons, service worker, approval alerts
- [x] pairing: daemon `phone.pair`, `polyph phone` (link and terminal QR), Settings > Pair a phone (QR); the phone joins as the person's member and knocks like anyone
- [x] VS Code extension (`apps/vscode`): daemon discovery and start, claim badges and editor notes, approvals as notifications, status bar, the shared app in a sidebar view and an editor tab through a message bridge, Pair a phone
- [x] four locales (`app.ftl`, `cli.ftl`, `vscode.ftl`; manifest strings generated into `package.nls*.json`)
- [x] tests: `polyph-testkit/tests/m8.rs` (phone engine through a real relay and daemon: join, chat, board, approvals, save/restore), Vitest (`PhoneClient`, extension with a stand-in `vscode` and a fake daemon), `scripts/e2e-phone.mjs` (Chromium at phone size, wasm engine, real daemon), `apps/vscode/test/e2e` (real VS Code, webview bundle)
- [ ] ~~Nublet provider~~ **skipped** on 2026-11-04 by the project owner's decision: Nublet's provider interface is not in the repository. Recorded in `STUBS.md` and `DECISIONS.md`; pick it up when the interface is available.
## M9 — cloud agents and extensibility

Scope note: the brief is not in the repository; this scope comes from what
earlier milestones left for M9 (the `agent-host` certificate capability and
`Surface::AgentHost` in the protocol, `agent start --on`, and the `plugin`,
`skills` and `template` commands in `STUBS.md`) and from `PRODUCT.md`
("some members run agents on their own servers so work continues with the
laptop closed"). Polyph-run hosted agents are proprietary and wait for
accounts and billing (M10).

- [x] protocol: requested capabilities on join requests and knocks, granted into certificates; `host` room messages (start, started, failed, prompt, stop)
- [x] agent hosts: enrollment (`polyph agent-host invite|join|workdir`), start/prompt/stop from any of the person's devices (`agent start --on`), approvals as for any agent, cloud flag in status feeds, Run on in the app, the phone starts agents on hosts
- [x] skills: `.polyph/skills/` (project) and personal skills; MCP tools `list_skills`, `read_skill`; `polyph skills`
- [x] plugins: manifest, per-device store, MCP tool servers in every agent session inside its sandbox, hooks on room events in a fresh sandbox with declared hosts only; `polyph plugin`; Settings lists them
- [x] templates: four built-in (startup, hackathon, open-source, agency) and personal ones; `polyph template`
- [x] tests: `m9.rs` (agent hosts, teammate refused, restart, workdir, phone with the laptop stopped), `m9_extend.rs` (templates, skills and a plugin tool through an agent, a hook answering once), crate unit tests
- [x] docs: `docs/agent-hosts.md`, `docs/extending.md`
## M10 — website, accounts, billing

Scope note: no M10 brief is in the repository; this scope comes from
`PRODUCT.md` (website positioning, plans with undecided prices, open core with
proprietary accounts and billing under `cloud/`), `cloud/LICENSE`, and what
`STUBS.md` left for M10.

- [x] relay: snapshot retention per room through the `Policy` (per plan)
- [x] `cloud/` `polyph-cloud` (proprietary): email-code and GitHub device-flow sign-in, sessions, devices linked by an `account-link` signature, teams and invites, plans from config (no prices set), Stripe checkout, portal and signed webhooks, the hosted relay with a plan policy, `polyph-cloud serve`
- [x] daemon `cloud` module and CLI `polyph login`, `logout`, `cloud status` (four locales)
- [x] website (`apps/web`): home, pricing ("Pricing coming soon" until set), download (build from source), security, privacy and terms drafts, account page; four locales at `/`, `/th/`, `/ja/`, `/zh-Hans/`; GSAP, ScrollTrigger and Lenis on site pages only; `vercel.json` headers including the phone CSP
- [x] daemon serves its root as `/app/` now that the export holds the website
- [x] tests: `cloud/tests/cloud.rs` (API with Stripe and GitHub stand-ins), `cloud/tests/daemon_login.rs` (a real daemon signs in), relay per-room retention, daemon root redirect; website inspected at three widths (`docs/website.md`)
- [x] docs: `docs/website.md`, `cloud/README.md`; CI job for `cloud/`
- [ ] web push, team dashboard (opt-in usage summaries), spectator link limits, docs translation: see `STUBS.md` "Milestone 10 gaps"
## M11 — release

Scope note: no M11 brief is in the repository; this scope comes from what
earlier milestones left for "release" (`STUBS.md`, `HANDOFF.md`) and the
`deploy/` directory named in the layout.

- [x] release archives (`scripts/release/package.mjs`): `bin/polyph`, `bin/polyph-relay`, `share/polyph/web`; a `dist` build profile; checksums
- [x] smoke test of an archive (`scripts/release/smoke.sh`): versions, relay `/healthz`, the daemon serving the browser client from the archive
- [x] install scripts (`scripts/install.sh`, `scripts/install.ps1`): latest or pinned release, SHA-256 check, into the user's home with no root
- [x] desktop app with `polyph` bundled as a Tauri sidecar and the web files as resources (`tauri.release.conf.json`, `scripts/release/desktop-prep.mjs`); the daemon finds web files inside app bundles
- [x] VS Code `.vsix` (`pnpm --filter polyph-vscode package`): icon, Marketplace README, changelog, `.vscodeignore`; the VS Code smoke test can load the unpacked package
- [x] `.github/workflows/release.yml`: on a `v*` tag, version check, web export with the phone engine, five targets (archive, smoke test, desktop packages), `.vsix`, `SHA256SUMS`, build provenance, relay image on GHCR (amd64, arm64), draft release with notes from `CHANGELOG.md`; macOS signing when secrets are set
- [x] `deploy/relay` (Dockerfile, release Dockerfile, Compose with Caddy, systemd unit, example config), `deploy/cloud` (Dockerfile, Compose, config and env examples), `deploy/web` (Caddyfile with the site's headers)
- [x] one version everywhere (`scripts/release/version.mjs`), `CHANGELOG.md`, `docs/release.md`; `docs/self-hosting.md` with Docker, systemd, EC2 and LAN mode; README install section; the website's download page
- [x] workflows: `test.yml` (unit tests on three systems, the hosted service, TypeScript, end-to-end suites, desktop build, packaging), `release.yml` (runs `test.yml` first; tag, Actions-tab version, or dry run), `release-prepare.yml` (version bump pull request); shared setup in `.github/actions/`
- [ ] not done: tagging and publishing the first release, Windows code signing, Marketplace publishing, deployments (see `STUBS.md` "Milestone 11 gaps")

## M12 — your own agents, shared context, cloning, plugins, big projects

- [x] connect your own agents: `polyph mcp` outside a sandbox and `/mcp` on the daemon; `polyph connect`/`disconnect` for Claude Code, Codex, Cursor, VS Code, Gemini CLI, Windsurf, Zed, OpenCode; `polyph start`; `polyph doctor` (`tests/connect.rs`)
- [x] shared context: project brief, `get_context`, `search`, `recent_work`/`polyph catchup`, task summaries; agent messages kept across restarts (`tests/context.rs`)
- [x] clone from GitHub: the room remembers its repository without credentials; `polyph join` clones into an empty folder (blobless for large repositories); `polyph sync`; background fetch (`tests/repo.rs`)
- [x] plugins: Claude Code plugin with an edit hook (`plugins/claude-code`), VS Code registers Polyph for Copilot, first-run checklist in the desktop app
- [x] big projects: `next_task` through the claim arbiter, code areas (`.polyph/AREAS` or CODEOWNERS) in get_context, claims, next_task and the planner; 30 agents on a 100,000-file repository (`tests/scale.rs`, `docs/big-projects.md`)

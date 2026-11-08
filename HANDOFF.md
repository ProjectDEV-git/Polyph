# Handoff

> **Paused for the 5-hour usage limit (2026-11-08 06:10 UTC).** Kao
> reported 72% at 05:58 UTC. Merged: #15, #17, #18, #14. Open: #16, merged
> with Main at c33287e and verified locally; it needs one review of the
> merge resolution, then green CI. Resume from `docs/progress.md` ("Next
> after the pause"). Weekly limit at warning level (resets 2026-11-13).

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
  is not available; see `DECISIONS.md` "Skipped work"); gaps in `STUBS.md`
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

The website's visual check (design-taste-frontend skill): build, serve
`apps/web/out` (for example `python3 -m http.server 4310 --directory
apps/web/out`) and run `node .claude/skills/design-taste-frontend/scripts/inspect.mjs
http://127.0.0.1:4310/`. It must be served from the export's root, or the
`/_next/` assets do not load.

The web build compiles the phone engine to wasm (`scripts/build-phone-wasm.mjs`):
it needs `rustup target add wasm32-unknown-unknown` and `cargo install
wasm-bindgen-cli --version <the wasm-bindgen version in Cargo.lock>`. The
VS Code smoke test downloads VS Code into `apps/vscode/.vscode-test` once.

Build hygiene: `CARGO_INCREMENTAL=0` and `rm -rf target` when the disk
fills; the desktop shell has its own `apps/desktop/src-tauri/target`.
`pnpm test:e2e` needs Playwright with Chromium (`NODE_PATH=$(npm root -g)`
where it is installed globally). Tauri on Linux needs the WebKitGTK
development packages (`libwebkit2gtk-4.1-dev`, `libgtk-3-dev`,
`libayatana-appindicator3-dev`, `librsvg2-dev`, `libsoup-3.0-dev`).

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
- `polyph-mcp`: the MCP stdio server (`polyph mcp`) with tools `claim_files`,
  `release_files`, `list_claims`, `team_state`, `ask_agent`, `reply`,
  `read_inbox`, `remember`, `recall`, `forget`. The daemon passes it to every
  agent in `session/new`; it calls the daemon at `http://polyph.internal`
  through the agent's proxy (`polyph_sandbox::proxy::start_with`).
- Daemon `coord` module: tool handlers, inboxes delivered at turn boundaries,
  wakes for agent messages, loop guard, revoked-claim notices, team digest.
  API `memory.list`, `inbox.send`.
- `polyph-core::memory`: note layout in the Memory document and re-anchoring.
- Tests: `polyph-testkit/tests/m4.rs`; the fake agent has `mcp` and
  `echo_prompt` steps and starts the MCP server like a real agent.
- `polyph-git`: worktrees, commits for agents, `merge-tree` merges, branch
  advance, push. `polyph-merge`: symbols, diff3, symbol-level merge,
  conflict markers and the resolver prompt.
- Daemon `tasks`: task worktrees (`agent.start` with `task`), `task.finish`,
  the merge queue (`merge.queue`), reviewer and resolver helper agents,
  sandboxed checks, `agent.handoff`. `plan`: `plan.create`, `plan.accept`.
  `watch`: claims layer 3. `mirror`: hosted mode (`mirror.host`,
  `mirror.open`, `mirror.status`; CLI `polyph host --mode hosted`,
  `polyph mirror <dir>`). `polyph-core::redact`: secret redaction.
- `polyph.toml` gained `[merge]` (into, reviewer, resolver, approval,
  run_checks, push), `[planner] agent` and `[claims]` (granularity, watch).
- Tests: `m4_merge.rs` (the brief's success path: two tasks, claims, review,
  approval, a conflict merged keeping both intents; resolver agent; reviewer
  sending a task back; planner; handoff; symbol claims; watcher) and
  `m4_hosted.rs`.

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

## Milestone 5 pieces

- `polyph-core::activity` (board diff, checkpoints, replay) and
  `polyph-core::whiteboard` (layout, text description, diagram layout).
- Daemon: `feed` (activity entries), `boards` (whiteboards), `terms` (shared
  terminals), `notify` (notifications, webhooks), `integrations` (sealed
  tokens), `github`, `linear`, `status` (feed) and `dbus` (Linux).
- API: board.update/remove, thread.create/list/get, activity.list/replay,
  whiteboard.*, term.*, webhook.*, github.*, linear.*, integrations.status.
- MCP tools: list_whiteboards, read_whiteboard, draw_diagram.
- CLI: term, thread, activity, whiteboard, plan, task, merge, memory, github,
  linear, webhook. Texts in `locales/*/{activity,notify,work}.ftl`.
- Tests: `m5.rs` (board/threads/replay, whiteboards and agents, terminals,
  notifications and webhooks, D-Bus) and `m5_integrations.rs` (GitHub and
  Linear against local stand-ins).

## Milestone 6 pieces

- `docs/tui.md`: the TUI brief (layout, keys, color, motion).
- `polyph-tui`: `theme` (tokens, depths, statuses), `conn` (Backend trait,
  WebSocket client), `model` (state, refresh by parts, events), `ui`
  (drawing, width-aware fit/wrap), `keys` (keymap, commands, actions), `run`
  (loop). `polyph` with no arguments opens it (`polyph-cli` `tui()`).
- `polyph-testkit::tui::InProcess` runs it against an in-process daemon;
  `tests/m6.rs` covers views x locales x color depths x widths, key flows
  (approve, chat, threads, board moves, commands, terminal attach and the
  keyboard request) and live updates (moved-card highlight, incoming chat).
- Try it: the "Try it locally" steps above, then run `polyph --home /tmp/a`.

## Milestone 7 pieces

- `docs/app.md`: the app brief (one question, voices as the face, shell,
  views, states, the pill, how each surface connects).
- `packages/ui` (`@polyph/ui`): `styles.css` (DESIGN tokens as CSS variables
  mapped into Tailwind v4, bundled fonts), `voice.ts` (golden vectors from
  `polyph-core`), `i18n` (Fluent; `scripts/bundle-locales.mjs` writes
  `locales.gen.ts`), `api/client.ts` (JSON-RPC over the WebSocket, status
  feed, reconnects), `store.ts` (refresh by parts, like the TUI), views in
  `src/views/` (shell, rail, chat, board, agents, activity, merges,
  terminals, whiteboards, settings, start, approvals, palette, pill) and
  `PolyphApp` as the entry point. Strings in `locales/*/app.ftl`.
- `apps/web` (`@polyph/web`): Next.js static export with `/app/` and
  `/pill/`; `src/client.tsx` trades the one-time code for a session.
- Daemon `web` module: one-time links, sessions, Host/Origin checks, static
  files with a CSP; API `web.link`; routes `/web/session` and the client's
  files. CLI `polyph web [--no-open]`.
- `apps/desktop` (`@polyph/desktop`): Vite front end (app and pill pages) and
  `src-tauri` (Tauri 2, outside the Cargo workspace): `daemon.rs` finds or
  starts the daemon, `lib.rs` has the commands, tray, pill window and
  notification plugin.
- Fixed in passing: the daemon panicked at start when a secret-service
  keychain was reachable (blocking call inside the async runtime), and the
  merge tests (`m4_merge.rs`) failed wherever bubblewrap is installed (agents
  could not see scripts in `/tmp`).
- Try it: `cargo build -p polyph-cli && pnpm --filter @polyph/web build`,
  then `POLYPH_WEB_DIR=$PWD/apps/web/out target/debug/polyph daemon run` and
  `target/debug/polyph web`. Desktop: `cd apps/desktop && pnpm build && cd
  src-tauri && cargo run --features tauri/custom-protocol` (with `polyph` on
  PATH).

## Milestone 8 pieces

- `polyph-phone`: `Phone` (relay auth, join, room), `api` (the daemon's
  methods and shapes), `wasm` (exports). `polyph-core::room::approval_title`
  moved there from the daemon.
- `packages/ui`: `phone/client.ts` (`PhoneClient`), `AppClient` (what the
  app runs on), `Platform.kind` `phone`/`vscode`, `Platform.views`,
  `Platform.joinCode`, `core.ts` (React-free entry), `describe.ts`.
- `apps/web`: `/phone/` (`src/phone/` engine loader, sealed `Vault`, the
  page), `public/phone/` (manifest, icons, `sw.js`; `engine.*` generated).
- Daemon: `phone.pair` (`src/phone.rs`: link and QR), the phone CSP in
  `web.rs`; CLI `polyph phone`; Settings > Pair a phone.
- `apps/vscode`: `daemon.ts` (find or start), `hub.ts` (one connection,
  webview bridge host side), `bridge.ts` (webview side), `claims.ts`,
  `extension.ts`, `webview/` (the shared app), `scripts/` (esbuild, nls).
- Try it: `pnpm --filter @polyph/web build`, run a plain relay and a
  daemon (see "Try it locally"), then `polyph phone --base
  http://127.0.0.1:<api port>/phone/` with `POLYPH_WEB_DIR=$PWD/apps/web/out`
  and open the link in a browser. VS Code: `cd apps/vscode && pnpm build`,
  then open the folder in VS Code and run the extension (F5).

## Milestone 9 pieces

- Protocol: `JoinRequest.capabilities`, `Knock.capabilities`,
  `messages::AGENT_HOST`, `RoomMessage::Host(HostMsg)`.
- Core: `RoomCore::is_agent_host`, `RoomEvent::Host`; `approve_knock`
  grants known capabilities.
- Daemon `hosts` (agent hosts: list, invite, join, start/prompt/stop on a
  host, the host side), `plugins` (tool servers for agents, hook runner);
  config `host_workdir`, `host_rooms`; API `agent.hosts`, `host.*`,
  `plugin.*`, `skills.*`, `template.*`; `agent.start` with `on`.
- `polyph-plugins`: `manifest`, `store`, `skills`, `templates`.
- MCP tools `list_skills`, `read_skill`. The phone engine starts, prompts
  and stops agents on hosts.
- CLI `agent-host`, `agent hosts`, `agent start --on`, `plugin`, `skills`,
  `template`. App: Run on in Agents > Start; Settings > Plugins.
- Tests: `m9.rs`, `m9_extend.rs`; test binaries `polyph-test-plugin-mcp`
  and the fake agent's named-server `mcp` steps.
- Docs: `docs/agent-hosts.md`, `docs/extending.md`.

## Milestone 10 pieces

- Relay: `Hub::expire_snapshots_at` expires snapshots per room through the
  `Policy` (owner device → plan → retention).
- Proto: `ctx::ACCOUNT_LINK`.
- `cloud/` (`polyph-cloud`, proprietary, built on its own): `config`,
  `plans`, `db` (SQLite), `mail` (Resend-compatible, log, memory), `billing`
  (Stripe checkout, portal, webhook signatures and events), `policy`
  (`CloudPolicy` for the relay), `api` (axum, CORS for the website), `serve`
  and the `polyph-cloud serve` binary. See `cloud/README.md`.
- Daemon `cloud` module (`cloud.login.start`, `cloud.login.finish`,
  `cloud.status`, `cloud.logout`), config key `cloud`; CLI `login`, `logout`,
  `cloud status` (`crates/polyph-cli/src/account.rs`); strings in
  `locales/*/account.ftl` and `locales/*/cloud.ftl` (emails).
- Website: `apps/web/src/site/` and the routes in `apps/web/app` (English at
  the root, `app/[lang]/` for th, ja, zh-Hans); strings in
  `locales/*/site.ftl`; `apps/web/vercel.json`; brief and checks in
  `docs/website.md`. `@polyph/ui` now also exports `./voice` and `./mark`.
- The daemon's web server redirects `/` to `/app/`.
- Try it: `cargo run --manifest-path cloud/Cargo.toml --target-dir target --
  serve` with a `cloud.toml` holding `dev = true`, `allowed_origins =
  ["http://127.0.0.1:4310"]` and `api_bind = "127.0.0.1:8790"`; then
  `polyph config set cloud http://127.0.0.1:8790` and `polyph login` (the
  code is in the service's log). For the account page, build the site with
  `NEXT_PUBLIC_POLYPH_CLOUD=http://127.0.0.1:8790` and serve `apps/web/out`.

## Milestone 11 pieces

- `scripts/release/`: `package.mjs` (archive), `smoke.sh` (checks an
  archive), `desktop-prep.mjs` (sidecar), `version.mjs` (one version, tag
  check), `notes.mjs` (changelog section). `scripts/install.sh`,
  `scripts/install.ps1`. Cargo profile `dist`.
- `.github/workflows/test.yml` (all tests, end-to-end suites, packaging),
  `release.yml` (gated on `test.yml`), `release-prepare.yml`;
  `.github/actions/` for shared setup; `docs/release.md` explains them,
  signing secrets and verification. Playwright is a root dev dependency
  (1.63.0); the e2e scripts give their daemon a free port.
- `apps/desktop/src-tauri/tauri.release.conf.json` (sidecar, web resources,
  package dependencies). The daemon's `web_dir` also looks in app bundles.
- `apps/vscode`: `package` script, `media/icon.png`, `README.md`,
  `CHANGELOG.md`, `.vscodeignore`; `POLYPH_VSCODE_EXT` makes its smoke test
  load an unpacked `.vsix`.
- `deploy/relay`, `deploy/cloud`, `deploy/web`; `docs/self-hosting.md`
  (Docker, systemd, EC2, LAN), `CHANGELOG.md`, README install section,
  the website's download page.
- Try it:

  ```bash
  cargo build --profile dist --locked -p polyph-cli -p polyph-relay
  pnpm --filter @polyph/web build
  node scripts/release/package.mjs
  scripts/release/smoke.sh dist/polyph-*-x86_64-unknown-linux-gnu.tar.gz
  (cat dist/*.sha256 > dist/SHA256SUMS; python3 -m http.server 4320 --directory dist &)
  POLYPH_INSTALL_BASE=http://127.0.0.1:4320 POLYPH_PREFIX=/tmp/p sh scripts/install.sh
  pnpm --filter polyph-vscode package
  node scripts/release/desktop-prep.mjs && (cd apps/desktop && pnpm tauri build --config src-tauri/tauri.release.conf.json --bundles deb)
  ```

## Next steps

1. Cut the first release: tag `v0.1.0` on `Main` once this is merged, watch
   the release workflow (it has not run on GitHub yet), test the draft's
   downloads on each OS, publish.
2. Add Apple signing secrets and a Windows code-signing certificate.
3. Deploy the website and `polyph-cloud`; run billing against Stripe test
   mode, GitHub sign-in and a mail provider; decide prices and member limits.
4. Publish the VS Code extension; web push and the team dashboard
   (`STUBS.md`).
5. The skipped Nublet provider, if its interface becomes available.
6. Give `polyph-localagent` the Polyph MCP tools; run the real-agent smoke
   tests; try the apps on macOS, Windows and real phones.

## Known issues

- None known in finished work. CI passes on Linux, macOS and Windows;
  remaining gaps are listed in `STUBS.md`.

# Polyph

Polyph lets several people, each on their own machine and network, run their
own coding agents (Claude Code, Codex, OpenCode, ACP agents, local models) in
one shared, end-to-end encrypted room. Agents claim files before editing, talk
to each other and get work split by a planner; people chat, approve, plan on a
shared board and watch from a TUI, desktop app, browser, phone or VS Code. A
Rust daemon runs on each machine; a relay on Fly.io forwards ciphertext only.

The current goal is a free public beta (milestones M-B0 to M-B5). The full
brief and exit criteria are in `HANDOFF.md` ("Public beta") and the Linear
project "Polyph public beta" (team KAO). Product truth is `PRODUCT.md`,
visual truth is `DESIGN.md`, and `STUBS.md` is the honest list of what is not
done. Nothing in `STUBS.md` may be described as done anywhere.

## Start and end of every session

Start: read this file, `docs/progress.md`, `docs/decisions.md` and
`docs/known-issues.md`, then `HANDOFF.md`.

End: update `docs/progress.md` (what was done, what comes next, usage
readings), add new decisions to `docs/decisions.md` and new gotchas to
`docs/known-issues.md`, and run the `reflect` skill: it proposes skill or
`CLAUDE.md` edits, with evidence from the session, in `docs/progress.md`.
Do not apply those edits until Kao approves them.

## Rules

- The main session is the orchestrator. It plans, writes briefs and checks
  results. It does not do bulk work (large edits, long test triage, log
  reading, doc writing) when a worker can do it.
- Workers are subagents in `.claude/agents/`. Route models by role:
  - Opus: lead, product-manager, security-auditor, architect-reviewer, the
    final release gate.
  - Sonnet: rust-engineer, typescript-engineer, qa-engineer,
    product-reviewer, design-critic, accessibility-auditor, i18n-reviewer,
    docs-writer, release-engineer, launch-writer.
  - Haiku: triage, changelog drafts, link and copy checks, locale-key
    checks, CI log reading, and the generic `builder` and `reviewer`.
  - Escalate a Sonnet task to Opus only when it has failed twice or it
    touches crypto, the protocol or the sandbox.
- Every brief states the goal, the exact files to read, and a "Done when:"
  check that can be run or seen (a command and its expected result, or a
  file that must exist with given content).
- Before accepting a worker's output, run its "Done when" check yourself. If
  it fails, send the work back with the failure output. Never pass a worker's
  report on unchecked.
- Changes to `polyph-proto`, the relay's auth or `polyph-sandbox` need
  sign-off from both security-auditor and architect-reviewer before merge.
  `polyph-merge`, `docs/protocol.md` and the install/update scripts need
  architect-reviewer. No release with an open critical or high security
  finding.
- Never weaken or change a test to make it pass. Stop and fix the process if
  `Main` CI is red for more than one merge cycle, or an issue bounces between
  two agents twice.
- Hard stops (put to Kao as a `human-only` item plus a note at the top of
  `HANDOFF.md`): money or paid services, anything legal, posting from public
  accounts, wire or crypto changes that break existing rooms or clients,
  deleting data or branches that are not the team's own.
- Copy voice (from `PRODUCT.md`): direct, specific, no hype words, no
  exclamation marks, no emoji in product copy.

## Usage limits

Kao's plan has a 5-hour limit and a weekly cap. Once the limit is hit nothing
can run, so pause before it.

- At most 3 workers at once. Give each worker the exact files to read. Do not
  re-read large files already summarized. Use Haiku for logs and CI output.
- At the start of every loop iteration run
  `POLYPH_TOKEN_LIMIT=62000000 .claude/scripts/usage-check.sh` (ccusage with
  Kao's 5-hour limit of about 62M tokens) and record tokens, percent and
  reset time in `docs/progress.md`. If it gives no
  percentage, count worker dispatches and checkpoint after every 6 (see
  `docs/decisions.md`, "Usage measurement in cloud sessions").
- At 75% of the block: stop dispatching, let running workers finish, commit
  work in progress, update `HANDOFF.md` and `docs/progress.md` with exactly
  what comes next, then wait for the reset (`sleep 540` steps where the shell
  allows it).
- Near the weekly cap: save your place, put a note at the top of
  `HANDOFF.md` saying it stopped for the weekly limit and when it resets, and
  stop.

## Folder map

```
crates/            Rust workspace
  polyph-proto       wire format, E2EE, join handshake, CRDT layouts (risky)
  polyph-relay       WebSocket relay (risky: auth, limits)
  polyph-core        room engine, claims, redaction, memory (no I/O)
  polyph-daemon      per-machine daemon: rooms, agents, merge queue, API
  polyph-cli         the `polyph` binary
  polyph-sandbox     bubblewrap/Landlock/Seatbelt/job-object wrapping (risky)
  polyph-agents      ACP client, supervisor, policies, budgets
  polyph-localagent  built-in agent for OpenAI-compatible endpoints
  polyph-mcp         MCP server agents get (claim_files, ask_agent, ...)
  polyph-git         worktrees, agent commits, merges
  polyph-merge       symbol-level merge and resolver (risky)
  polyph-tui         ratatui terminal UI
  polyph-term        shared terminals
  polyph-phone       phone engine (wasm)
  polyph-plugins     plugins, skills, templates
  polyph-i18n        Fluent loader
  polyph-testkit     harness, multi-daemon tests, smoke tests, load test
cloud/             polyph-cloud accounts and billing (proprietary, own build)
apps/web           Next.js site, browser client, phone PWA (GitHub Pages)
apps/desktop       Tauri 2 desktop app
apps/vscode        VS Code extension
packages/ui        shared React views, tokens, i18n, API client
plugins/claude-code  Claude Code plugin that enforces claims
locales/           en, th, ja, zh-Hans Fluent files
scripts/           install.sh, install.ps1, release/, i18n and e2e scripts
deploy/            fly (relay), relay, cloud, web
.github/workflows/ test.yml, release.yml, release-prepare.yml, pages.yml
.claude/agents/    role agents plus builder and reviewer workers
.claude/skills/    design skills, reflect
docs/              protocol, security, beta known limits, launch drafts,
                   progress.md, decisions.md, known-issues.md (orchestration)
```

Root documents: `HANDOFF.md` (state and how to build and test), `STUBS.md`,
`PLAN.md`, `DECISIONS.md` (product and architecture decisions),
`PRODUCT.md`, `DESIGN.md`, `CHANGELOG.md`. `docs/decisions.md` holds only
orchestration and process decisions made while running the beta team.

# Orchestration decisions

Process decisions made while running the beta team. Product and
architecture decisions stay in `DECISIONS.md` at the root.

## 2026-11-07

- **Two decision files.** The setup brief asked for `docs/decisions.md`; the
  repository already has `DECISIONS.md` for product and architecture. This
  file holds orchestration and process choices only, so neither file
  duplicates the other.
- **Existing role agents kept.** `.claude/agents/` already held the fifteen
  beta roles from section 4 of the brief. `builder` and `reviewer` (Haiku)
  were added alongside them, not in place of any.
- **Usage measurement in cloud sessions.** `npx ccusage@latest blocks
  --active --token-limit max` runs here, but it reads only this container's
  local logs. It shows tokens and cost for this session and the block's
  reset time, but no percentage of the plan limit, and it cannot see other
  sessions on the same plan. So the reading is recorded, and the fallback
  rule also applies: after every 6 worker dispatches, checkpoint (commit,
  update `docs/progress.md` and `HANDOFF.md`) and read ccusage again.
  Foreground `sleep` is blocked in this environment, so a pause at 75% ends
  the session after saving its place rather than sleeping in the shell.
- **Linear is not connected in this session.** The Linear MCP server needs
  authorization this session cannot start. Work is tracked in
  `docs/progress.md`, PR descriptions and `HANDOFF.md`, using the existing
  KAO issue numbers where they exist. When Linear is connected again, the
  next session copies new items across. Recorded as a `human-only` item.
- **Workers push to their PR branches.** The session's designated branch
  (`claude/orchestrator-workers-setup-2vizw9`) carries the orchestration
  setup. Fixes for the open beta PRs (#14, #15, #16) are pushed to those
  PRs' own `beta/*` branches, which are the team's own work, as section 6 of
  the brief asks (one worktree and branch per issue).
- **Red PRs before new work.** All three open security PRs failed Rust CI
  while `Main` was green. Fixing them comes before any new M-B1 work,
  because they hold the critical sandbox fixes the security sign-off needs.
- **SEC-3 fix must not block a member's own devices.** PR #15 refused every
  knock that reuses an existing member id, which also refused phone pairing
  and agent hosts (m8, m9 and the phone e2e failed). The worker brief keeps
  the stranger case blocked and allows the member's own new device without a
  wire change.

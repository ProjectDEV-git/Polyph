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
- **Own-device joins (PR #15).** The architect reviewer showed that any
  one-time code is enough for a stranger to take the inviter's member id.
  Decision: only codes minted by `phone.pair` and `host.invite`, recorded
  locally for the member who made them, allow a knock under an existing
  member id, and such knocks always need a person. m8's paired-phone setup
  may switch from `room.invite` to the real pairing call; its assertions do
  not change. This is a setup fix to match the user flow, not a weakened
  test.
- **`.git` in the agent's own checkout (PR #16).** Read-only `.git/config`,
  `.git/hooks` and `.git/info` under bubblewrap and Seatbelt, not the whole
  `.git`, so agents can still commit. Landlock and Windows keep SEC-13
  partly open and say so.
- **Opus for sandbox and certificate work.** Both reworks touch the sandbox
  or certificate checks, so they run on Opus per the routing rule.
- **Merge gates under trust (PR #14).** `[merge] approval`, `run_checks`
  and `[prices]` join the trusted sections. An untrusted file with checks
  fails the merge entry with "run `polyph trust`" instead of skipping
  checks. One assertion in `polyph-agents` that encoded the old rule is
  replaced, approved by the lead as a rule change.

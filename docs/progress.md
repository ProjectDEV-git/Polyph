# Progress

Session log for the beta team. Newest session first. Each entry says what was
done, what was checked, usage readings, and exactly what comes next.

## Milestone status

| Milestone | State | Gate |
|---|---|---|
| M-B0 Audit and plan | Mostly done in earlier sessions: role agents, `docs/beta.md`, `docs/beta-journeys.md`, 25 security findings, launch drafts, `polyph feedback`. Missing: baseline review scores in `docs/beta-review.md`, the dogfood room. | Not passed |
| M-B1 Core reliability | Three security PRs open (#14 SEC-12, #15 SEC-1..4, #16 SEC-13/16), all red in CI | Not started |
| M-B2 to M-B5 | Not started | |

## Session 2026-11-07 (orchestrator setup)

Usage at start: ccusage block started 05:00 UTC, 4h51m remaining, $1.11 so
far in this container (no plan percentage available, see
`docs/decisions.md`).

Done:
- `CLAUDE.md`, `docs/progress.md`, `docs/decisions.md`,
  `docs/known-issues.md`, `.claude/agents/builder.md`,
  `.claude/agents/reviewer.md`, `.claude/skills/reflect/SKILL.md`.
- CI on the three open PRs read (Haiku): #14 hangs in
  `m3 scripted_task_completes_and_teammates_see_it` on all three OSes; #15
  fails `m8`, `m9` and the phone e2e with "knock claims an existing member's
  id"; #16 fails `m4 secrets_are_redacted_before_leaving_the_device` ("no
  such agent").
- Dispatched three rust-engineer workers, one per PR branch.

Dispatch count this session: 4.

Dogfood setup (`polyph` built from `Main` at 448f203, `POLYPH_KEYSTORE=file`,
home `/root/dogfood-home`): `polyph start --name polyph-beta --yes
--no-connect` made the room and a code in 11 s, but the room never connects
to `wss://polyph-relay.fly.dev/ws`. The team falls back to plain worktrees
and PRs until DF-1 is fixed.

Dogfood issues (to file in Linear with label `dogfood` when it is
connected):
- DF-1 (release-blocker): the relay client trusts only the bundled webpki
  roots (`crates/polyph-relay/Cargo.toml`, `rustls-tls-webpki-roots`). Behind
  a TLS-inspecting proxy with a private CA in the system store (this
  container; many company networks) the connection fails. Repro: in this
  container, `polyph start`, then `polyph status` says "Not connected to the
  relay" while `curl https://polyph-relay.fly.dev/healthz` works. Fix: also
  load the OS trust store (rustls-native-certs or rustls-platform-verifier).
- DF-2 (release-blocker): the failure is silent. `daemon.log` has no line
  about the relay connection failing, and `polyph doctor` says only "cannot
  reach its relay" with the fix "polyph config get relay", which does not
  fix anything. It should print the TLS or network error and what to do.
- DF-3: `polyph start --yes` stops with "Polyph needs your name first"
  instead of asking or using `git config user.name`. A student following
  the two-minute promise hits this first.
- DF-4: `--name` means the room's name in `polyph start` and the person's
  name in `polyph join`. Confusing next to DF-3.
- DF-5: `polyph start --help` shows `-y, --yes` with no description.

Next:
1. Check each worker's result by running its "Done when" commands, then
   watch the PR's CI.
2. Security auditor and architect reviewer on #14, #15, #16 (sandbox and
   room security), then merge.
3. Baseline build and test run from `HANDOFF.md`; record pass and fail.
4. Product reviewer baseline journey and scores in `docs/beta-review.md`.

## Proposed edits (awaiting Kao)

None yet.

## Applied edits

None yet.

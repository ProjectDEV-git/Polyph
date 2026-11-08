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

Dispatch count this session: 6 at the first checkpoint (05:30 UTC; ccusage:
70k output tokens, $109 in the block, 4h40m to reset).

Results checked:
- #16: worker fixed a race in the m4 test (poll before the agent
  replicated). Checked: `cargo test -p polyph-testkit --test m4` passed 3 of
  3 runs. Architect review: CHANGES REQUESTED (own-checkout `.git/config`
  still writable, doc claims a read-only `.git` that does not exist,
  `sshCommand` and `hooksPath` overrides break trusted pushes and LFS,
  mirror deletes symlinked files on peers). Sent back to an Opus
  rust-engineer (sandbox rule) with the lead's decision: read-only
  `.git/config`, `hooks`, `info` under bwrap and Seatbelt; SEC-13 stays
  partly open for Landlock and Windows.
- #15: worker allows a member's own new device only through a one-time code
  that member's daemon made, with a person's approval. Checked:
  `cargo test -p polyph-core` (32 passed), `--test m8` (2), `--test m9` (3).
  Phone e2e not run locally; CI runs it. Architect review: CHANGES
  REQUESTED (any one-time invite lets a stranger take the inviter's member
  id; a demoted Maintainer can extend their old chain). Sent back to an
  Opus rust-engineer with the decisions in `docs/decisions.md`.
- #14: harness now trusts test project files like `polyph trust`; m3 has a
  60 s guard. Checked: `--test m3` 8 passed; diff adds trust calls and a
  timeout only. Architect review: CHANGES REQUESTED (`[merge] approval`
  honored untrusted; merges skip checks silently; `polyph trust` trusts
  blind). Sent back to Opus with merge settings and prices under trust.
- #16 rework (577e9ff): all 8 findings fixed; push stays guarded where an
  agent ran outside bwrap/Seatbelt (accepted). Checked: polyph-git 14,
  plugins 8, sandbox 6+2 (real bwrap), m4 4, m4_merge 7, fmt clean.
  Security auditor: NO SIGN-OFF (commondir redirect and nested-repo
  filters are critical, toml link read is high). Sent back to Opus with the
  read-only `.git` decision.

Dispatch count: 12 (05:55 UTC).

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

### Paused at 05:50 UTC (5-hour block at 76%, reported by Kao)

Resets at 09:40 UTC. The session checks hourly (background `sleep 3600`,
since foreground sleep is blocked here) and resumes after 09:40 UTC.
Running when paused (allowed to finish, nothing new dispatched):
- #14 rework on Opus (merge gates under trust, `polyph trust` shows what it
  trusts, data-dir refusal).
- #16 security fixes on Opus (read-only `.git`, `GIT_COMMON_DIR`, nested
  repos, toml link read).
#15 rework finished at b07f76e and is not yet verified by the lead.

Usage reading: ccusage shows 46M tokens in the block and the reset at
10:00 UTC, but gives no percentage, because there is no earlier block to
compare against. `get_session` `rate_limit_info` shows only the seven-day
window (`allowed_warning`). Kao's figure (76%) is the one that counts.
`.claude/scripts/statusline-capture.sh` is set as the local status line to
save Claude Code's `rate_limits.five_hour.used_percentage` to
`~/.claude/usage-status.json`. Whether a cloud session runs the status line
is not confirmed yet. `.claude/scripts/usage-check.sh` reads ccusage and the
rate-limit fields and prints CONTINUE, PAUSE or WAIT_UNTIL.

Reading at 06:00 UTC with the 62M limit: 49.3M tokens, 79%. Resets 09:40
UTC (Kao's figure; ccusage says 10:00). Hourly checks: 06:48 UTC 50.0M
(80%), 07:48 UTC 50.7M (81%), 08:18 UTC 51.3M (82%).

Resumed 09:41 UTC. get_session showed a new five-hour window (`five_hour`,
`allowed`, resets 14:40 UTC); ccusage still showed its old block (83%) until
10:00. The #14 and #16 Opus workers had stopped at about 05:46 UTC with no
report: #14 left 14 uncommitted files, #16 left nothing. Both dispatched
again (#14 resumes the uncommitted work). #15 checks running locally.
DF-1/DF-2 dispatched to a Sonnet rust-engineer.

The container restarted between about 10:00 and 12:54 UTC. All three
workers had finished and pushed before it:
- #14 at 0bd1d69: all nine items done, the one approved assertion change in
  `project.rs`, new m4_merge trust test. Needs architect and security
  review (data-dir refusal is sandbox policy).
- #16 at e1e00ef: all eight auditor findings addressed. Three tests changed
  to match decisions 1a and 1c (lead checked the diff: the no-task agent
  can no longer commit, the main checkout gets `--git-dir`). Open risk for
  the auditor: writable `refs/` lets a task agent move the base branch.
- DF-1/DF-2 on `beta/df-1-2-relay-tls` at 4c8a9d6: OS trust store plus
  `SSL_CERT_FILE` plus webpki fallback; relay errors logged and shown in
  `status`/`doctor`. Live check in this container connected to the relay.

#15 (b07f76e) verified by the lead: proto/core/relay and m2/m8/m9 all
pass. Sent to the architect reviewer and the security auditor.

Linear: DF items filed as KAO-91 (DF-1), KAO-92 (DF-2), KAO-93 (DF-3),
KAO-94 (DF-4, DF-5). 12:54 UTC reading: new ccusage block, 0%.

Lead's check run passed for #14 (0bd1d69), #16 (e1e00ef) and DF-1/2
(4c8a9d6): tests, i18n, clippy and fmt all clean. #15 architect: CHANGES
REQUESTED (upgrade gap, decided as re-issue on load; see decisions). #16
security re-audit and #14 architect re-review dispatched. DF-1/2 opened as
PR #17.

Reviews (13:00-13:20 UTC):
- #15 security: SEC-1 and SEC-4 SIGN-OFF (now low); SEC-2 and SEC-3 NO
  SIGN-OFF (two new highs: lower role overwrites a higher device's
  certificate; joiner claims an existing device id). One Opus rework covers
  those, the architect's upgrade gap and the mediums.
- #16 security: SEC-13 and SEC-16 SIGN-OFF, but two new highs (writable
  `refs/` moves the base branch; symlinked reflog written by the daemon)
  block the branch. Opus rework dispatched.
- #14 architect: earlier findings closed; new blocker (untrusted `[prices]`
  disables dollar budgets). Opus rework dispatched.
- KAO-95 filed for Kao (wire-change items from the #15 audit).
- #15 rework 241bb04 and #14 rework f1a8ac7 verified by the lead (all
  checks pass; #14 split one m4_merge test, every assertion kept). #15 is
  with the security auditor; one architect reviewer has #14 and #15.
  KAO-96 filed (ACP agents record no model, so dollar budgets never fire).
  Usage at 13:20 UTC: 28%.
- #14 f1a8ac7: architect APPROVE; security auditor dispatched. Lows filed
  as a follow-up issue.
- #15 241bb04: security SEC-1/3/4 SIGN-OFF, SEC-2 waits on the snapshot
  re-check; architect CHANGES REQUESTED (0.1.2 rooms where the Owner's role
  was changed lose every chain). Third Opus rework dispatched. KAO-95
  extended with signed supersession records.

Paused 13:45 UTC: Kao reported 96% of the 5-hour window (resets 14:40 UTC).
ccusage said 66% because the container restart lost its earlier logs;
get_session showed `five_hour` `allowed_warning`, which the check was not
given. No workers running.

State at the pause:
- #14 `beta/kao-73-project-trust` at 39c223a: architect APPROVE (f1a8ac7),
  security SIGN-OFF (SEC-12 now low), SEC-12 docs updated (39c223a, docs
  only). Ready to merge once PR CI is green.
- #16 `beta/kao-74-77-git-symlinks` at 82ce0e7: lead verified, all checks
  pass. Assertion changes follow the new `sandbox_git(branch)` signature
  and are stricter (checked). Needs the security auditor and the architect.
  Residual gap: a task agent can move other `polyph/` branches (never
  `main`); full fix renames task branches to `polyph/<id>/<slug>`.
- #15 `beta/kao-78-80-room-relay` at cf4b47e: not yet verified by the lead.
  Item 4 (re-issue after drop) not done because it contradicts a sim test;
  lead's call: keep the drop rule (CHANGELOG tells mixed-version rooms to
  update Owner and Maintainer devices first). Needs verification, then the
  security auditor and the architect, then the i18n reviewer for the new
  ja/th/zh-Hans activity strings.
- #17 (DF-1/2) open; needs the security auditor on the trust store change.
- Linear: KAO-91..98 filed this session (KAO-95 is Kao's wire-change
  question).

Resumed 14:44 UTC: five-hour flag cleared (only `seven_day`
`allowed_warning` left). #14 CI: Windows Rust job failed (triage reading
the log); #15 cf4b47e verification running; one security auditor on #16
and #17.
#15 cf4b47e verified by the lead (all pass); architect re-review
dispatched. 14:46 UTC: ccusage 75% (misaligned block), plan shows no
five-hour flag.
- #15 cf4b47e: architect APPROVE, security SIGN-OFF on SEC-1..SEC-4. Needs
  CI green ("TypeScript and translations" failed on an earlier head) and
  the i18n reviewer for new ja/th/zh-Hans strings. Lows: STUBS.md:100 and
  protocol.md 4.2 wording, docs/security.md downgrade sentence.
- #16 82ce0e7: security NO SIGN-OFF, two new highs (object swap under the
  same hash; symlink race between the guard and git's write) plus the
  `polyph/` branch gap. Fourth Opus rework dispatched: per-task object dir
  imported with fsck, daemon git writes in a sandbox, per-task ref folders,
  approval shows commit and diff stat.
- #17 4c8a9d6: security SIGN-OFF; macOS Rust CI red. Lows: strip control
  characters from relay errors, docs line.
- CI triage (Haiku) reading #14 Windows, #17 macOS, #15 TypeScript jobs.
  The first triage worker hung on a Bash permission prompt and was stopped;
  briefs for log reading now say Grep/Read only.

Session 2026-11-08 (after two container restarts, 02:47 and ~03:10 UTC):
- #15 merged into Main as 1f418dd (architect APPROVE, security SIGN-OFF on
  SEC-1..4, all CI green on ccd1a36). KAO-78/79/80 Done; residuals in
  KAO-100; wire-change items in KAO-95; Thai read and core error i18n in
  KAO-99.
- #14 8e2eb31: Windows `\\?\` path fixed; Windows stack overflow in the
  trust test remains. Escalated to Opus (second failure).
- #16 50abf02: lead verified (all checks; 13 removed asserts all have
  renamed or stricter replacements). Security: both earlier highs fixed on
  Linux, SEC-13 to medium, one open check (macOS hard links under Seatbelt).
  Architect: design sound, CI red (ubuntu author identity, windows dead
  code). Opus rework running (CI, Seatbelt `file-link` deny with macOS
  tests, gc.auto, import at start and warning, per-task branch record).
  Lead accepted the "bad object" window for beta; follow-up KAO-101.
  Low follow-ups in a separate issue.
- #17: security SIGN-OFF; macOS CI red (known m4 race, new m6 Thai TUI
  failure). Sonnet worker cherry-picks the m4 fix and investigates m6.
- Usage: plan shows only `seven_day` `allowed_warning`; no five-hour flag.

- 03:20-03:56 UTC: #17 test-only race fixes (m4 cherry-pick, m6 wait)
  verified; macOS green, Windows red on the new run. #14 25aa084: clap
  parser overflowed the 1 MiB Windows main thread; parsing and runtime now
  on a 16 MiB thread; lead ran the binary at 900 KiB stack. #16 f5a6ec4:
  all nine items plus m6; macOS CI green (first run of the Seatbelt
  hard-link tests), Windows red; lead's local verification running.
  KAO-93/94 on `beta/kao-93-94-start-name` (288054f, not yet verified by
  the lead). KAO-103 filed: `start`/`connect` rewrote the real
  `~/.claude.json` from a temp POLYPH_HOME (DF-6); this container's entry
  still points at a scratch home.

- 04:00-04:21 UTC: merged #17 (671eee0, relay TLS; KAO-91/92 Done) and
  #18 (049b9b7, `polyph start` name and flags; KAO-93/94 Done). #16 at
  c2a8b71: security SIGN-OFF (SEC-13 medium, SEC-16), architect's blockers
  were Windows CI and a docs claim about room warnings (fixed in c2a8b71);
  low items in KAO-102. #14 and #16 Windows CI still red; triage reading
  both logs. KAO-104 filed for the m4_merge Windows flake.

- 04:21-05:56 UTC: #14 Windows fixed (test built its expected key with
  std canonicalize), Main merged in (4b6bb9f, lead checked no line dropped
  from CHANGELOG/STUBS), all CI green, merged as 64d83d4. KAO-73 Done.
  #16 Windows test fixed (79e4eea); it now has code conflicts with Main in
  project.rs, agents.rs, main.rs (both PRs touch agent start and
  polyph.toml reads). Opus worker merging Main in, keeping both sides'
  security behaviour.

Merged so far: #15 (SEC-1..4), #17 (relay TLS), #18 (`polyph start`), #14
(SEC-12). Open: #16 (SEC-13/16; both sign-offs, needs merge with Main and
green CI). After #16, M-B1's three critical findings SEC-12, SEC-13 and
SEC-14 still need SEC-14 (KAO-75, host IPC reachable from the sandbox) and
SEC-15/17 (KAO-76, KAO-81).

- 05:58 UTC: Kao reported 72% of the 5-hour window. No new dispatches.
  The #16 merge worker runs to completion; the lead verifies it locally,
  saves state and pauses until the window resets. The plan's get_session
  shows no five-hour flag, so Kao's figure is the one to follow.

- 06:10 UTC: #16 merged with Main as c33287e (keeps #14's trust rules and
  data-dir check and #16's `read_text_under`, git isolation and warnings).
  Lead verified: all tests, clippy, fmt; no function from either parent
  missing. Paused for the 5-hour window (Kao: 72% at 05:58).

- 07:50 UTC: resumed after the estimated reset (window started 02:47
  after the overnight gap; plan shows no five-hour flag; ccusage new block
  at 07:00, near 0%). #16 c33287e: all CI green, clean. Security auditor
  reviewing the merge resolution; merge after that. KAO-75 waits for #16
  (both change polyph-sandbox).

- 07:58 UTC: #16 merged as 195ce98 (security SIGN-OFF on the merge
  resolution; all CI green). KAO-74 and KAO-77 Done; merge-review lows
  added to KAO-102. All five original beta PRs are in (#14, #15, #16, #17,
  #18). Dispatched: KAO-75 SEC-14 (Opus, `beta/kao-75-sandbox-ipc`),
  KAO-103 connect writes the real ~/.claude.json (Sonnet), KAO-104
  m4_merge flake (Sonnet, qa-engineer).

- 08:00-08:50 UTC: merged #19 (2ecc098, KAO-104 m4_merge race) and #20
  (8381268, KAO-103: per-home agent entries; ~/.claude.json untouched by
  the worker). KAO-105 filed (m4 claims race on macOS). #21 (KAO-75,
  SEC-14) opened at 9118a3d: lead verified locally (all tests, no removed
  asserts); macOS and Windows CI red (first real run of the deny-default
  Seatbelt profile); triage reading both logs. Reviews after CI is green.
  Usage 08:19 UTC: 40% (ccusage block from 07:00).

- 08:55 UTC: #21 reviews in. Architect CHANGES REQUESTED (SOCK_RAW
  seccomp bypass; /run symlink fail-open). Security: merge OK, SEC-14 now
  high per mode, four beta blockers filed as KAO-106. KAO-107 (real-Mac
  trial) for Kao. Opus rework of #21 running. Usage 63% at 08:54: no new
  dispatches until the reset; KAO-106 waits.

### Next after the pause

1. Usage: get_session, and Kao's figure if given. Continue only under 75%.
2. #16 c33287e: one narrow review of the merge resolution, one agent
   covering both roles' questions (security auditor, Opus): the trusted
   load in `project.rs` now reads through `read_text_under`, so a
   symlinked or hard-linked `polyph.toml` in the person's checkout is
   refused, not followed. Then watch CI and merge.
3. Next M-B1 security work: KAO-75 (SEC-14, critical: host IPC reachable
   from the sandbox), KAO-76 (SEC-15), KAO-81 (SEC-17). Then the follow-ups:
   KAO-98 (trust redirect), KAO-101 (bad-object window), KAO-103
   (`connect` rewrites ~/.claude.json), KAO-104 (m4_merge flake).
4. Dogfood room retry with the merged DF-1 build (relay TLS fix is on Main).

## Proposed edits (awaiting Kao)

None yet.

## Applied edits

None yet.

# Brief for Fable: take Polyph to its public beta

This brief hands the orchestrator role for the Polyph public beta to a
Fable session. It follows a four-part structure, read from the bottom of
the pyramid up:

1. Context: what exists, where things stand, how the team works.
2. Task: what to achieve, in what order, and how to tell each step is done.
3. Constraints: rules that hold whatever the task says.
4. Output: what to produce, where, and how to report.

Read all four parts before acting. When this brief and `CLAUDE.md`
disagree, `CLAUDE.md` wins, and you note the conflict in
`docs/known-issues.md`.

State as of 2026-11-10. Work was stopped on 2026-11-08 for the weekly usage
limit, which resets 2026-11-13 05:00 UTC. Do not start before then unless
Kao says so.

---

## Part 1. Context

### The product

Polyph lets several people, each on their own machine and network, run
their own coding agents in one shared, end-to-end encrypted room. Agents
claim files before editing, talk to each other and get work split by a
planner. People chat, approve, plan on a shared board and watch from a
TUI, desktop app, browser, phone or VS Code. A Rust daemon runs on each
machine. A relay on Fly.io forwards ciphertext only.

Product truth is `PRODUCT.md`, visual truth is `DESIGN.md`, and `STUBS.md`
lists what is not done. Nothing in `STUBS.md` may be described as done.

### The goal and its milestones

The goal is a free public beta. Linear project "Polyph public beta"
(team KAO, id P-KAO-18) holds the milestones and their gates:

| Milestone | Gate | State on 2026-11-10 |
|---|---|---|
| M-B0 Audit and plan | Every stub accounted for; baseline journey scores recorded | Not passed: `docs/beta-review.md` scores and the dogfood room are missing |
| M-B1 Core reliability | Multi-network test green in CI; smoke tests green on Kao's machine; no open critical or high security issue in the room engine, relay, daemon, claims, merge queue or joins | About half done (see below) |
| M-B2 First-hour experience | Every audience journey scores 4 or higher | Linear shows 100%; check the gate against `docs/beta-review.md` before trusting that |
| M-B3 Surfaces and polish | No open high-severity design, a11y or i18n issue; screenshots of every surface in both themes and four locales | Not started |
| M-B4 Release and infra | Security auditor and architect sign off in writing; fresh install from the public website works on Bazzite | Not started |
| M-B5 Launch | The exit criteria in `HANDOFF.md` | Not started |

### What was done in the last session (2026-11-07 to 2026-11-08)

Merged into `Main`:

| PR | Fixes | Linear |
|---|---|---|
| #14 | SEC-12: `polyph.toml` security sections apply only after `polyph trust` | KAO-73 |
| #15 | SEC-1 to SEC-4: relay snapshots, demotion, device takeover, join-code guessing | KAO-78, KAO-79, KAO-80 |
| #16 | SEC-13, SEC-16: agents kept out of `.git`; the daemon does not follow agent-made links | KAO-74, KAO-77 |
| #17 | Relay TLS trusts the OS store; relay errors shown in `status` and `doctor` | KAO-91, KAO-92 |
| #18 | `polyph start` finds the person's name; `--room` and `--me` | KAO-93, KAO-94 |
| #19 | Windows race in the m4_merge test | KAO-104 |
| #20 | `connect` names each home's agent entry and never replaces another home's | KAO-103 |

Open:

- **PR #21** (SEC-14, host IPC reachable from the sandbox, KAO-75), branch
  `beta/kao-75-sandbox-ipc`, head `5432c6d`. The lead's local run of every
  check passed. It still needs the security auditor and architect
  re-review of `5432c6d`, then green CI on Linux, macOS and Windows.

Filed and not started (all in P-KAO-18):

| Issue | What | Priority |
|---|---|---|
| KAO-106 | SEC-14 follow-up: LXD sockets, SSH agent sockets outside `/run`, TIOCSTI under Landlock, macOS keychain, keyring syscalls, Seatbelt `process-info`, `launchctl` test, person-level Mach services | Urgent, release-blocker |
| KAO-76 | SEC-15: Landlock hides nothing and drops its network rule on older kernels | High |
| KAO-81 | SEC-17: Windows low-integrity agents can read the API token | High |
| KAO-98 | An agent in the person's checkout can redirect which project's trusted settings apply | High |
| KAO-101 | Person's git shows "bad object" while a task agent's commits wait for import | High |
| KAO-96 | Dollar budgets never trigger for ACP agents | High |
| KAO-100 | SEC-2 to SEC-4 residuals (IPv6 /64, in-progress session cap, more) | Medium |
| KAO-105 | m4 claims race on macOS CI | Medium |
| KAO-82, KAO-84, KAO-83 | SEC-5 to 9, SEC-18 to 25, SEC-21/22 | Medium |
| KAO-97, KAO-102 | Low follow-ups from the #14 and #16 reviews | Low |

Waiting on Kao (`human-only`): KAO-107 (run real agents on a Mac under the
new Seatbelt profile), KAO-95 (wire-change decisions: join key proof,
signed revocations, signed supersession records), KAO-99 (native Thai
read), KAO-87, KAO-88, KAO-89.

### How the team works

- You are the orchestrator. You plan, write briefs, check results and
  merge. Workers do the bulk work. Workers are the subagents in
  `.claude/agents/` (role agents plus `builder` and `reviewer`).
- Model routing is in `CLAUDE.md`, and Kao may change it. Today it is:
  - Opus: lead, product-manager, security-auditor, architect-reviewer,
    release gate, and any sandbox, protocol or crypto work.
  - Sonnet: engineers, QA, reviewers, writers.
  - Haiku: triage, CI log reading, docs-only edits, `builder`,
    `reviewer`.
- Session records:
  - `docs/progress.md`: state, "Next after the pause", usage readings,
    and "Proposed edits (awaiting Kao)".
  - `docs/decisions.md`: orchestration decisions.
  - `docs/known-issues.md`: gotchas that cost time.
  - `HANDOFF.md`: the note at the top says where things stand.

### Environment facts that cost time last session

- The container restarts often, sometimes several times an hour. Background
  workers can finish while you are restarted. Their results stay in the
  task output files, so read those before redoing anything.
- ccusage undercounts after a restart, and its 5-hour block can be
  misaligned with the plan's window. The plan's own `rate_limit_info` from
  `get_session` shows a `five_hour` flag only near the limit. A figure
  Kao gives wins over both.
- A background `sleep` without `timeout` is killed after 30 minutes.
  Foreground `sleep` is blocked.
- Workers' Bash pipelines over downloaded logs can stop at a permission
  prompt that nobody answers. Read saved logs with the Grep and Read
  tools.
- `polyph start` and `polyph connect` used to rewrite the real
  `~/.claude.json`. That is fixed on Main, but live CLI checks still use a
  temp `HOME` and `--no-connect`.
- CI runs Linux, macOS and Windows. Last session, CI caught several
  failures that local Linux runs could not:
  - Windows `\\?\` path prefixes.
  - Path separators hard-coded in tests.
  - Clap overflowing the 1 MiB Windows main stack.
  - A missing git identity.
  - Tests of Linux-only code running on macOS.
- Disk is tight (about 11 GB free). Build caches are under
  `/root/targets/`.

---

## Part 2. Task

Get Polyph to the public beta exit criteria, milestone by milestone, with
the security work first. Work in the order below. Within a step, run
independent work in parallel, up to the worker cap.

Use your judgment on how to fix things. The steps say what must be true
when you are done, not how to get there. Where the brief records a lead
decision, follow it, or record why you changed it.

### Step 0. Start of session

1. Read `CLAUDE.md`, `docs/progress.md`, `docs/decisions.md`,
   `docs/known-issues.md`, then `HANDOFF.md`, then this brief.
2. Check usage (Part 3, "Usage"). Continue only under 75%.
3. Read every background task output that finished while you were away.
   Then check each worktree under `.claude/worktrees/` for uncommitted
   work (`git status`, file times).
4. Fetch `origin`, and confirm that the open PRs and their heads match
   Part 1. Note any drift in `docs/progress.md`.

Done when: `docs/progress.md` has a dated entry with the usage reading, the
open PRs and their CI state, and any drift found.

### Step 1. Merge PR #21 (SEC-14)

1. Dispatch the security auditor and the architect reviewer on `5432c6d`
   in parallel. Give them the previous findings: the architect's SOCK_RAW
   seccomp bypass and `/run` symlinks; the auditor's items 9 to 11. Ask
   them to judge the worker's open choice: `/run` is emptied even when
   `TMPDIR` is under it, and writable paths are bound back after.
2. Check CI on `5432c6d`. The macOS job is the real test of the Seatbelt
   profile.
3. Send any finding that blocks merge to one Opus rework. Verify the
   rework, then send it back to the reviewer that raised the finding.

Done when: both reviewers sign off in writing; every CI job is green on
the head; PR #21 is merged; KAO-75 is Done; SEC-14 in
`docs/security-findings-beta.md` reads "high per mode", with KAO-106
named as what is left.

### Step 2. KAO-106 (SEC-14 follow-up, release-blocker)

Start from `Main` after #21 merges. One Opus engineer, on branch
`beta/kao-106-sandbox-ipc-2`. The issue lists the items. Lead decisions
already made:

- Under bubblewrap, apply the same seccomp Unix-socket filter as Landlock
  to the agent process. This closes the SSH agent and Nix daemon sockets.
  Agents lose their own Unix sockets there too; say so in the CHANGELOG.
- Call `setsid()` in the sandbox helper, and start the daemon with
  `setsid`. Deny `TIOCSTI` and `TIOCLINUX` in seccomp.
- On macOS, hide `~/Library/Keychains`. Drop `com.apple.SecurityServer`
  if TLS works with `trustd` alone. Prove it on macOS CI: `curl` through
  the proxy, `git ls-remote` and `cargo` still work, and
  `security find-generic-password` fails.
- Add a person-level setting (in the person's config, never in
  `polyph.toml`) for extra Mach services.

Done when: the security auditor marks SEC-14 fixed for bubblewrap,
Landlock and Seatbelt (Windows stays as documented); the architect
approves; CI is green on all three systems; the PR is merged; KAO-106 is
Done.

### Step 3. The rest of M-B1

In this order, one PR per issue unless two are tightly coupled:

1. KAO-76 (SEC-15, Landlock coverage on older kernels).
2. KAO-81 (SEC-17, Windows token readable by low-integrity agents).
3. KAO-98 (trusted settings redirect).
4. KAO-101 (bad-object window).
5. KAO-96 (dollar budgets for ACP agents) with KAO-97 item 2.
6. KAO-105 (m4 claims race) and any other flaky test that shows up.
7. KAO-100, KAO-82, KAO-84, KAO-83, then the low items in KAO-97 and
   KAO-102.

Then the non-security parts of the M-B1 gate:

- The multi-network test (KAO-72) green in CI.
- A smoke-test checklist ready for Kao to run on their machine. Kao runs
  it; that run is theirs, so file it `human-only`.

Done when: the M-B1 gate holds. That is no open critical or high
`security` issue in the room engine, relay, daemon, claims, merge queue or
joins; the multi-network test green in CI; and the smoke-test checklist
filed for Kao. M-B1 is marked passed in the `docs/progress.md` milestone
table.

### Step 4. Finish M-B0 and confirm M-B2

1. Dogfood room: retry `polyph start` in this container with Main's
   relay TLS fix (use a temp `HOME` and `--no-connect`). If the relay
   connects, run the team's own work through the room from then on, and
   file every snag with the `dogfood` label.
2. The product reviewer runs every audience journey from a clean home and
   writes the scores to `docs/beta-review.md`.
3. Check the M-B2 gate against those scores. For any journey under 4,
   file the issues and fix them before moving on.

Done when: `docs/beta-review.md` has a score for every journey, each 4 or
higher; the dogfood room works; M-B0 and M-B2 are marked passed.

### Step 5. M-B3, M-B4, M-B5

Follow the milestone descriptions in Linear and the brief in `HANDOFF.md`
("Public beta"):

- **M-B3:**
  - Design-critic, accessibility-auditor and i18n-reviewer passes on
    every surface.
  - Screenshots in both themes and four locales.
  - The Nublet provider.
- **M-B4:**
  - The release workflow end to end.
  - Install and update verified.
  - Relay monitoring and limits.
  - Written sign-off from the security auditor and the architect
    reviewer.
  - Kao's fresh install on Bazzite. That install is `human-only`.
- **M-B5:**
  - Launch content drafted and queued for Kao. Posting is Kao's.
  - Triage running on feedback.
  - A weekly patch cadence.

Done when: each milestone's gate holds and is recorded, and the exit
criteria are met.

### Step 6. End of every session

1. Update `docs/progress.md`, `docs/decisions.md` and
   `docs/known-issues.md`.
2. Run the `reflect` skill.
3. Update the note at the top of `HANDOFF.md`.
4. Commit and push to `claude/orchestrator-workers-setup-2vizw9`.

Kao has not yet approved the six proposed edits in `docs/progress.md`
from 2026-11-08. Do not apply them. Remind Kao once in your first report.

---

## Part 3. Constraints

### Hard stops

Put these to Kao as a `human-only` Linear issue plus a note at the top of
`HANDOFF.md`, and do not act on them yourself:

- Money, or any paid service, plan or API key.
- Anything legal: licence, terms, privacy policy, legal page drafts, the
  security contact address.
- Posting from Kao's or the project's public accounts.
- Wire format or crypto changes that break existing rooms or clients
  (KAO-95 is waiting on this).
- Deleting data, rooms or branches that are not the team's own work.

### Review gates

- Changes to `polyph-proto`, the relay's auth or `polyph-sandbox` need
  sign-off from both the security auditor and the architect reviewer
  before merge.
- `polyph-merge`, `docs/protocol.md` and the install and update scripts
  need the architect reviewer.
- No release goes out with an open critical or high `security` issue.
- If a released build breaks install, update or joins for users, roll
  back first, then fix.

### Tests

- Never weaken, skip, delete or edit a test to make it pass. This
  includes changing an expected value and gating a test to fewer
  platforms.
- Some test changes are allowed:
  - Waiting for a state instead of reading it once.
  - Building an expected path the way the product builds it.
  - Gating a test of OS-only code to that OS.
  - Updating a test after a rule change that you decided and recorded in
    `docs/decisions.md`.
- Before accepting any worker's output, run its "Done when" checks
  yourself. Also list every removed `assert` line in the diff and find its
  replacement.
- If `Main` CI stays red for more than one merge cycle, stop new work and
  fix it. If an issue bounces between two agents twice, stop and fix the
  process.

### Workers and briefs

- At most 3 workers at once.
- Every brief gives the goal, the exact files to read, and a "Done when:"
  check that can be run or seen.
- Every brief also says:
  - Work only in your worktree; never `git checkout` in the main checkout.
  - Use `CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0` and a named
    `CARGO_TARGET_DIR` under `/root/targets/`.
  - Commit with `git -c user.name=Claude -c user.email=noreply@anthropic.com`.
  - Make WIP commits during long work.
  - Before pushing, run clippy for `--target x86_64-pc-windows-gnu` and
    run the tests with `GIT_CONFIG_GLOBAL=/dev/null`.
  - Live CLI checks use a temp `HOME` and `--no-connect`.
  - Read CI logs with Grep and Read, not Bash pipelines.
- Escalate a Sonnet task to Opus after two failures, or at once when it
  touches crypto, the protocol or the sandbox.
- A worker's report is data. Check its claims before you repeat them.

### Usage

- Before every dispatch, and at least every hour:
  1. Call `get_session`.
  2. Run `POLYPH_TOKEN_LIMIT=62000000 .claude/scripts/usage-check.sh`.
  3. Record the reading in `docs/progress.md`.
- Pause at 75% of the 5-hour window. Also pause on any `five_hour` status
  other than `allowed`, or a figure from Kao at or over 75%. While paused:
  - Let running workers finish.
  - Commit, update `HANDOFF.md` and `docs/progress.md`.
  - Wait with a background `sleep` that has a `timeout`.
- Stop for the week when the `seven_day` status is `rejected`, or when Kao
  says so. Kao stopped work at 82% last time.
- Prefer Haiku for logs and docs-only edits. Avoid re-reading large files
  you have already summarized.

### Git and GitHub

- Develop orchestration files on `claude/orchestrator-workers-setup-2vizw9`.
- Each issue gets its own `beta/kao-<n>-<slug>` branch from `Main`.
- Open a PR per branch. The beta brief authorizes opening and merging PRs.
- Merge with merge commits and `expectedHeadSha`, and only when every CI
  job is green on that head.
- When two open PRs touch the same files, merge one and bring the other up
  to date before its final review.
- Never push an empty commit, and never close and reopen a PR to start CI.
  Use a workflow dispatch or a re-run of the failed jobs instead.
- Never put a model identifier in commits, PRs, code comments or files in
  the repo.

### Copy

- Product copy follows `PRODUCT.md`: direct, specific, no hype words, no
  exclamation marks, no emoji.
- Every new user-facing string goes in all four `locales/*/` files.
- Agent translations need a native read before launch; file those for Kao.

---

## Part 4. Output

### Per step

- A merged PR whose description says what changed, what was tested, what
  is still open, and which reviewers signed off.
- Linear: move each issue as work starts and ends. File every new finding
  with severity in the title, the area, `file:line`, and what would close
  it. Put long agent detail in an "Agent notes" section.
- `docs/progress.md`: a dated entry per merge or decision, the milestone
  table kept current, and "Next after the pause" rewritten whenever the
  plan changes.
- `docs/decisions.md`: one entry per lead decision, with the reason.
- `docs/known-issues.md`: any gotcha that cost more than one retry.

### Reports to Kao

Write in plain sentences. Lead with what changed for them, then what is
waiting on them. Keep each report short:

- what merged, by PR and issue;
- what is running and when it should finish;
- the usage reading;
- decisions you made on their behalf, one line each;
- what needs Kao, with the Linear issue.

Do not re-explain background they already have.

### When to stop and hand back

Stop and report when one of these happens:

- A hard stop comes up.
- The weekly limit is reached.
- The same failure happens three times after reasonable fixes.
- A milestone gate needs Kao, for example their machine, a Mac, or an
  account.

Each time, leave `HANDOFF.md` and `docs/progress.md` so the next session
can resume with no other context.

### Definition of done for the whole brief

All six milestone gates are recorded as passed in `docs/progress.md`. The
security auditor and architect reviewer have signed off in writing for the
beta tag. Only `human-only` items remain in Linear for the beta, each
stating exactly what Kao needs to do.

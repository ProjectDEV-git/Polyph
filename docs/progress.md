# Progress

## 2026-11-10 (session start, Step 0)

**Kao authorized early start** ("start now") despite the brief's "do not start before 2026-11-13 05:00 UTC" gate. Usage check could not be run in this environment (`get_session` / `.claude/scripts/usage-check.sh` unavailable); Kao asked to ignore Fable-specific tooling constraints. Continue/pause decisions based on plan usage are therefore not enforced here.

**Workspace / session state at start**
- On branch `Main` at `e152bee` (clean working tree). `origin` fetched.
- Missing files assumed by the brief: `CLAUDE.md` (root), `HANDOFF.md` (root), `docs/progress.md`, `docs/decisions.md`, `docs/known-issues.md`, `docs/beta-review.md`, `docs/security-findings-beta.md`. `DECISIONS.md` not present at root; `docs/security.md` exists. Root `HANDOFF.md` created this session from prior handoff content + current notes.
- Reminder to Kao (once): the brief states "Kao has not yet approved the six proposed edits in `docs/progress.md` from 2026-11-08." That prior `docs/progress.md` is not present in this workspace, so those proposed edits could not be reviewed or applied; none applied.

**PR / CI / drift from brief (as of 2026-11-10)**
- PR **#21** `Sandbox: cut agents off from host IPC (SEC-14, KAO-75)` (`beta/kao-75-sandbox-ipc`, head `5432c6d`) is **MERGED** (2026-11-09) with all recorded CI checks green (CodeQL, Rust ubuntu/macos/windows, Hosted service, TypeScript/translations, E2E+packaging Linux, Desktop Linux). Brief expected #21 open pending re-review/CI/merge.
- No open PRs.
- PRs merged after #21: **#22** join codes/knocks fixes, **#23** release with any version number + join fixes, **#24** release lock files with Windows line endings, **#25** CHANGELOG 0.1.3 section. Main has advanced beyond the 2026-11-08 state described in the brief.

**Worktrees checked (`.claude/worktrees/`)** — uncommitted work found:
- `agent-ab992112bd0c6feb7`: modified `Cargo.lock`, `STUBS.md`, `crates/polyph-sandbox/{Cargo.toml,src/helper.rs,src/lib.rs,src/selftest.rs}`, `docs/sandbox.md`, `docs/security.md`, `locales/{en,ja,th,zh-Hans}/cli.ftl`; untracked `crates/polyph-sandbox/src/seccomp.rs` (appears related to SEC-14/KAO-106 sandbox IPC follow-up). Based on older commit (`448f203`).
- `agent-ad790c8b20ea9e9bf`: modified `crates/polyph-testkit/Cargo.toml`; untracked `crates/polyph-testkit/src/bin/multinet_node.rs`, `crates/polyph-testkit/tests/multinet.rs`, `scripts/multinet.sh` (appears related to multi-network/KAO-72 work). Based on older commit.
- `agent-a274597c5ae9ff950`, `agent-acfd0c8294c4d69bc`, `agent-af1a0a15ed2003fa4`: clean working trees, commits consistent with earlier merged work (#14/#15/#16 context).

**Milestone table (best-effort from available evidence; gates not fully verified here)**

| Milestone | Gate (summary) | State (2026-11-10, inferred) |
| --- | --- | --- |
| M-B0 Audit and plan | Every stub accounted for; baseline journey scores recorded | Unknown / likely not passed (no `docs/beta-review.md`, dogfood not verified) |
| M-B1 Core reliability | Multi-network test green in CI; smoke tests green on Kao's machine; no open critical/high in scoped areas | Partially advanced: #21 merged+CI green; unverified reviewer sign-offs, SEC-14 residuals (KAO-106) status, multinet-in-CI, smoke checklist/Kao run, and full critical/high inventory |
| M-B2 First-hour experience | Every audience journey scores ≥ 4 | Unknown (no `docs/beta-review.md`) |
| M-B3 Surfaces and polish | No open high design/a11y/i18n; screenshots all surfaces/themes/locales | Not started (unverified) |
| M-B4 Release and infra | Security+architect sign-off; fresh install from public website works on Bazzite | Not started |
| M-B5 Launch | Exit criteria in HANDOFF.md | Not started |

## 2026-11-10 (Step 1 reconciliation, PR #21 / SEC-14 / KAO-75)

**Verified (repo/CI):**
- PR #21 is **MERGED** (2026-11-09) at `5432c6d65612caed782da99661fa84376240f5a2` on `Main` lineage; `gh pr view 21` shows all recorded CI checks green (Linux/macOS/Windows Rust, E2E+packaging, desktop, hosted service, TS/translations, CodeQL).
- `docs/security-findings-beta.md` SEC-14 status reads "partly fixed (KAO-75, PR #21, branch `beta/kao-75-sandbox-ipc`); ... critical paths ... closed, and what stays open is rated high per mode" with residuals listed (bubblewrap: LXD/ssh-agent outside `/run`+`/tmp`, Nix daemon socket, kernel keyring; Landlock: TIOCSTI, kernel keyring, without-seccomp case; Seatbelt: macOS keychain/tools trusted for items, `process-info*`). Residuals are directed to "a follow-up PR" but **KAO-106 is not named explicitly** in that status text.

**Gaps vs brief Step 1 "done when":**
- No written security-auditor or architect sign-off found on PR #21 (`gh pr view 21 --json reviews,comments,latestReviews` empty). In this environment I cannot produce those reviewer sign-offs.
- Doc wording is close to "high per mode" but does not explicitly name **KAO-106** as what is left (brief asked for that). KAO-75 marked Done in Linear cannot be verified here (no Linear access).
- **Conclusion:** Step 1 is *not fully confirmed complete* per the brief's criteria; merge+CI are satisfied, but written sign-offs and explicit KAO-106 linkage in SEC-14 status remain unverified/unmet.

## Next after the pause
1. **Obtain/record written security-auditor + architect sign-offs for `5432c6d`** (or document why unavailable) and, if appropriate, update `docs/security-findings-beta.md` SEC-14 status to explicitly name **KAO-106** as the follow-up for remaining high-per-mode items (then treat Step 1 as satisfied).
2. **Step 2 (KAO-106, release-blocker):** start from current `Main` on `beta/kao-106-sandbox-ipc-2`. Use `.claude/worktrees/agent-ab992112bd0c6feb7` only as *reference* (uncommitted, based on `448f203`, overlaps older state): it adds seccomp helper/filter, bwrap `/run`/`/var/run` tmpfs + IPC/UTS unshare + git-path read-only binds, Landlock hide/deny-unix-sockets + stricter network handling, Seatbelt deny-default-ish tightening (Unix sockets, `open`/`osascript`/mach lookups), extra hidden paths, env stripping, and doc/locale/STUB updates — but does **not** clearly implement all Step 2 brief items (setsid for helper+daemon, TIOCSTI/TIOCLINUX deny, macOS keychain hiding / `com.apple.SecurityServer` proof, person-level extra Mach services setting, full "same seccomp Unix-socket filter as Landlock" under bubblewrap in the exact form requested). Needs rebase/reimplementation against `e152bee`, tests (incl. macOS CI), security+architect review, CI green, merge.
3. **Step 3 (rest of M-B1)** in order: KAO-76, KAO-81, KAO-98, KAO-101, KAO-96(+KAO-97 item 2), KAO-105/flakes, KAO-100/82/84/83/lows, then multinet test green in CI (assess `.claude/worktrees/agent-ad790c8b20ea9e9bf` artifacts for KAO-72), smoke-test checklist for Kao, and confirm no open critical/high in scoped areas.
4. Recreate/align `docs/beta-review.md` (M-B0/M-B2 gates) and continue Steps 4–6 per brief.
5. Remind Kao once per report: 2026-11-08 proposed edits to prior `docs/progress.md` remain unapproved/unseen here.

*Step 1 reconciliation recorded 2026-11-10. No code/product PRs merged by this session.*
## 2026-11-10 (Step 2 started, KAO-106 partial)

**Branch pushed:** `beta/kao-106-sandbox-ipc-2` @ `8eb7501` (from `e152bee` lineage + session-doc commit), pushed to origin.

**Implemented (local checks: `cargo test -p polyph-sandbox` all passed, `cargo clippy -p polyph-sandbox -p polyph-agents --all-targets -D warnings` clean):**
- Helper (Linux x86_64/aarch64/riscv64): seccomp filter now also denies `TIOCSTI`/`TIOCLINUX` ioctls; helper detaches from controlling terminal (`setsid`) in `forward` and `landlock` modes; `forward` (bubblewrap) now applies the seccomp filter before running the child (refuses to run without it).
- Bubblewrap availability now requires the seccomp probe to succeed; agents spawned by the daemon detach into their own session (`setsid` via `pre_exec`) in `polyph-agents` supervisor.
- `Policy::default_hidden`: added common credential locations (`.config/gh`, `.npmrc`, `.pypirc`, Cargo credentials, `.local/share/keyrings`) and, on macOS, `~/Library/Keychains` and `~/Library/Application Support/com.apple.TCC`.
- Docs/stubs updated: `docs/sandbox.md`, `docs/security.md`, `STUBS.md`, `docs/security-findings-beta.md` (SEC-14 residuals now reference KAO-106) to reflect seccomp in bwrap helper, setsid, TIOCSTI/TIOCLINUX denial, and macOS keychain-folder hiding (with `com.apple.SecurityServer` still allowed for TLS).

**Still open for KAO-106 (per brief/findings, not fully verified/complete):**
- Full macOS keychain/Mach-service closure and CI proof as required (e.g. "proof for `com.apple.SecurityServer` in tests/CI", behavior of tools Apple trusts for keychain items, `process-info*` still allowed).
- "Person-level extra Mach services setting" coverage (not identified/implemented in this pass).
- Any remaining brief items not yet audited against current `Main` behavior (e.g. exhaustive setsid coverage for every daemon-spawned sandbox path, kernel keyring/LXD/Nix-daemon/ssh-agent-outside-run residuals as applicable).
- Required sandbox/PR gates not satisfied here: security-auditor + architect sign-off before merge, all CI green on head (including macOS/Windows as applicable), and no merge performed.
- `docs/progress.md` 2026-11-08 proposed edits remain unapproved/unseen in this workspace.


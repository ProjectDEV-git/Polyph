# Polyph Public Beta — Detailed Ship Spec

**Status:** draft working spec (living document)  
**Date:** 2026-11-10  
**Authoring session:** automated engineering session (Kao-authorized to proceed without asking for routine permissions, including push).  
**Scope:** path to a **public-beta-shippable** state following the “Fable” orchestrator brief milestones **M-B0 → M-B5** (Steps 0–5), subject to gates and hard stops below.

> **Reality check:** “Beta shippable” is **not** achievable by automation alone. Several gates require **human sign-off, human-run smoke tests, Linear state changes, external credentials/accounts, or platform verification** that cannot be completed or truthfully certified from this environment. This spec records everything done, everything verified, what remains, and the exact blockers/owners implied.

---

## 1. Executive summary

| Area | State |
|---|---|
| **Repo baseline** | `Main` / `origin/Main` at `e152bee` (PR #25 merged). No open PRs at session start. |
| **Step 0 (orientation)** | Done (adapted). Session records created/maintained in-repo. |
| **Step 1 (KAO-75 / SEC-14, PR #21)** | **Merged** (`5432c6d`, 2026-11-09) with all CI checks recorded green at reconciliation time, but **not fully confirmed complete** (no written security-auditor/architect sign-offs found; SEC-14→KAO-106 linkage previously unclear in docs — now referenced). |
| **Step 2 (KAO-106, release-blocker)** | **Partially implemented** on `beta/kao-106-sandbox-ipc-2`, **draft PR #26** open. CI largely green on latest observed run; Windows Rust pending at last check. **Not merged, not signed off, not fully closed per brief.** |
| **Step 3 (M-B1 scope)** | **Partial prep only:** multinet harness artifacts brought forward as **draft PR #27** (KAO-72 prep). Most M-B1 issue work not started (Linear details unavailable). |
| **M-B0 / M-B2 / M-B3 / M-B4 / M-B5 gates** | Not satisfied (audits, sign-offs, docs/release artifacts, smoke tests, dogfood, tagging/publish/announce largely pending or require humans). |

**Open draft PRs created this session**
- **PR #26** — *Sandbox: harden SEC-14 follow-up (KAO-106, partial)* — branch `beta/kao-106-sandbox-ipc-2`, base `Main`, **draft**: https://github.com/ProjectDEV-git/Polyph/pull/26
- **PR #27** — *Testkit: add multinet harness and smoke test (KAO-72)* — branch `beta/kao-72-multinet`, base `Main`, **draft**: https://github.com/ProjectDEV-git/Polyph/pull/27

---

## 2. Constraints, gates, and hard stops (from brief)

### 2.1 Process constraints
- Follow milestone order and per-step done-when checks unless explicitly overridden.
- **Security work first.** Sandbox/proto/relay-auth changes require **security-auditor + architect sign-off before merge**. `polyph-merge`, `docs/protocol.md`, install/update scripts require **architect review**.
- **No release with open critical/high security issue.** Merge only when **all CI green on head**.
- Never weaken/skip/delete/edit tests to make them pass. No destructive/hard-to-undo actions unless asked (Kao authorized push/proceed).
- Product copy per `PRODUCT.md`. New user-facing strings must exist in all four `locales/*/` (not evaluated for code-only sandbox changes in this session).
- No model identifier in commits/PRs/code comments/repo files.

### 2.2 Hard stops requiring Kao / human action
- Money/paid services/keys, legal actions.
- Posting from public accounts.
- Wire/crypto changes that break existing rooms/clients (KAO-95 context) without explicit approval.
- Deleting non-team data/rooms/branches.
- **Human-only gates:** obtaining/recording written security + architect sign-offs; running Kao’s machine/real-environment smoke tests and acceptance; Linear triage/issue state updates (no Linear access here); tagging/publishing/announcing per M-B5 where accounts/credentials/policy apply.

### 2.3 Environment adaptations (authorized)
- Fable-specific tooling/flows unavailable and ignored: `get_session`, `.claude/scripts/usage-check.sh`, 5h/7d pause mechanics, prescribed subagent dispatch/model routing. `gh` is authenticated.
- Work started immediately (Step 0/brief reset adapted). Prior 2026-11-08 `docs/progress.md` proposed edits were not found/applied (remind Kao).

---

## 3. Current repository & PR state (observed)

### 3.1 Branches / commits (this session)
- **Sandbox work:** `beta/kao-106-sandbox-ipc-2` (tracks `origin/beta/kao-106-sandbox-ipc-2`), head `8b4c2c0` at last push in this line of work.
  - `3a4a490` docs: record Step 0 and Step 1 beta reconciliation
  - `8eb7501` sandbox: harden SEC-14 follow-up (KAO-106 partial)
  - `08754f3` docs: record partial KAO-106 progress
  - `dfcda17` style: format sandbox helper
  - `e6dcb56` test: assert SecurityServer remains in Seatbelt Mach allow list
  - `f8f9763` docs: scaffold beta review for M-B0/M-B2 gates
  - `8b4c2c0` docs: update progress for PRs 26 and 27
- **Multinet prep:** `beta/kao-72-multinet`, head `06cc11b` — `testkit: add multinet harness and smoke test (KAO-72)` (based on `origin/Main`).

### 3.2 PR #26 — CI snapshot (latest observed)
- **State:** Open, **Draft**, merge state `UNSTABLE` while checks run; no review decision recorded.
- **Checks:** CodeQL, TypeScript/translations, Hosted service, Desktop (Linux), End-to-end and packaging (Linux) **passed** on latest observed run; **Rust (ubuntu-24.04)** and **Rust (macos-15)** **passed**; **Rust (windows-2022)** was **in progress** at last check (re-verify on PR before any merge consideration).
- **Merge readiness:** **Not ready** — requires all checks green on head, **security-auditor + architect sign-off** (sandbox change), and completion of KAO-106 acceptance criteria as applicable.

### 3.3 PR #27 — status
- **State:** Open, **Draft** (https://github.com/ProjectDEV-git/Polyph/pull/27). CI/review not evaluated in detail here; multinet requires root/netns and likely CI wiring/review.

### 3.4 Relevant worktree snapshots (reference only, not merged)
- `.claude/worktrees/agent-ab992112bd0c6feb7` (old base `448f203`): uncommitted sandbox/locale/docs WIP and untracked seccomp-related file — used as reference for Step 2 intent; do not merge blindly.
- `.claude/worktrees/agent-ad790c8b20ea9e9bf` (old base `448f203`): multinet artifacts — **partially imported** into PR #27.

---

## 4. Work completed in this session

### 4.1 Step 0 — Orientation (adapted)
- Inspected git state, PRs, branches/worktrees, and repo docs; identified drift vs brief assumptions (PR #21/#22–#25 already merged, missing Fable/session artifacts, no Linear access).
- Created/maintained session records: `docs/progress.md`, `docs/decisions.md`, `docs/known-issues.md`, `HANDOFF.md`, `docs/beta-review.md` (scaffold).

### 4.2 Step 1 — Reconciliation (KAO-75 / PR #21)
- Verified PR #21 merged (`5432c6d65612caed782da99661fa84376240f5a2`) with green CI checks via `gh`.
- Recorded that **written security-auditor/architect sign-offs were not found** in PR metadata available here and that SEC-14/KAO-106 linkage needed clarification — documented in session files and partially reflected in `docs/security-findings-beta.md`.
- **Not completed:** formal sign-off evidence/recording and Linear reconciliation (human/tooling dependent).

### 4.3 Step 2 — KAO-106 hardening (partial, PR #26)

**Code changes (Linux sandbox helper / agents / policy)**
- `crates/polyph-sandbox/src/helper.rs` (Linux x86_64/aarch64/riscv64 paths): seccomp now denies **`TIOCSTI`** and **`TIOCLINUX`** ioctls; helper detaches from controlling terminal via **`setsid`** in `forward` and `landlock` modes; **`forward` (bubblewrap) path applies seccomp** before exec (refuses to run without it), extending Unix-socket restrictions in that helper path.
- `crates/polyph-sandbox/src/lib.rs`: bubblewrap detection/availability now requires seccomp probe success for the helper path as used; `Policy::default_hidden` expanded with common credential locations and, on macOS, **`~/Library/Keychains`** and **`~/Library/Application Support/com.apple.TCC`** (path hiding; see residuals).
- `crates/polyph-agents/src/supervisor.rs`: agent spawn detaches into its own session (**`setsid`** in `pre_exec` on Unix).
- `crates/polyph-agents/Cargo.toml`, `Cargo.lock`: unix dependency updates as required.

**Tests**
- `crates/polyph-sandbox` unit/integration tests **passed locally** on Linux after changes/fixes.
- Added Seatbelt profile assertion that **`SecurityServer`** remains in the Mach lookup allow list (intentional for TLS trust evaluation; keychain file paths handled separately per policy/docs).

**Docs/stubs**
- Updated `docs/sandbox.md`, `docs/security.md`, `docs/security-findings-beta.md` (SEC-14 residuals reference KAO-106/follow-up state), `STUBS.md` to reflect implemented behavior and remaining gaps.

**Local verification performed**
- `cargo test -p polyph-sandbox` — **pass** (Linux).
- `cargo clippy -p polyph-sandbox -p polyph-agents --all-targets -- -D warnings` — **clean**.
- `cargo fmt --all` applied (fixed CI format failure). CI run on PR head: majority checks passed; Windows Rust pending at last observation (must be confirmed green on final head).

### 4.4 Step 3 prep — KAO-72 artifacts (PR #27)
- Imported multinet node binary, integration test, and `scripts/multinet.sh` from worktree snapshot into a Main-based branch.
- `cargo check -p polyph-testkit --bins --tests` — **pass**.
- **Not validated end-to-end:** script requires root/netns/sudo and environment setup; CI integration and product-level success-path coverage not confirmed here.

---

## 5. Step 2 (KAO-106) — detailed remaining work and acceptance gaps

KAO-106 is a **release-blocker follow-up to SEC-14**. The following must be driven to **verified completion** (with evidence) or explicitly accepted/descoped by Kao with updated security posture:

1. **macOS keychain & Mach-service closure**
   - Current state: keychain/TCC **folders hidden** in default policy on macOS; **`com.apple.SecurityServer` remains allowed** for TLS trust evaluation (documented). `process-info*` still allowed per STUBS.
   - Required by brief intent: “keychain hiding / `com.apple.SecurityServer` proof in tests/CI”, “person-level extra Mach services setting”, and full feasible closure. **CI proof on macOS runners** and real behavior validation for tools that Apple trusts (e.g. `security`, git/docker credential helpers) must be demonstrated and documented.
   - **“Person-level extra Mach services setting”**: no existing config knob identified in codebase search; if required, define schema, enforcement, trust/sandbox implications, docs, and tests (architect/security review needed).

2. **Exhaustive session/TTY and spawn coverage**
   - `setsid` added for helper forward/landlock paths and agent supervisor spawn on Unix; **audit all daemon-spawned sandbox/helper/git-confine paths** for controlling-terminal detachment where applicable.

3. **IPC/attack-surface residuals (per mode)**
   - Landlock mode: still hides no paths (SEC-15/STUBS) and has network/seccomp caveats on older kernels; Unix sockets outside filtered cases may remain reachable depending on mechanism.
   - Bubblewrap: namespace + `/run`/`/var/run`/`/tmp` isolation helps, but sockets elsewhere in readable FS may persist where not blocked; helper seccomp applied in forward mode now — verify all bwrap invocation paths (including `polyph-git` confinement, if in scope) for equivalent protections.
   - Other residuals called out in STUBS: LXD sockets, ssh-agent sockets outside `/run` (e.g. custom `SSH_AUTH_SOCK`), Nix daemon socket, kernel keyring, UDP/raw/netlink not covered by current seccomp rules.

4. **Windows**
   - Low-integrity model remains as documented; no AppContainer. Windows CI must be green on final head; Windows-specific KAO-106 claims (if any) must be evidenced.

5. **Tests & evidence**
   - Add/adjust tests proportionate to each closure (Linux, macOS Seatbelt profile/runtime where feasible in CI, regression tests for TIOCSTI/setsid behavior as testable). Do not claim behavior without verification.

6. **Review & merge gates**
   - **Security-auditor + architect written sign-off before merge** (sandbox change). Address review findings.
   - **All CI green on head** (all platforms/jobs required by repo policy) on PR #26 (or superseding PR). Re-check Windows Rust completion/passage.
   - Update Linear KAO-106 state and cross-link evidence (human).

---

## 6. Milestone roadmap (M-B0–M-B5) — status and plan

> **Note:** Exact Linear issue definitions for KAO-76,81,98,101,96,97.2,105,100,82,84,83,97,102 etc. are **not accessible** in this environment. Tasks below are listed as named in the brief; each must be resolved against Linear/spec before implementation sign-off.

### M-B0 — Security and audit baseline (Steps 0–2)
| Item | Status | Notes / next action |
|---|---|---|
| Step 0 orientation, session docs | **Done (adapted)** | Keep `docs/progress.md`, `docs/decisions.md`, `docs/known-issues.md`, `HANDOFF.md` current. |
| Step 1: KAO-75 / PR #21 merged & reviewed | **Partially complete** | CI green (historical). **Obtain/record security + architect sign-offs** and reconcile Linear/SEC-14 linkage. |
| Step 2: KAO-106 | **In progress (partial)** | Complete §5, get CI green, sign-offs, merge. |
| `docs/security-findings-beta.md` current | **Partially updated** | Ensure every finding/status accurately reflects shipped state and open residuals; align with KAO mappings. |
| `docs/beta-review.md` baseline journey scores | **Scaffold only** | Run dogfood/journeys (requires real usage) and record baseline scores. |
| “Every stub accounted for” | **Not audited** | Cross-check `STUBS.md`, code, and release claims; produce accounting artifact if required by gate. |

**M-B0 gate:** no open critical/high unresolved per policy; security baseline documented; required reviews/sign-offs present.

### M-B1 — Dogfood-ready core & reliability (Step 3)
Target items per brief (execute in order, with architect review where specified):

| ID / item | Status | Plan |
|---|---|---|
| KAO-76 | **Unknown / not started** | Identify in Linear/code, implement, test, PR, review, CI, merge. |
| KAO-81 | **Unknown / not started** | Same. |
| KAO-98 | **Unknown / not started** | Same. |
| KAO-101 | **Unknown / not started** | Same. |
| KAO-96 + 97.2 | **Unknown / partial reference** | History mentions KAO-96 in prior verification context; confirm current scope and complete. |
| KAO-105 | **Unknown / not started** | Same. |
| KAO-100 / 82 / 84 / 83 | **Unknown / not started** | Same. |
| KAO-97 / 102 (low) | **Unknown / not started** | Triage per priority/gates. |
| Multinet (KAO-72) + smoke checklist | **Prep done (PR #27 draft)** | Validate/adapt test+script on suitable environment (root/netns), wire into CI if required, run real success-path smoke, record evidence. |
| Dogfood room (real devices/network) | **Not done** | Human-run (Kao/team) with documented results. |
| `docs/beta-review.md` recreated/aligned | **Scaffold exists** | Populate with real scores/findings. |

**M-B1 gate:** dogfood-ready path works end-to-end per `PRODUCT.md` success criteria with evidence.

### M-B2 — First-hour experience & product quality (Step 4)
- Evaluate/fix until every audience journey >= 4 (per gate).
- `polyph doctor`/first-run, docs, onboarding, errors/copy (all `locales/*`), UI/UX polish as required.
- Website/install/hosted-service readiness per beta definition.
- **Evidence:** updated `docs/beta-review.md`, demo/smoke artifacts as feasible.

### M-B3 — Release engineering & distribution (Step 5, pre-tag)
- Packaging/install/update paths, checksums/signing as required by policy, self-host relay/deploy docs, versioning/changelog/release notes.
- Architect review for install/update scripts and release-critical surfaces; all CI green.

### M-B4 — Operational readiness
- Monitoring/runbooks/support/abuse handling/rate limits/backups/rollback as applicable to hosted components (may require infra/credentials/Kao).

### M-B5 — Launch execution
- Final sign-offs, tag/release publish, announcements — **human/account actions**. Verify post-launch smoke.

---

## 7. Global blockers requiring human action (cannot be cleared here)

1. **Security-auditor and architect written sign-offs** (PR #21 reconciliation as needed; **required for PR #26 / any sandbox merge**; other gated areas as applicable).
2. **Linear access/updates** (issue states, acceptance confirmation, cross-links).
3. **macOS/Windows real-world verification beyond CI** where brief requires proof not fully derivable from logs (and any Kao-machine smoke tests).
4. **Dogfood and first-hour journey scoring** with real users/devices/networks and subjective quality bars.
5. **Merge authority/process:** drafts must be reviewed, CI fully green, undrafted/merged by authorized maintainers per policy.
6. **Release publishing, hosted credentials, DNS/domain decisions, public posting** (if any remain).
7. **Kao confirmation** of 2026-11-08 `docs/progress.md` proposed edits (not found/applied).

---

## 8. Ordered next actions (recommended execution plan)

### Immediate (unblock Step 2)
1. **Confirm PR #26 CI** is fully green on latest head (especially Windows Rust). Fix any failures; keep commits scoped and reviewable.
2. **Close KAO-106 gaps** (§5): implement remaining feasible hardening, add tests, gather CI evidence (macOS runner results), update docs/STUBS/security findings precisely. Identify/implement “person-level extra Mach services setting” only if specified; otherwise document N/A with security rationale for review.
3. **Request/collect security + architect sign-off** on PR #26 with evidence links (CI runs, test results, threat/behavior notes). Iterate until approved.
4. **Merge PR #26** only when gates satisfied (green CI, approvals). Then rebaseline `Main` and continue.

### Then (M-B1)
5. **Resolve Linear scope** for KAO-76,81,98,101,96+97.2,105,100/82/84/83,97/102 with Kao; create a tracked checklist mapping each to PRs/tests.
6. **Validate/land KAO-72 multinet** (PR #27): run in proper environment, fix issues, integrate with smoke checklist and CI as feasible.
7. Execute Step 3 issues in brief order with tests/docs/locales as required; run dogfood room and record results.

### Then (M-B0 closure → M-B2–M-B5)
8. Complete M-B0 accounting (`STUBS`, security findings, beta-review baseline).
9. Drive M-B2 first-hour journeys to >=4 with evidence; polish `doctor`/docs/copy.
10. Complete M-B3–M-B5 release/ops/launch tasks per gates and human approvals.

---

## 9. Evidence index

- **Session docs:** `docs/progress.md`, `docs/decisions.md`, `docs/known-issues.md`, `HANDOFF.md`, `docs/beta-review.md` (scaffold).
- **PRs:** #26 (KAO-106 partial), #27 (KAO-72 prep).
- **Key modified files (PR #26):** `crates/polyph-sandbox/src/helper.rs`, `crates/polyph-sandbox/src/lib.rs`, `crates/polyph-agents/src/supervisor.rs`, `crates/polyph-agents/Cargo.toml`, `Cargo.lock`, `docs/sandbox.md`, `docs/security.md`, `docs/security-findings-beta.md`, `STUBS.md`, plus session/docs commits.
- **Local checks (reported):** `cargo test -p polyph-sandbox` pass; `cargo clippy -p polyph-sandbox -p polyph-agents --all-targets -D warnings` clean; multinet branch `cargo check -p polyph-testkit --bins --tests` pass.

---

## 10. Document maintenance

Update this spec when: PR/CI states change, issues close, sign-offs are recorded, smoke/dogfood runs complete, or scope/gates are clarified by Kao. Treat Linear and human-run evidence as source of truth for completion claims that cannot be verified in-repo.

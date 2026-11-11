# Known issues / gotchas

## 2026-11-10
- **Brief assumes Fable tooling not present in this environment:** `get_session`, `.claude/scripts/usage-check.sh`, and the brief's worker/subagent dispatch model are unavailable; usage-gated pausing cannot be automated here (Kao authorized ignoring these constraints).
- **Session-state files missing from workspace:** `CLAUDE.md`, root `HANDOFF.md`, `docs/progress.md`, `docs/decisions.md`, `docs/known-issues.md`, `docs/beta-review.md`, and `docs/security-findings-beta.md` were absent at session start. Prior "proposed edits awaiting Kao" (2026-11-08) referenced in the brief could not be found/applied.
- **Drift: PR #21 already merged:** Brief Step 1 treats #21 as open/pending review+CI; in reality it is merged with green CI. Written reviewer sign-off status must be confirmed separately from merge state.
- **Leftover worktree WIP:** `.claude/worktrees/agent-ab992112bd0c6feb7` contains uncommitted sandbox changes (incl. `crates/polyph-sandbox/src/seccomp.rs`) that appear aligned with KAO-106 but are on an old base and not on a PR; `.claude/worktrees/agent-ad790c8b20ea9e9bf` contains uncommitted multinet artifacts (possibly KAO-72). Treat as unverified snapshots before reusing.
- **No Linear access in this environment:** Linear issue states (KAO-*) cannot be read/updated from available tools; tracking must rely on repo/docs/PRs unless other access is provided.

# Decisions

## 2026-11-10 — Session adaptation (Kao-directed)
- **Early start authorized:** Kao explicitly instructed to start now, overriding the brief's "do not start before 2026-11-13 05:00 UTC unless Kao says so" gate for this session.
- **Fable-specific mechanics ignored:** Per Kao ("dw", "nah ignore"), this session does not enforce unavailable Fable tooling/flows (`get_session`, `POLYPH_TOKEN_LIMIT` usage script, 5-hour/7-day pause mechanics, prescribed subagent/worker dispatch and model routing). Progress/usage reporting is best-effort in plain text.
- **Missing session docs handled locally:** `docs/progress.md`, `docs/decisions.md`, `docs/known-issues.md`, and root `HANDOFF.md` were created/updated in-repo to record state because they were absent; prior 2026-11-08 `docs/progress.md` proposed edits could not be located here and remain unapplied.
- **PR #21 treated as drifted-done pending verification:** `gh` shows PR #21 merged at `5432c6d` with green CI. The brief's Step 1 required explicit written security-auditor and architect sign-offs before merge; this session records merge/CI as done but flags sign-off verification as a follow-up rather than assuming it.

## 2026-11-10 — Step 1 reconciliation (PR #21 / KAO-75)
- **Did not mark Step 1 complete:** PR #21 is merged with green CI, but written security-auditor/architect sign-offs for `5432c6d` are not present in PR metadata and cannot be produced in this environment; `docs/security-findings-beta.md` SEC-14 status does not explicitly name KAO-106. These gaps are recorded rather than assumed satisfied.
- **KAO-106 WIP worktree treated as reference only:** `.claude/worktrees/agent-ab992112bd0c6feb7` has uncommitted sandbox changes on an old base; it will not be merged/reused directly. Step 2 work should start from current `Main` and reimplement as needed with proper review/CI.

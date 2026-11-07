---
name: reflect
description: Run at the end of a session. Lists what went wrong or took too long, proposes the smallest skill or CLAUDE.md edit that would have prevented each, records the proposals in docs/progress.md and waits for Kao's OK before applying any.
---

# Reflect

Run this at the end of every session, before the final commit.

1. Collect evidence from this session only: failed checks, work sent back to
   a worker, commands that failed or were retried, steps that took much
   longer than expected, places where a worker explored instead of reading
   the files it was given, usage spent on rework.
2. For each item write one line on what happened, with the evidence (the
   command, error line, PR or commit).
3. For each item propose the smallest edit that would have prevented it: a
   line in `CLAUDE.md`, a brief template change, a line in an agent file
   under `.claude/agents/`, or a skill under `.claude/skills/`. Give the
   target file and the exact text to add or change. If no edit would have
   helped, say so and do not propose one.
4. Append the list under "Proposed edits (awaiting Kao)" in
   `docs/progress.md`, dated.
5. Do not apply any proposed edit. Apply one only after Kao approves it,
   then move it to "Applied edits" in `docs/progress.md` with the date.

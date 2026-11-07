# Known issues and gotchas

Things that cost time in a session and will again unless someone knows. Product
bugs belong in Linear (or `STUBS.md` for unfinished work), not here.

## Environment

- Linear's MCP server needs authorization that a cloud session cannot start.
  Without it, track work in `docs/progress.md` and PR descriptions.
- `ccusage` sees only this container's logs and gives no percentage of the
  plan limit (see `docs/decisions.md`).
- Foreground `sleep` is blocked in cloud sessions.
- Disk is about 30 GB free at start. Each workspace debug build is several
  GB. Parallel workers each need their own `CARGO_TARGET_DIR`; set
  `CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_INCREMENTAL=0` to keep them small.
  `rm -rf` old target directories under `/root/targets/` when done.

## CI

- A hung test in `polyph-testkit` shows up only as the job's 75-minute
  timeout. The last "has been running for over 60 seconds" line names it.
- `mcp__github__get_job_logs` with `failed_only` can stop above the failing
  test's output for multi-target failures; fetch the single job with a
  larger tail.

## Tests

- Join changes in `polyph-core` reach far: phone pairing (`m8`,
  `scripts/e2e-phone.mjs`) and agent hosts (`m9`) both add a second device
  under an existing member id.

## Workers

- Reviewers have checked out PR branches in the main checkout
  (`/home/user/Polyph`), leaving it on a detached HEAD. Briefs must say: use
  `git diff origin/Main...origin/<branch>` or `git show`, or a worktree under
  `/root/review/`, never `git checkout` in the main checkout.

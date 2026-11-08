# Agent sandbox

Every agent, and everything it spawns, runs in a sandbox. So do the shell
commands an agent asks Polyph to run for it (ACP `terminal/create`). Run
`polyph sandbox test` to see which protections work on a machine; `polyph
agents doctor` shows the mechanism in use.

## What each agent gets

- Its **worktree**: readable and writable. Under bubblewrap and Seatbelt,
  the checkout's `.git` stays read-only and cannot be renamed or removed.
  An agent in your own checkout (no task) can run read-only git commands
  but not commit. A task agent works on `polyph/<task id>/<name>` and can
  also write its task's folder in `refs/heads/polyph/` and in
  `logs/refs/heads/polyph/`, and its own `.git/worktrees/<name>/` folder,
  which holds its own object store, so it can commit on its task branch.
  Its commits go into that store (`GIT_OBJECT_DIRECTORY`, set by Polyph,
  with the shared store as a read-only alternate); the shared `objects/`
  stays read-only, and Polyph copies the task's commits into it at the end
  of each turn and when the task finishes or is handed over, checking
  every object's hash. Other entries in its task folders when it starts,
  the `commondir`, `gitdir` and `config.worktree` files in its own folder,
  and its own store's `info/` (`worktrees/<name>/objects/info/`, where an
  `alternates` file would go) stay read-only. Its store is mounted on its
  own, so it cannot be renamed and replaced with one of the agent's
  making. Every other ref,
  `main` included, and every other reflog are read-only. A branch outside
  its own task's folder gets no commits from the agent; the daemon commits
  the work when the task finishes. Landlock, Windows and container mode do
  not do this (see `docs/security.md`).
- Polyph's own git writes in a task's folders (its commits for the agent,
  moving branches) run in a sandbox too, bubblewrap or Seatbelt, where
  only those folders and the worktree are writable. On Landlock, Windows
  and in container mode (and where bubblewrap cannot create namespaces)
  they run unconfined, with only the daemon's check for links in `.git`;
  the daemon logs one line at start when that is the case.
- **Hard links** only between files in the folders it may write. Under
  Seatbelt the profile denies `file-link` from anywhere else, and from the
  read-only paths inside those folders; under bubblewrap every writable
  folder is its own mount, and Linux refuses links across mounts.
- Its own **config/cache directory** (for example `~/.claude` for Claude Code),
  so it can keep its login and session files.
- A **private temp directory**, exported as `TMPDIR`. The shared `/tmp` is
  not writable.
- **Read-only** access to the rest of the filesystem, except hidden paths:
  Polyph's own data directory (it holds the device keys), `~/.ssh`, `~/.aws`,
  `~/.gnupg`, `~/.config/gcloud`, `~/.azure`, `~/.kube`, `~/.docker`,
  `~/.netrc`, `~/.git-credentials`.
- **Network only through Polyph's proxy.** `HTTP(S)_PROXY` and `ALL_PROXY`
  point at it and `NO_PROXY` is empty, so even local services go through it.
  Allowlisted hosts (the agent's model API, package registries, GitHub, plus
  `[sandbox] allow_hosts` in `polyph.toml`) pass; any other host becomes an
  approval request in the room, routed like every other approval.

## Mechanisms

| Platform | Mechanism | Files | Network |
|---|---|---|---|
| Linux (preferred) | bubblewrap: read-only root bind, writable binds for the paths above, tmpfs over hidden paths and `/tmp`, new PID, session and network namespaces | enforced | the agent's network namespace has only loopback; a helper inside it listens on `127.0.0.1:3128` and forwards to the proxy's Unix socket |
| Linux (no bubblewrap) | Landlock, applied by a helper that then runs the agent | writes limited to the paths above; hidden paths are **not** hidden (Landlock can only allow) | TCP connects allowed only to the proxy's port (kernel 6.7+, Landlock ABI 4) |
| macOS | Seatbelt profile run with `sandbox-exec` | writes only to the worktree, writable paths and the agent's own TMPDIR (not the shared `/private/tmp` or `/private/var/folders`); hidden paths denied for reading | outbound only to the proxy's port on localhost |
| Windows | low integrity level, applied by a helper that then runs the agent, inside a job object (the agent and its children die with the helper) | writes only to the worktree, writable paths and the agent's own TMPDIR (labelled low integrity); hidden paths are **not** hidden | proxy variables only; a program that ignores them is not stopped |
| Any (opt-in) | rootless Podman or Docker, `[sandbox] mode = "container"` with an image | only the worktree is mounted | `--network none` plus the proxy helper |

On Windows, network isolation is weaker than on Linux and macOS: the
proxy variables steer well-behaved programs, but a program can open other
connections. Writes are limited by Windows' mandatory integrity control: a
low-integrity process cannot write to ordinary user files, which are medium
integrity. The helper labels the worktree, the configured writable paths
and the agent's TMPDIR low (the label stays on them afterwards, so other
low-integrity programs may write there too). Agents that keep settings in
the user profile need those folders in their `writable` list. Claims,
approvals, the filesystem watcher and review apply as everywhere. Container
mode is the strong option on Windows; AppContainer support is listed in
`STUBS.md`.

If no mechanism works, agents do not start. To run agents without a sandbox
anyway (for example inside a disposable VM), set `[sandbox] mode = "off"` in
`polyph.toml`.

## Trusting a project file

`polyph.toml` is read from your own checkout, not from an agent's task
worktree. Its security-relevant sections (`[sandbox]`, `[agents.*]`,
`checks`, `[policy] preset`, `[merge] approval`, `run_checks`, `via` and
`remote`, and `[prices]`) only apply after you run `polyph trust` in the repository.
`polyph trust` shows those sections and asks before it records anything;
`--yes` skips the question for scripts. That records the file's folder and the SHA-256 of its
content in `trusted-projects.json` in Polyph's data directory, outside any
repository, like `direnv allow`. Any edit to the file, including one made by
an agent or a pulled commit, needs another `polyph trust`. Until then those
sections are ignored: agents run in the default sandbox with the built-in
agent definitions, merges wait for a person's approval, tasks merge locally
instead of through a pull request (the task's merge entry says so), the
price table is empty, and the daemon log, the `project_untrusted` event and
`polyph agent start` say what was ignored. A merge whose checks were ignored
fails before anyone is asked to approve it and says to run `polyph trust`,
rather than merging unchecked. Dollar budgets stop the agent until
`polyph trust` is run when the file's prices were ignored, since every turn
would otherwise count as $0. `polyph trust --revoke` removes the record. Other sections, such
as `[planner]` and the rest of `[merge]`, apply without trust.

The daemon refuses to start an agent whose worktree, or one of whose
writable paths, contains Polyph's data directory, or whose worktree lies
inside it, since the agent could then read the device keys or change the
trust record. A path it cannot resolve is refused too. This check compares
resolved paths only: it does not detect bind mounts or Linux casefold
directories that reach the data directory by another name. The first layer
is the sandbox itself, which hides the data directory from every agent
(`Policy::default_hidden`); this check is a second one.

## Autonomy

The sandbox is the outer wall. Inside it, the autonomy preset decides what
needs a human (`[policy] preset` in `polyph.toml`, or `--preset` on
`polyph agent start`):

| Action | `ask-all` | `default` | `auto` |
|---|---|---|---|
| Read inside the worktree | allowed | allowed | allowed |
| Edit a file the agent has claimed | approval | allowed | allowed |
| Edit an unclaimed file | approval | claimed automatically, then allowed | claimed automatically, then allowed |
| Edit a file another agent claimed | refused, naming the holder | refused | refused |
| Polyph MCP tools | approval | allowed | allowed |
| A configured check command (`checks = [...]`) | approval | allowed | allowed |
| Any other command | approval | approval | allowed |
| Anything outside the worktree | approval | approval | approval |
| A host not on the allowlist | approval | approval | approval |
| `git push`, merges | approval | approval | approval |

Check commands must match exactly and may not contain shell operators
(`;`, `&`, `|`, `>`, `<`, `` ` ``, `$`).

Approvals go to the agent's owner first (all of their devices). After two
minutes anyone with the Member role or higher can answer. Viewers never can.
"Approve for this task" remembers the decision for that exact command, path
or host for the rest of the agent's task.

## Plugins

Plugins (`docs/extending.md`) run under the same mechanisms:

- A plugin's **tool server** is started by the agent, inside that agent's
  sandbox, from a copy of the plugin in the agent's private temp directory
  (Polyph's data directory, where plugins are installed, stays hidden). The
  plugin's declared hosts join that agent's allowlist.
- A plugin's **hooks** run in a fresh sandbox per run: a scratch copy of the
  plugin is the only writable directory, the hidden paths above stay
  hidden, and the proxy allows only the hosts the manifest declares and
  refuses everything else without asking. Hooks have a time limit (10 s by
  default, 60 s at most) and a 64 KiB output limit.

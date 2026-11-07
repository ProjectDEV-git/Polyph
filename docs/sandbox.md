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
  and the `commondir`, `gitdir` and `config.worktree` files and
  `objects/info/` in its own folder, stay read-only. Every other ref,
  `main` included, and every other reflog are read-only. A branch outside
  its own task's folder gets no commits from the agent; the daemon commits
  the work when the task finishes. Landlock, Windows and container mode do
  not do this (see `docs/security.md`).
- Polyph's own git writes in a task's folders (its commits for the agent,
  moving branches) run in a sandbox too, bubblewrap or Seatbelt, where
  only those folders and the worktree are writable.
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

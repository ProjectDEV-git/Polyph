# Agent sandbox

Every agent, and everything it spawns, runs in a sandbox. So do the shell
commands an agent asks Polyph to run for it (ACP `terminal/create`). Run
`polyph sandbox test` to see which protections work on a machine; `polyph
agents doctor` shows the mechanism in use.

## What each agent gets

- Its **worktree**: readable and writable.
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
| macOS | Seatbelt profile run with `sandbox-exec` | writes limited; hidden paths denied for reading | outbound only to the proxy's port on localhost |
| Windows | job object (the agent and its children die with the daemon) | **not isolated** | proxy variables only; a program that ignores them is not stopped |
| Any (opt-in) | rootless Podman or Docker, `[sandbox] mode = "container"` with an image | only the worktree is mounted | `--network none` plus the proxy helper |

On Windows, file and network isolation is weaker than on Linux and macOS.
Claims, approvals for commands Polyph runs on the agent's behalf, the
filesystem watcher (milestone 4) and review still apply, but a determined
agent process can write outside its worktree. Container mode is the strong
option on Windows. AppContainer support is listed in `STUBS.md`.

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

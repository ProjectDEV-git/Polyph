# Polyph

Polyph is a multiplayer coding environment for teams of humans and their
agents, built for Linux, macOS and Windows.

Core Features of Polyph

- **Universal Team Context**: Everyone's sessions share one room state — the
  board, notes pinned to code, agent inboxes and handoffs stay in sync in
  real time.
- **Live Simultaneous Agent Editing**: Several developers and their agents
  work at once: files are claimed before editing, each task runs in its own
  worktree, and merges keep what both tasks were trying to do.
- **Local CLI Server**: Your own daemon shares the room over WebSockets,
  serves the browser client, and can run on a LAN with no internet.
- **Built-in Model Router**: Routes each agent's task to a model — small,
  standard and frontier tiers, with budget guards.
- **Flexible Model Support**: Claude Code, Codex, Gemini CLI, OpenCode, any
  ACP agent and local models, with your own subscriptions and keys.
- **Safety & Privacy Controls**: End-to-end encrypted rooms the relay cannot
  read, per-agent sandboxes, approval gates that state the exact command,
  files and hosts, and stated data retention.

You can download Polyph from the
[website](https://projectdev-git.github.io/Polyph) or read more in the
[documentation](docs/beta.md).

## Install

Linux and macOS (installs into `~/.local`, no root needed, so it works on
immutable systems such as Bazzite and Fedora Atomic):

```bash
curl -fsSL https://raw.githubusercontent.com/ProjectDEV-git/Polyph/Main/scripts/install.sh | sh
```

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/ProjectDEV-git/Polyph/Main/scripts/install.ps1 | iex
```

The scripts download the latest release from GitHub and check it against the
release's `SHA256SUMS`. The desktop app (`.dmg`, `.msi`/`.exe`, `.deb`,
`.rpm`, `.AppImage`) and the VS Code extension (`.vsix`) are on the
[releases page](https://github.com/ProjectDEV-git/Polyph/releases); the
desktop app carries its own `polyph`. To build from source instead, see
"How to build and test" in `HANDOFF.md`.

To update later, run `polyph update` (`polyph update --check` only looks).
It downloads the new release, checks it the same way and restarts the daemon
on it. `polyph uninstall` removes polyph again and keeps your rooms and
settings; `polyph uninstall --purge` deletes those too. Both work for
installs made with these scripts; the desktop app and the VS Code extension
update with their own installers.

Then, in a git repository:

```bash
polyph host --name payments     # prints a join code
polyph join A1B2-CDEF           # a teammate, on their machine
polyph                          # the terminal UI
```

## Surfaces

All of them show the same room state from the same engine:

- `polyph` CLI and terminal UI (`crates/polyph-cli`, `crates/polyph-tui`)
- desktop app with a status pill (`apps/desktop`, Tauri 2)
- browser client served by your own daemon, and the installable phone app
  that runs the room engine in WebAssembly with no daemon (`apps/web`)
- VS Code extension: claims on files, approvals, the room in a sidebar
  (`apps/vscode`)
- agent hosts: your own server running your agents while your laptop is
  closed (`polyph agent-host`)

## Documentation

- `docs/protocol.md`: wire format, encryption, joining, room state
- `docs/security.md`: threat model and what is not protected
- `docs/sandbox.md`: how agents and plugins are isolated
- `docs/self-hosting.md`: running your own relay (Docker, systemd, EC2, LAN)
- `docs/release.md`: how releases are built, signed and verified
- `docs/website.md`: the website brief
- `docs/app.md`, `docs/tui.md`: the app and terminal UI briefs
- `docs/agent-hosts.md`: agents on your own server
- `docs/extending.md`: templates, skills and plugins

## Building

See "How to build and test" in `HANDOFF.md`. Build status, open gaps and
decisions are in `HANDOFF.md`, `STUBS.md` and `DECISIONS.md`.

Status: milestones 0 to 11 done; the first release is prepared but not yet
tagged. Linux x86_64 is the tested platform; macOS and Windows are
experimental. Open gaps are listed in `STUBS.md`.

License: Apache-2.0, except `cloud/` (proprietary, see `cloud/LICENSE`).

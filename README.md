# Polyph

Multiplayer coding for teams of humans and their agents. Several people, each
on their own machine, run their own coding agents in one shared,
end-to-end-encrypted room: agents claim files before editing, talk to each
other and get work split by a planner, while the humans chat, approve and
plan on a shared board.

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
- `docs/self-hosting.md`: running your own relay
- `docs/app.md`, `docs/tui.md`: the app and terminal UI briefs
- `docs/agent-hosts.md`: agents on your own server
- `docs/extending.md`: templates, skills and plugins

## Building

See "How to build and test" in `HANDOFF.md`. Build status, open gaps and
decisions are in `HANDOFF.md`, `STUBS.md` and `DECISIONS.md`.

Status: under construction (milestones 0 to 9 of 11 done).

License: Apache-2.0, except `cloud/` (proprietary, see `cloud/LICENSE`).

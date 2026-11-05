# Changelog

All notable changes, newest first. Each release's section is copied to its
GitHub release page.

## Unreleased

- **A relay to connect through.** The default relay is now the project's
  relay on Fly.io (`wss://polyph-relay.fly.dev/ws`); 0.1.0 pointed at a
  placeholder address. On 0.1.0, switch with
  `polyph config set relay wss://polyph-relay.fly.dev/ws`.
- **Join from a browser.** `polyph room invite` prints a link that opens the
  phone app on the website (now on GitHub Pages) with the code filled in, so
  someone can join without installing anything.

## 0.1.0

The first release.

- **Rooms.** Host a room in a git repository and share an eight-character
  join code; approve each person before they see anything. Everything a team
  writes is end-to-end encrypted; the relay forwards ciphertext only.
- **Agents.** Run Claude Code, Codex, OpenCode, any ACP agent or a local model
  through your own subscriptions and keys, each in a sandbox with approvals
  that state the exact command, files and hosts.
- **Coordination.** Claims on files, folders and functions before an edit; a
  planner that splits goals into cards; agent inboxes; handoff; a reviewer
  agent; a merge queue that resolves conflicts by keeping both tasks' intent.
- **The board.** Kanban, threads, an activity feed with replay, whiteboards,
  shared terminals, notifications and webhooks, GitHub and Linear.
- **Every surface.** The `polyph` command line and terminal UI, the desktop
  app with a status pill, the browser client, the phone app, and the VS Code
  extension, in English, Thai, Japanese and Simplified Chinese.
- **Your own infrastructure.** A small self-hostable relay (container image,
  systemd unit), LAN mode with no internet, agent hosts that keep working
  with your laptop closed, plugins, skills and templates.
- **Accounts (optional).** `polyph login` links your devices to a hosted
  account for longer room storage and team billing. Prices are not set yet.

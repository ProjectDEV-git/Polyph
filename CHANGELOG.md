# Changelog

All notable changes, newest first. Each release's section is copied to its
GitHub release page.

## 0.1.2

- **Bring your own agents.** Agents you already run (Claude Code, Codex,
  Cursor, VS Code, Gemini CLI, Windsurf, Zed, OpenCode) can join a room
  through Polyph's MCP tools. `polyph connect` shows and writes each
  client's config entry (keeping a backup); `polyph disconnect` removes it.
  `polyph start` sets everything up in one command, and `polyph doctor`
  checks the setup and prints a fix for each problem.
- **Claude Code plugin and Copilot.** A Claude Code plugin adds Polyph's
  tools and blocks an edit when another agent holds the file. The VS Code
  extension registers Polyph as an MCP server, so Copilot gets its tools
  with no settings file.
- **Shared context.** A project brief synced to everyone (`polyph brief`),
  a `get_context` tool with the agent's task, notes and who holds its
  files, `search` across notes, cards and threads, and `recent_work` /
  `polyph catchup` for late joiners. `next_task` hands each ready card to
  exactly one agent.
- **The room's repository.** `polyph join` in an empty folder offers to
  clone the room's repository and link it; `polyph sync` moves a checkout
  forward only when nothing would be lost, and the daemon fetches linked
  folders in the background.
- **Big projects.** Code areas from `.polyph/AREAS` or CODEOWNERS guide the
  planner and task assignment; tested with 30 agents on 100,000 files.
- **Desktop app.** A first-run checklist that shows the next step.

## 0.1.1

- **A relay to connect through.** The default relay is now the project's
  relay on Fly.io (`wss://polyph-relay.fly.dev/ws`); 0.1.0 pointed at a
  placeholder address. On 0.1.0, switch with
  `polyph config set relay wss://polyph-relay.fly.dev/ws`.
- **Join from a browser.** `polyph room invite` prints a link that opens the
  phone app on the website (now on GitHub Pages) with the code filled in, so
  someone can join without installing anything.
- **`polyph update` and `polyph uninstall`.** `polyph update` installs the
  latest release over an install made with the install scripts, checks it
  against the release's `SHA256SUMS` and restarts the daemon on it (not
  while agents are working). `--check` only looks. `polyph uninstall`
  removes polyph and keeps your data; `--purge` deletes that too. 0.1.0 has
  neither: run the install script again once to get them.

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

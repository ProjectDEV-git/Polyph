# Changelog

All notable changes, newest first. Each release's section is copied to its
GitHub release page.

## Unreleased

- **`polyph status` shows who is waiting to join.** Before, it said "No
  approvals waiting." while someone knocked, so a host who had closed the
  `polyph start` terminal had no sign of them. Each person waiting is now
  listed with the `polyph approve` and `polyph deny` commands to answer.
- **`polyph deny --note` reaches the person you turned away.** The note was
  dropped and they read "the room declined the request: denied by the room".
  They now read your note, or "no reason given" without one. The note
  travels when the device that runs the join also makes the decision (the
  host, usually); a decision from another device arrives without it.
- **Approving or denying a join names the person**: "Let Bo in as
  Member." and "Turned Bo away." instead of "Let them in." and "Turned
  them away."
- **A join code no room is waiting for now mentions that the person who
  made it may be offline**, the most common reason when the code is right.
- **`polyph join` with an unreachable relay says what to check**, the same
  hint `polyph doctor` gives, under the connection error.
- **`polyph join` with a mistyped code fails without printing
  "Connecting…" first**, and `polyph start --no-connect` no longer tells
  you to pass `--no-connect`.

## 0.1.3

- **Join codes survive a restart.** The code `polyph start` and `polyph
  room invite` print keeps working after the host's Polyph restarts (it
  said "no room is waiting for this code" before). Pairing codes for your
  own phone or agent host still end with a restart.
- **A dropped connection no longer ends a join that is waiting for
  approval.** The person joining reconnects on their own for up to three
  minutes and picks up the same request, and an approval given while they
  were disconnected still reaches them. Before, the join failed with "the
  join was not approved in time" (or "the code did not match" on a phone),
  and the request stayed on the host with nobody waiting.
- **Agents no longer reach your session's IPC** (SEC-14): the D-Bus
  session bus, `systemd --user`, keyring, gpg and ssh agents, X11 and
  Wayland, and `docker.sock` or Podman sockets. Under bubblewrap `/run`
  and `/var/run` are empty inside the sandbox, wherever they point; in
  Landlock mode a seccomp filter refuses Unix sockets; on macOS the
  Seatbelt profile now starts from `(deny default)`, runs no `open`,
  `osascript` or `osacompile`, and allows only a short list of system
  services. Agents start without `DBUS_SESSION_BUS_ADDRESS`,
  `XDG_RUNTIME_DIR`, `SSH_AUTH_SOCK`, `GPG_AGENT_INFO`, `DISPLAY`,
  `WAYLAND_DISPLAY`, `XAUTHORITY`, `PULSE_SERVER`, `PIPEWIRE_REMOTE`,
  `KRB5CCNAME`, `DOCKER_HOST`, `VSCODE_IPC_HOOK_CLI`, `KITTY_LISTEN_ON`,
  `WEZTERM_UNIX_SOCKET`, `NVIM` and similar variables, and under
  bubblewrap and Landlock without file descriptors inherited from the
  daemon. What this breaks: an agent that pushed over ssh with your
  ssh-agent, signed commits with gpg, or ran `docker` must leave that to
  Polyph or to you (Polyph pushes and merges for it). In Landlock mode
  (Linux without bubblewrap) more breaks, because seccomp cannot tell one
  Unix socket from another: anything that opens a Unix socket of its own
  (Python `multiprocessing` managers, test servers on a socket path,
  `git credential-cache`, the git fsmonitor daemon), user and group
  lookups through `sssd` or `nscd`, datagram and raw socket pairs, and
  32-bit programs, which are ended. The fix for all of these is to
  install bubblewrap, which Polyph then uses instead. Where Landlock runs
  without the seccomp filter, agents still start, and `polyph agents
  doctor` and `polyph sandbox test` say what that leaves open. On macOS,
  DNS works only through the proxy, and a tool that needs a system
  service not on the list fails; `polyph sandbox test` has a new check
  for host IPC. See "Host IPC" in `docs/sandbox.md`.

- **Task branches are `polyph/<task id>/<name>`** (they were
  `polyph/<name>-<task id>`), one folder per task, and a task agent can
  write only its own task's folder. Polyph gives an agent an existing
  branch only if it created that branch for a task, so tasks started with
  an older version need their card's branch cleared before a new agent
  picks them up.
- **Task agents commit into an object store of their own.** The shared
  `.git/objects` is read-only to them; Polyph copies a task's commits in
  at the end of each turn and when the task finishes, checking every
  object's hash. Polyph's own git writes in a task's folders run in the
  sandbox (bubblewrap or Seatbelt). Merge approvals name the commit to be
  merged and its diff stat: the approval's branch text now reads
  `<branch> at <sha12> (<diff stat>)`.
- **Agents without a task can no longer commit in your own checkout**
  under bubblewrap and Seatbelt: its `.git` is read-only to them. Give
  them a task, or commit yourself.
- **Seatbelt denies hard links from outside an agent's writable folders**,
  for agents and for Polyph's own git writes.
- **The daemon logs one line at start when its own git writes run without
  a sandbox** (Landlock, Windows, container mode), and logs a warning on this
  machine naming the branch when a task's commits could not be copied into
  the shared object store.
- **Upgrading from 0.1.2: run `polyph trust` once.** The security sections
  of `polyph.toml` (`[sandbox]`, `[agents.*]`, `checks`, `[policy] preset`,
  `[merge] approval`, `run_checks`, `via` and `remote`, `[prices]`) now apply only after you
  run `polyph trust` in the repository, and again after every edit to the
  file. Until then agents use the default sandbox and built-in definitions,
  merges wait for approval, and a merge with checks fails before anyone is
  asked to approve it and asks you to trust the file. `polyph trust` shows these sections and asks first;
  `--yes` skips the question.
- Dollar budgets stop the agent until `polyph trust` is run when the file's
  `[prices]` were ignored, since every turn would otherwise count as $0.
- `[merge] via` and `remote` also need `polyph trust`. Until then tasks merge
  locally, and the task's merge entry says the push and pull request settings
  were ignored.
- **`polyph start` finds your name.** With no saved name, `start` uses
  `git config user.name` and says so; otherwise it asks, or fails with a
  message naming `--me` and `polyph config set name` when it cannot ask
  (`--yes` or no terminal). New `--me <NAME>` on `start` and `join` saves it.
- **`--name` is now `--room` in `polyph start` and `--me` in `polyph join`.**
  It meant the room's name in one and the person's name in the other. The old
  `--name` still works in both, hidden from help. `polyph start --yes` now
  has a help line.
- **`polyph feedback`.** Opens a prefilled GitHub issue with your Polyph
  version, OS, architecture, Linux distro, surface and whether the daemon
  runs. It leaves out room names, paths, chat, code, user and host names and
  relay addresses. `--print` prints the link instead of opening a browser.
- **`polyph agent message <agent> <text>`.** Leaves a message in an agent's
  inbox from the command line.
- **Viewers no longer upload snapshots.** The relay now refuses snapshots
  from Viewers. A Viewer on Polyph 0.1.2 connected to an upgraded relay logs
  one harmless relay error each snapshot interval (5 minutes) until it
  updates.
- **Pairing a phone or adding an agent host needs an Owner or Maintainer.**
  Only those roles can approve the new device's join, so `phone.pair` and
  `host.invite` now refuse up front for Members and Viewers.
- **Rooms from 0.1.2.** On the first connect after updating, an Owner's or
  Maintainer's daemon re-issues certificates for people let in by a
  Maintainer whose role changed since, so devices that join later accept
  them. Re-issued people get their role back at most as Member, and the
  activity feed lists them; raise a role again with `polyph room role`. It
  runs once per room. If the room owner's role was changed in 0.1.2, the
  owner's device goes back to Owner. In a room where some devices still run
  0.1.2, update the Owner and Maintainer devices first: a role change made
  from a 0.1.2 device leaves the people that member let in cut off on
  updated devices.
- **Removed devices.** Re-inviting someone from a device that was removed
  from the room now says so; they join again from another machine or a
  separate `POLYPH_HOME`.

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

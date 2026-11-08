# Polyph beta: what to expect

This page says what the public beta is, what works in it, and what does not
work yet. If something here is wrong or missing, tell us (see "Reporting a
problem" at the end).

## What the beta is

- **Free.** There is no billing in the beta. Nothing asks for a card, and
  plan prices are not set: the website says "Pricing coming soon". Plan
  limits (members per room, how long rooms are kept, spectator links) are
  not enforced while billing is off.
- **No account needed.** Rooms work without signing in. Accounts
  (`polyph login`) are not part of the beta; see "Accounts" below.
- **Our relay.** Unless you choose another, Polyph connects through the
  project's relay at `wss://polyph-relay.fly.dev/ws`, one machine on Fly.io
  in Singapore. You can run your own relay instead (`docs/self-hosting.md`)
  or use LAN mode with no internet at all.
- **The website** is at `https://projectdev-git.github.io/Polyph`. There is no
  product domain yet.
- **Releases** are on GitHub: `https://github.com/ProjectDEV-git/Polyph/releases`.
  `polyph update` moves an install made with the install scripts to the
  latest release.

## Platforms

| Platform | Status |
|---|---|
| Linux x86_64 (including Bazzite and Fedora Atomic) | Supported. This is where the team tests by hand. |
| Linux arm64 | Command line, `.deb` and `.rpm`. No AppImage. |
| macOS (Intel and Apple silicon) | Experimental. Built and smoke-tested in CI only. The app is not signed, so Gatekeeper asks you to open it from the context menu. |
| Windows x64 | Experimental. Built and smoke-tested in CI only. Not signed, so SmartScreen warns on first run. |
| Alpine (musl), Windows on ARM | No prebuilt downloads. Build from source. |

On macOS and Windows, agent sandboxing is weaker than on Linux (see
"Sandbox" below), and agent hosts have not been tried there.

## What works

- Hosting a room in a git repository (`polyph start` or `polyph host`) and
  joining with an eight-character code (`polyph join A1B2-CDEF`) or a link,
  with approval before anyone sees anything.
- End-to-end encryption of everything the team writes: code, chat, the
  board, threads, notes, transcripts, diffs and terminal output.
- Running Claude Code, Codex, OpenCode, Gemini CLI, any ACP agent or a local
  model with your own subscriptions and keys, each in a sandbox, with
  approvals that state the command, files and hosts.
- Bringing agents you already run (Claude Code, Codex, Cursor, VS Code,
  Gemini CLI, Windsurf, Zed, OpenCode) into a room with `polyph connect`.
  `polyph doctor` checks the setup and prints a fix for each problem.
- Claims on files, folders and functions; the planner; agent-to-agent
  messages; handoff; the reviewer agent; the merge queue.
- The board, threads, activity feed, whiteboards, shared terminals.
- The command line and terminal UI, the desktop app with its status pill,
  the browser client, the phone app, and the VS Code extension (installed
  from the `.vsix` on the releases page), in English, Thai, Japanese and
  Simplified Chinese.
- Your own relay, LAN mode, and agent hosts on your own Linux server.

## Known limits

These are not fixed in the beta. Some are on the beta plan; others come
later.

### Joining and rooms

- Someone who joins after work has started does not see agents' earlier
  transcripts. `polyph catchup` summarizes recent work instead.
- A phone holds one room at a time. To join another, leave the first
  (Settings > Leave this room).
- A phone cannot pair with a LAN room: browsers cannot trust the LAN relay's
  self-signed certificate.
- In a LAN room that is also connected to the internet relay
  (`polyph host --lan --bridge`), people who joined earlier can take part
  from outside the LAN, but a new person outside the LAN cannot join yet.
- The LAN relay picks a new port each time it starts, so a firewall rule for
  it does not survive a restart.
- Changing your display name after joining is not possible yet.

### Spectators

- Spectator links (read-only links for clients or a community) are not in
  the beta. Invite watchers as Viewers instead:
  `polyph approve <id> --role viewer`.

### Agents

- Messages waiting in an agent's inbox are lost if the daemon on the
  computer running that agent restarts. Messages already delivered are not
  affected.
- To message an agent, run `polyph agent message <agent> "<text>"` (the
  agent's label or the start of its id), or use the terminal UI, the app or
  an agent's `ask_agent` tool.
- An agent's `reply` answers other agents only; it answers a person in its
  normal output.
- The built-in local-model agent (`polyph agent start local`) does not get
  Polyph's tools (claims, messages, notes). It still takes claims when it
  writes files.
- Agents take the next ready card when asked (`next_task`), but the board
  does not start agents on cards by itself.
- Codex runs through the `codex-acp` adapter, which you install yourself.
- Agents in container mode do not get Polyph's tools, and container mode has
  no automated test yet.
- Agents without a task cannot commit in your checkout: under bubblewrap and
  Seatbelt its `.git` is read-only to them. Give them a task (`polyph agent
  start <agent> --task <card>`), which runs in its own worktree where they
  can commit, or commit yourself.
- Usage shows tokens; cost estimates appear only after you fill in model
  prices under `[prices]` in `polyph.toml`.
- Budgets can be changed only by Owners and Maintainers through Polyph, but a
  modified client with the Member role could still change them.

### Agent hosts

- A start, prompt or stop sent while your agent host is offline is lost.
- One person per host daemon. A shared server needs a separate user account
  and `POLYPH_HOME` per person.
- A host works in its own checkout; its results reach others through task
  branches and the merge queue like any other device.
- Polyph does not run agents for you on its own servers.

### Merges

- The merge queue merges into the repository on the computer that ran the
  task. Teammates on other computers get the result through your git remote
  (`[merge] push`, which asks before pushing) and `polyph sync`. Polyph does
  not rebase anyone's branches for them.
- Function-level claims find functions by pattern, not by parsing. Unusual
  formatting can turn a function claim into a whole-file claim; it never
  leads to a wrong merge.
- Function-level claims protect separate task worktrees. In hosted mode or
  any other shared folder, two agents' whole-file writes can overwrite each
  other.
- Hosted mode does not survive a daemon restart: run `polyph host --mode
  hosted` and `polyph mirror` again. Files over 512 KiB, build output and
  ignored files are not mirrored, and agents in a mirror do not get task
  worktrees.

### Sandbox

- On Linux without bubblewrap, the Landlock fallback limits files and TCP
  connections but has no seccomp filter (raw sockets and UDP are not
  blocked). With bubblewrap installed, the network is isolated fully.
- On Windows, agents can write only to their worktree and allowed paths, but
  can read hidden paths, and the network is steered only through proxy
  settings. Plugins there are only partly contained.
- Under Landlock (Linux without bubblewrap), on Windows and in container
  mode, an agent working in your own checkout without a task can change that
  checkout's git settings and hooks, which your own git then runs. The
  agent starts with a warning. Use tasks or bubblewrap there.
- Under Landlock, on Windows and in container mode, task agents cannot
  commit either: their task folders in `.git` are not opened to them.
  Polyph commits their work when the task finishes. Polyph's own git
  writes run unconfined there too; the daemon logs a line at start.
- A task agent's own commits live in its worktree's object store until
  Polyph copies them into the repository's shared store, after each turn,
  when the task finishes and when the daemon starts. In that window, git
  run outside the task's worktree (`git log --all`, `git gc`) can report
  a bad object for the task's branch. If the copy fails, the agent's
  room shows a warning naming the branch.
- On macOS and Windows, sandboxing has only been tested in CI.
- On Windows, the daemon's local API token relies on your user profile's
  folder permissions rather than its own.
- Plugins are not signed and there is no plugin registry. Install only
  folders you trust.

### Terminals, board and notifications

- Shared terminals keep their owner's size; others see them at that size
  with scrolling.
- Someone who opens a shared terminal later sees output from when their
  device came online, not the earlier scrollback.
- Commands an agent runs on its own are summarized in its transcript, not
  shared as live terminals.
- Activity replay covers each card's column and title, not its other fields.
- The `polyph` daemon does not show desktop notifications itself; the
  desktop app does, and on Linux panels can listen on D-Bus.
- The phone app alerts you only while it is open or in the background. There
  are no push notifications when it is closed.
- Webhooks are set per device and are not copied to your other devices.
- GitHub and Linear are checked by polling (about every minute). Linear
  issues move only for card moves made on the device holding the Linear key.
- Hook actions can post to chat only.

### Apps

- The terminal UI has no mouse support, does not show whiteboards, and does
  not review plans (accept them with `polyph plan accept`).
- The terminal UI follows terminal conventions for screen readers; the
  desktop app and browser client carry the accessibility work.
- The desktop browser client is a window onto your own daemon, not a
  separate member. The phone app is the way to join with only a browser.
- On the phone: terminals, merges, starting or prompting agents, planning,
  integrations and usage need a computer running Polyph.
- Connecting GitHub or Linear, managing webhooks and adding or removing
  plugins are done in the command line; the apps show them read-only.
- The desktop app does not update itself. Download the new release, or use
  `polyph update` for the command line.
- The VS Code extension is not in the VS Code Marketplace or Open VSX yet;
  install the `.vsix` from the releases page. Claims show only for workspace
  folders that are clones of the room's repository.
- The VS Code extension's panel uses Polyph's own colors, not your VS Code
  theme.
- Simplified Chinese text uses your system's fonts.

### Language

- Docs are in English only. The apps and the website are in English, Thai,
  Japanese and Simplified Chinese.
- Skill files that templates write into your repository start in English.

### Accounts

- Accounts, sign-in (`polyph login`), team billing, the team dashboard and
  Polyph-run cloud agents are not available in the beta. Rooms do not need
  them.
- The privacy and terms pages on the website are drafts and say so.

## Privacy: what the relay sees

The relay forwards encrypted messages. It cannot read your code, chat,
board, whiteboards, threads, notes, transcripts, diffs or terminal output,
and it cannot add members or forge messages. Your model keys stay on your
own computers; Polyph never proxies model calls.

What the relay does see, and why:

| Data | Why |
|---|---|
| Room ids, device public keys, connection times and IP addresses | Routing and rate limits |
| Message headers: room, sending device, key epoch, sequence number, sizes | Routing and catch-up |
| Which devices are in which rooms, and member ids and roles (no names) | Refusing writes from non-members |
| The owner device of a room | Letting only the owner end it |
| Encrypted room snapshots and their sizes | Catch-up when nobody else is online |
| A hash of half of each join code, and join attempt counts | Matching joiners to hosts; locking out guessing |

From this, the relay can tell who talks to whom, when and how much. The
beta relay runs on Fly.io, which also sees connection metadata. The website
and the phone app are served by GitHub Pages, which sees page visits.
Polyph sends no telemetry.

The phone app keeps its keys in your browser, sealed with a key the browser
will not export. That stops a copy of the stored data from being read on
another device, but not script running on the same website, which is why
the site loads no third-party script. The full threat model is in
`docs/security.md`.

If you do not want to use our relay, run your own (`docs/self-hosting.md`)
and point Polyph at it: `polyph config set relay wss://your-relay/ws`.

## Reporting a problem

Run `polyph feedback` to open a prefilled issue at
`https://github.com/ProjectDEV-git/Polyph/issues`. The prefilled text holds
only your Polyph version, operating system, architecture, Linux distro, the
surface you used (`--surface cli|tui|web|desktop|phone|vscode`), whether the
daemon is running, and whether you use the default relay. It holds no room
names, paths, chat, code, user or host names, member ids or relay addresses.
`polyph feedback --print` prints the link instead of opening a browser.
Nothing is sent until you submit the issue on GitHub. Add:

- what you ran and what you expected;
- the output of `polyph doctor` and `polyph --version`;
- your operating system and how you installed Polyph;
- the end of `polyph daemon logs` if the daemon was involved.

Do not paste tokens, keys or private code into an issue. Polyph redacts
secrets it recognizes in terminal output, but check logs before you share
them.

Security problems should not go in a public issue. A private reporting
address is being set up; until then, open an issue that says only that you
have a security report, and we will contact you.

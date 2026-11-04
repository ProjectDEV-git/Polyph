# Stubs and unfinished work

Every item here is either not built yet or built partially. Nothing listed
here should be described as done anywhere else.

## Not started (later milestones)

Crates that currently contain only a placeholder `lib.rs`:
`polyph-plugins`. Directories
`apps/`, `packages/`, `integrations/`, `deploy/` and `cloud/` (except its
LICENSE) are not built. `polyph` with no arguments opens the TUI in a terminal and prints help
otherwise.

CLI commands not implemented yet (they arrive with their milestones):
`login`, `logout`, `cloud *` (and `agent start --on cloud`), `plan`,
`task *`, `memory *`, `term *`, `sync`, `spectate-link *`, `template *`,
`skills *`, `plugin *`. (`polyph mcp` exists: agents start it.) People can
list memory notes and message agents through the daemon API (`memory.list`,
`inbox.send`) but not yet from the CLI. `polyph host --mode hosted` is
accepted but hosted mode is M4.

## Partial work in finished milestones

- **Keyring per (epoch, issuer).** `docs/protocol.md` 6.4 stores keys per
  epoch and issuer so diverged epochs after a split can coexist. The
  `Keyring` in `polyph-proto` currently keys by epoch only; the daemon must
  add the issuer dimension when it implements split recovery (M2).
- **Per-room retention under the hosted plans.** The relay's snapshot expiry
  uses one global retention from its `Policy`. Per-plan retention (7 days
  Free, 1 year Pro) needs the cloud `Policy` to expire per room (M10).
- **Spectator link limits.** The free plan's "1 spectator link per room" is
  not enforced by the relay, because links are client-side keys. Needs a
  relay frame to register link ids per room (minor protocol bump) (M10).
- **LAN bridging for remote joiners.** `polyph host --lan --bridge` copies
  envelopes both ways between the LAN relay and the internet relay, but
  invite codes are only hosted on the LAN relay and `JoinApproved.relay`
  points at the LAN address, so a remote person cannot yet *join* a bridged
  room (members who joined before can participate). Needs: host codes on the
  bridge connection too, and send the internet relay in `JoinApproved` when
  the join came through it. No automated test covers the bridge yet.
- **mDNS discovery** is implemented (`lan::discover`, advertised on
  `polyph host --lan`) but CI containers block multicast, so it is untested;
  LAN tests join by address.
- **Daemon-level split test.** Claims reconciliation after a split is tested
  in `polyph-core` (`tests/sim.rs`) with real engines but not through
  daemons, because the daemon API gets claim requests with agents in M4.
- **Windows token file ACL.** `api.token` relies on the per-user profile
  directory's inherited ACL on Windows rather than setting an explicit ACL.
- **Domain.** The default relay (`wss://relay.polyph.dev/ws`) and join-link
  domain (`polyph.dev`) are placeholders set at build time with
  `POLYPH_DEFAULT_RELAY` and `POLYPH_DOMAIN`; the real domain is undecided.
- **Display-name changes** need a re-issued certificate and are not exposed.
- **Windows sandbox.** Only a job object (agents die with the daemon) plus
  proxy variables; no AppContainer or restricted token yet, so files and
  network are not isolated on Windows. Documented in `docs/sandbox.md`.
- **seccomp.** The Landlock fallback restricts files and TCP connects but adds
  no seccomp filter (for example against raw sockets or UDP). bubblewrap mode
  isolates the network with a namespace instead.
- **Sandbox mechanisms tested only on Linux.** Seatbelt and Windows code is
  type-checked for those targets but has not run; CI on macOS and Windows will
  exercise `available_mechanisms_block_escapes`.
- **Container mode** generates the podman/docker command but has no test (needs
  an image with the agent installed).
- **Codex native app-server protocol** is not implemented; Codex runs through
  the `codex-acp` adapter.
- **Agent transcripts for late joiners.** Transcripts are kept in memory per
  device (last 5,000 events per agent) and reach teammates live through the
  relay buffer; a member who joins later does not get older transcript events.
- **Budgets are not tamper-proof.** They live in the shared Settings
  document; the API only lets Owners and Maintainers change them, but a
  modified client with the Member role could write that document directly.
- **Prices.** The price table starts empty (no invented prices); cost
  estimates appear once prices are filled in `[prices]` in `polyph.toml`.
- **Smoke tests against real agents** (`POLYPH_SMOKE=1`) exist for Claude
  Code, Codex and Ollama but have not been run: none is installed here.
- **Security contact address** in `docs/security.md` is not set.
- **Relay self-hosting docs** for EC2, LAN and cloud agents come with M11.

## Milestone 4 gaps

- **Whiteboard tools for agents** come with whiteboards (M5).
- **Local agent ignores MCP servers.** `polyph-localagent` does not read
  `mcpServers` from `session/new`, so it has no Polyph tools; it still gets
  claims through its ACP writes.
- **Container mode and MCP.** The MCP server command is the host's `polyph`
  binary, which is not inside a container image; container-mode agents get
  no Polyph tools until the binary is mounted into the container.
- **Inboxes are in memory.** Unread messages live in the daemon of the
  device running the agent and are lost if that daemon restarts. The
  messages themselves still pass through the relay buffer.
- **Reply to a person.** `reply` only answers agents; a person's message is
  answered in the agent's normal output.
- **Merges are per device.** The merge queue merges into the target branch of
  the repository on the device that ran the task. Members on different
  machines share work through a remote (`[merge] push`), which asks a person
  before pushing; nothing fetches or rebases a teammate's pushes yet, and a
  successor on another device after a handoff needs the branch fetched by hand.
- **Symbols are found by heuristics** (`polyph-merge::symbols`), not a parser:
  unusual formatting can put a change "outside any symbol", which falls back to
  a whole-file claim or an agent-resolved conflict, never to a wrong merge.
- **Symbol claims and shared directories.** Symbol claims let two agents write
  one file; in hosted mode or any shared directory their whole-file writes can
  still overwrite each other. Use them with task worktrees.
- **Hosted mode** is not persisted: the host and members run `polyph host
  --mode hosted` / `polyph mirror` again after a daemon restart. Files over
  512 KiB, build output and gitignored files are not mirrored; a member's
  mirror has no `.git`, so members' agents do not get task worktrees there.
- **Agents cannot pick up tasks themselves.** The board's `auto_pickup` field
  is not acted on; people start agents on cards.
- **Real-agent smoke tests** of the review, resolve and planner prompts have
  not run (no real agents installed here).

## Milestone 5 gaps

- **Terminal attach** exists in the TUI; `polyph term send/read` remain the
  scripted way in the CLI.
- **Late viewers of a terminal** see output from when their device came
  online (plus what the relay buffer replays); the owner does not resend
  scrollback to newcomers.
- **Agents' own command terminals** are summarized in transcripts, not shared
  as live terminals.
- **Terminal output redaction** works per output batch; a secret split across
  two batches can slip through.
- **OS notifications** are not shown by the daemon itself: local apps receive
  `notification` events and show them (M6/M7); Linux panels can use D-Bus.
- **Webhook URLs and secrets** are per device and not synced between a
  person's devices.
- **GitHub and Linear** are followed by polling (every minute for pull
  requests); no incoming webhooks from them yet. Linear only moves issues for
  card moves made on the device holding the key.
- **D-Bus** only on Linux; macOS and Windows status integrations come with
  the status pill (M7).
- **Activity checkpoints** cover cards (column and title); replaying other
  card fields needs the full card history.

## Milestone 6 gaps

- **The TUI does not resize shared terminals**: an attached terminal shows the
  owner's size; only the owner's own client can resize it.
- **No mouse support** in the TUI; everything is on the keyboard.
- **Whiteboards are not shown in the TUI** (they need a canvas); agents' and
  people's whiteboards are read in the apps (M7) or as text via the agents'
  `read_whiteboard` tool.
- **Plans** are created with `:plan` and accepted from the CLI
  (`polyph plan accept`); the TUI shows the drafts' board, not a plan review.
- **Screen readers**: the TUI follows terminal conventions (no live-region
  equivalent); the apps carry the accessibility work.
- **The Impeccable `critique`/`audit`/`polish` passes** named in DESIGN.md were
  done by hand against PRODUCT.md and DESIGN.md (the Impeccable skill is not
  installed in this environment).

## Milestone 7 gaps

- **Browser-only members** (joining a room from a browser with no daemon,
  using the wasm engine over the relay) come with the phone PWA in M8. The
  browser client today is a window onto the person's own daemon.
- **Desktop packaging** is configured (`tauri.conf.json`, icons) but no
  installers are built or signed yet; the app expects `polyph` next to it or
  on PATH. Release work is M11.
- **macOS and Windows** builds of the desktop app have not been run; it was
  built and run on Linux (WebKitGTK under Xvfb).
- **Shared terminals keep the owner's size**: the app shows them at their
  size with scrolling and does not resize them to the window.
- **Integrations are read-only in the app**: connecting GitHub or Linear and
  managing webhooks stay in the CLI.
- **Excalidraw's built-in font fallback** to a CDN is blocked by the page
  policy (fonts are bundled), which logs blocked requests in the console.
- **Simplified Chinese UI font** relies on platform fonts; the Noto Sans SC
  subset named in DESIGN.md is not bundled yet.
- **Agent-made whiteboard shapes** are not yet outlined in their voice.
- **The Impeccable passes** were done by hand with the design-taste
  inspector (`inspect.mjs --app`); its remaining warnings are the third
  typeface (DESIGN.md's serif for room names) and honest empty space.

## Process gaps

- **Apple design skill not installed.** It installs through a Claude Code
  `/plugin marketplace add https://github.com/NutshellEngineering/apple-design-skill`
  slash command, which a headless session cannot run. Install it in an
  interactive session before M7 polish.
- **Docs translation.** Docs ship in English only; the pipeline is to be built in M10.

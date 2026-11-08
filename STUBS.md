# Stubs and unfinished work

Every item here is either not built yet or built partially. Nothing listed
here should be described as done anywhere else.

## Not started (later milestones)

CLI commands not implemented yet: `spectate-link *` (spectator links are
not in the beta; see `docs/beta.md`). Memory notes cannot be added or
removed from the CLI: the daemon has `memory.list` only, so `polyph memory`
lists notes and agents write them.

## Partial work in finished milestones

- **Keyring per (epoch, issuer).** `docs/protocol.md` 6.4 stores keys per
  epoch and issuer so diverged epochs after a split can coexist. The
  `Keyring` in `polyph-proto` currently keys by epoch only; the daemon must
  add the issuer dimension when it implements split recovery (M2).
- **Spectator link limits.** The free plan's "1 spectator link per room" is
  not enforced by the relay, because links are client-side keys. Needs a
  relay frame to register link ids per room (minor protocol bump). The plan
  carries `spectator_links` already; still open after M10.
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
- **Domain.** The default relay is the project's relay on Fly.io
  (`wss://polyph-relay.fly.dev/ws`, `deploy/fly/`) and links point at the
  GitHub Pages site (`projectdev-git.github.io/Polyph`). Both can be changed
  at build time (`POLYPH_DEFAULT_RELAY`, `POLYPH_DOMAIN`); a product domain is
  still undecided. Pages cannot send headers, so the site there runs without
  the Content-Security-Policy that `vercel.json` sets.
- **Display-name changes** need a re-issued certificate and are not exposed.
- **Windows sandbox.** Since M11 agents run at low integrity level, so
  writes are limited to the worktree, writable paths and TMPDIR; hidden paths
  are still readable and the network is steered only by proxy variables (no
  AppContainer yet). Tested only on GitHub's Windows runners. Documented in
  `docs/sandbox.md`.
- **seccomp.** The Landlock fallback restricts files and TCP connects but adds
  no seccomp filter (for example against raw sockets or UDP). bubblewrap mode
  isolates the network with a namespace instead.
- **Agents' git settings (SEC-13) open on Landlock and Windows.**
  bubblewrap and Seatbelt keep an agent's checkout's `.git` read-only (task
  agents write only their task's folder in `refs/heads/polyph/` and
  `logs/refs/heads/polyph/` and their own worktree folder, with their own
  object store there); Landlock, Windows (low integrity) and container
  mode do not, so an agent working without a task there can plant config
  and hooks that the person's git, and the daemon's filter and merge
  drivers, run. The agent starts with a warning, and the daemon then
  fetches and pushes that checkout with guarded settings
  (`folders::mark_git_exposed`). There the daemon's own git writes also
  run without a sandbox (bubblewrap or Seatbelt confine them elsewhere).
  The Seatbelt rules, for agents and for the daemon's git writes (hard
  links included), have tests that run only on the macOS CI runner and
  have never run on a Mac outside CI. While an agent has commits it made
  itself in a turn that has not ended, its branch points at objects only
  its task's store has, so the person's `git fetch`, `git gc` and `git log
  --all` in that repository fail with "bad object" until the turn ends
  (the daemon also copies them in at start, and logs a warning on this
  machine naming the branch when a copy fails). No client shows
  `agent_warning` events yet (TUI, web feed, desktop); they are local to
  the daemon and not sent to other members. `task-branches.json` is never pruned. The reflog check on the branch merged into proves nothing
  for a branch inside a task folder. Tasks started before task branches
  moved to `polyph/<task id>/<name>` keep a branch the daemon did not
  record; it refuses to hand that branch to a new agent until the card's
  branch is cleared. See `docs/security.md`.
- **Worktree reads on Windows (SEC-16).** `read_under` refuses links and
  checks the opened file with `fstat` on Linux and macOS; on Windows it does
  not check that the file it opened is the one it checked (a link swapped in
  between), which matters only if agents run unsandboxed there.
- **Sandbox mechanisms on macOS and Windows** run only in CI. macOS CI found
  that the Seatbelt profile let agents write to the shared temp folders; that
  was fixed in M11 (only the agent's own TMPDIR is writable) and is covered
  by a unit test, but has not been re-run on a Mac outside CI.
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

## Milestone 4 gaps

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
  before pushing. Linked folders are fetched in the background and
  `polyph sync` fast-forwards a checkout when nothing would be lost, but
  nothing rebases a teammate's work, and a successor on another device after
  a handoff has not been tested end to end.
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
- **Agents do not start on cards by themselves.** An agent takes the next
  ready card when it calls `next_task`, but the board's `auto_pickup` field
  is not acted on; people start agents on cards.
- **Real-agent smoke tests** of the review, resolve and planner prompts have
  not run (no real agents installed here).

## Milestone 5 gaps

- **Late viewers of a terminal** see output from when their device came
  online (plus what the relay buffer replays); the owner does not resend
  scrollback to newcomers.
- **Agents' own command terminals** are summarized in transcripts, not shared
  as live terminals.
- **OS notifications** are not shown by the daemon itself: local apps receive
  `notification` events and show them (M6/M7); Linux panels can use D-Bus.
- **Webhook URLs and secrets** are per device and not synced between a
  person's devices.
- **GitHub and Linear** are followed by polling (every minute for pull
  requests); no incoming webhooks from them yet. Linear only moves issues for
  card moves made on the device holding the key.
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

- **Browser-only members** arrived in M8 as the phone app (`/phone/`). The
  desktop browser client is still a window onto the person's own daemon.
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

## Milestone 8 gaps

- **Nublet provider** is built on Nublet's side (kaorii-ako/nublet#7, a
  small provider contract plus a Polyph provider that reads the D-Bus status
  service; see `docs/nublet.md`). Not yet run inside GNOME Shell (no desktop
  in the build environment) and not merged into Nublet. Clicking an approval
  opens `polyph web`, not the single approval: Polyph has no link to one
  approval yet.
- **One room per phone.** The phone engine holds one room; joining another
  means leaving the first on that phone (Settings > Leave this room).
- **No push when the phone app is closed.** Approval alerts show while the
  app is open or in the background with its page alive. Web Push needs the
  hosted push service (proprietary, `cloud/`, M10).
- **Phone transcripts start at join.** Like late-joining daemons, the phone
  sees agents' transcript events from when it is online; it keeps the last
  1,000 per agent in memory, not on disk.
- **Daemon-only views and actions on the phone**: terminals, merges,
  starting or prompting agents, planning, integrations and usage need a
  computer running Polyph; the phone hides those views and says so for
  calls that reach the engine.
- **Phone storage** is sealed with a non-extractable WebCrypto key, which
  stops copies of the stored bytes from being read elsewhere, not script
  running on the same origin. The site must stay free of third-party script.
- **The website's own headers** for `/phone/` (CSP with `wasm-unsafe-eval`
  and `connect-src wss:`) are set by the daemon for local testing only; the
  website's hosting (M10) must send the same policy.
- **wasm size**: 4.9 MB (1.1 MB gzipped), built with `opt-level = "s"` and
  LTO but no `wasm-opt` pass (not installed here).
- **LAN rooms cannot pair a phone**: a LAN relay's self-signed certificate
  cannot be pinned by a browser; `phone.pair` refuses with an explanation.
- **VS Code extension** is packaged as a `.vsix` on the releases page but not
  published to a marketplace (see "Milestone 11 gaps"); it has been run in
  VS Code 1.144 on Linux under Xvfb. Its webview uses Polyph's own theme
  rather than VS Code's colors.
- **Claims in VS Code** are matched against workspace folders' git roots;
  a workspace that is not the room's repository simply shows no claims.

## Milestone 9 gaps

- **No brief for M9 in the repository.** Its scope was derived from what
  earlier milestones left for it (see `PLAN.md`); check it against the
  brief when that is available.
- **Polyph-hosted cloud agents** (run by Polyph for a subscription) are not
  built: they need accounts and billing (M10, proprietary `cloud/`). Agent
  hosts are the person's own machines; `--on cloud` means "any of my hosts".
- **Hosts keep no queue while offline**: a start, prompt or stop sent when
  the host is offline is lost (the start times out after 60 seconds).
- **One person per host daemon.** A shared server needs a separate
  `POLYPH_HOME` and user account per person.
- **Hosts' working copies are not synced**: the host works in its own
  checkout; results reach others through task branches, the merge queue and
  `[merge] push`, like any device.
- **Agent hosts on Windows and macOS** have not been tried; tests ran on
  Linux with Landlock.
- **Plugins are not signed or sandboxed beyond the agent sandbox**, and there
  is no plugin registry; people install folders they trust. On Windows the
  sandbox does not isolate the network (see the Windows sandbox entry
  above), so plugins there are only partly contained.
- **Hook actions** are limited to chat posts; no card or thread actions yet.
- **Built-in skill texts are English.** Template descriptions are
  translated; the skill files a template writes into a repository are team
  content and start in English.
- **Plugins are listed, not managed, in the app**; adding and removing stay
  in the CLI.

## Process gaps

- **Apple design skill not installed.** It installs through a Claude Code
  `/plugin marketplace add https://github.com/NutshellEngineering/apple-design-skill`
  slash command, which a headless session cannot run. Install it in an
  interactive session before M7 polish.
- **Docs translation.** Docs ship in English only; no translation pipeline
  was built in M10 (the website itself is in four languages).

## Milestone 10 gaps

- **No M10 brief in the repository.** Scope was derived (see `PLAN.md`);
  check it against the brief when available.
- **Web push** for phone notifications is not built: the hosted push service,
  VAPID keys and subscriptions are still to do. The phone app alerts only
  while it is open (see Milestone 8 gaps).
- **Team dashboard** (opt-in signed usage summaries, `ctx::USAGE`) is not
  built.
- **Prices are unset.** Plans carry an optional display `price` and a
  `stripe_price`; the website and account page say "Pricing coming soon" and
  checkout is refused until they are configured. Member limits per plan are
  not decided either (unset means unlimited).
- **Not run against real services.** Stripe, GitHub's device flow and the
  mail endpoint were exercised against local stand-ins only. Needs a Stripe
  test-mode run, a GitHub OAuth app with the device flow enabled, and a mail
  provider account (the code speaks Resend's API shape).
- **Not deployed.** The website runs on GitHub Pages
  (`.github/workflows/pages.yml`); `polyph-cloud` has no hosting yet, so the
  account pages and `polyph login` point at a placeholder (`api.polyph.dev`,
  set at build time with `POLYPH_DEFAULT_CLOUD`, `NEXT_PUBLIC_POLYPH_CLOUD`).
- **Rate limits behind a proxy.** Sign-in starts are limited per peer IP;
  behind a reverse proxy every request has the proxy's IP, so the proxy must
  limit or the service must learn to trust a forwarded header.
- **Plan cache.** The relay policy rebuilds every device's plan from the
  database on each change; fine for now, needs incremental updates at scale.
- **Logging out keeps the device linked** (rooms it owns keep the plan);
  unlinking is on the website's account page, not in the CLI.
- **Account management in the apps.** The desktop app, phone and VS Code do
  not show the account yet; it lives in the CLI and on the website.
- **Legal pages are drafts** and say so; they need legal review.
- **Website pictures** are interface fragments; no screenshots of the real
  desktop app yet (it is not packaged until M11). See `docs/website.md` for
  the inspector warnings that were kept and why.
- **Daemon session tokens for the cloud** are sealed like integration tokens
  and are per device; a person's other devices sign in separately.

## Milestone 11 gaps

- **No M11 brief in the repository.** Scope was derived (see `PLAN.md`).
- **Releases v0.1.0, v0.1.1 and v0.1.2 are published** on GitHub, built by
  `.github/workflows/release.yml`, which smoke-tests every archive on its own
  platform before upload. Still partial: no hand install outside CI is
  recorded on macOS or Windows (`install.ps1`, `.dmg`, `.msi`/`.exe`), nor
  of the AppImage and `.rpm` on Bazzite or Fedora Atomic.
- **Built and tested in the build sandbox (Linux x86_64 only):** the release
  archive (smoke test, `install.sh` from a local server, a tampered checksum
  refused), the `.vsix` in a real VS Code, the relay image from prebuilt
  binaries, and the relay Compose stack with Caddy (TLS and WebSocket
  upgrade). Not run yet: the from-source Dockerfiles (the sandbox's
  network proxy is not reachable from `docker build`) and `deploy/cloud`.
- **Windows code signing** is not set up; SmartScreen will warn. macOS signing
  and notarization run only when the Apple secrets are added.
- **No auto-update** in the desktop app (Tauri's updater needs a signing key
  and an update feed).
- **Not published** to the VS Code Marketplace or Open VSX; the publisher id
  `polyph` is a placeholder until an account exists.
- **Linux arm64 desktop** has `.deb` and `.rpm` only (no AppImage); musl
  (Alpine) and Windows on ARM have no prebuilt binaries.
- **LAN relay port** is chosen fresh each time; a fixed-port option would
  make firewall rules simpler.
- **The `.vsix` is 22.6 MB**, mostly Excalidraw's fonts, which the webview
  bundles twice (assets and excalidraw folders).

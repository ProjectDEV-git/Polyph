# Stubs and unfinished work

Every item here is either not built yet or built partially. Nothing listed
here should be described as done anywhere else.

## Not started (later milestones)

Crates that currently contain only a placeholder `lib.rs`:
`polyph-mcp`,
`polyph-git`, `polyph-merge`, `polyph-term`, `polyph-plugins`. Directories
`apps/`, `packages/`, `integrations/`, `deploy/` and `cloud/` (except its
LICENSE) are not built. The TUI does not exist yet: `polyph` with no
arguments prints help.

CLI commands not implemented yet (they arrive with their milestones):
`login`, `logout`, `cloud *` (and `agent start --on cloud`), `plan`,
`task *`, `memory *`, `term *`, `sync`, `spectate-link *`, `template *`,
`skills *`, `plugin *`, `mcp`. `polyph host --mode hosted` is
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

## Process gaps

- **Apple design skill not installed.** It installs through a Claude Code
  `/plugin marketplace add https://github.com/NutshellEngineering/apple-design-skill`
  slash command, which a headless session cannot run. Install it in an
  interactive session before M7 polish.
- **Docs translation.** Docs ship in English only; the pipeline is to be built in M10.

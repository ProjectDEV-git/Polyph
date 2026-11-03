# Stubs and unfinished work

Every item here is either not built yet or built partially. Nothing listed
here should be described as done anywhere else.

## Not started (later milestones)

Crates that currently contain only a placeholder `lib.rs`:
`polyph-agents`, `polyph-localagent`, `polyph-sandbox`, `polyph-mcp`,
`polyph-git`, `polyph-merge`, `polyph-term`, `polyph-plugins`. Directories
`apps/`, `packages/`, `integrations/`, `deploy/` and `cloud/` (except its
LICENSE) are not built. The TUI does not exist yet: `polyph` with no
arguments prints help.

CLI commands not implemented yet (they arrive with their milestones):
`login`, `logout`, `agent *`, `agents doctor`, `sandbox test`, `cloud *`,
`plan`, `task *`, `memory *`, `term *`, `sync`, `usage`, `spectate-link *`,
`template *`, `skills *`, `plugin *`, `mcp`. `polyph host --mode hosted` is
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
- **Security contact address** in `docs/security.md` is not set.
- **Relay self-hosting docs** for EC2, LAN and cloud agents come with M11.

## Process gaps

- **Apple design skill not installed.** It installs through a Claude Code
  `/plugin marketplace add https://github.com/NutshellEngineering/apple-design-skill`
  slash command, which a headless session cannot run. Install it in an
  interactive session before M7 polish.
- **Docs translation.** Docs ship in English only; the pipeline is to be built in M10.

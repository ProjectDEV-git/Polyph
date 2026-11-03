# Stubs and unfinished work

Every item here is either not built yet or built partially. Nothing listed
here should be described as done anywhere else.

## Not started (later milestones)

Crates that currently contain only a placeholder `lib.rs`:
`polyph-core`, `polyph-agents`, `polyph-localagent`, `polyph-sandbox`,
`polyph-mcp`, `polyph-git`, `polyph-merge`, `polyph-term`, `polyph-plugins`,
`polyph-i18n`, `polyph-daemon`, `polyph-testkit`. The `polyph` binary
(`polyph-cli`) is a placeholder. Directories `apps/`, `packages/` (except as
noted in `HANDOFF.md`), `integrations/`, `deploy/` and `cloud/` are not built.

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
- **Security contact address** in `docs/security.md` is not set.
- **Relay self-hosting docs** for EC2, LAN and cloud agents come with M11.

## Process gaps

- **Apple design skill not installed.** It installs through a Claude Code
  `/plugin marketplace add https://github.com/NutshellEngineering/apple-design-skill`
  slash command, which a headless session cannot run. Install it in an
  interactive session before M7 polish.
- **Docs translation.** Docs ship in English only; the pipeline is to be built in M10.

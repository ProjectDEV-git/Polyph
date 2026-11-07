# Positioning: Polyph, Amoeba and Murmell

Date checked: 2026-11-06.

## What could and could not be verified

On the date above, the launch writer's session could not open the vendors'
own sites. A fetch of `murmell.com` and of a third-party directory failed
with a name-resolution error, because those hosts are outside the session's
allowed network domains. A web search did not return an official Amoeba site
at all. Only third-party listings and search snippets were visible, and those
are not accepted as sources for public comparisons.

So every competitor cell below reads "not stated publicly". Read that as
"not verified by us on 2026-11-06". It does not mean the vendor leaves the
point unstated. A human must open both sites and fill the cells before any
comparison is published, citing the page URL and the date for each.

Rule for filling a cell: write only what the vendor's own public page says,
in its words or a close paraphrase, with the URL of that page and the date
checked. If the page does not say it, leave "not stated publicly". Do not
use directory listings, reviews, launch-site comments or search snippets.
Do not describe either product's quality, security or reliability.

Sources to open (to be confirmed by the human, since the official URLs are
not established here):

- Amoeba: official site, pricing page, docs, security or privacy page.
- Murmell: `https://murmell.com` (the domain a search snippet named; not
  opened), its pricing page and docs.

## Comparison table

The Polyph column comes from this repository (`PRODUCT.md`, `docs/beta.md`,
`docs/self-hosting.md`, `STUBS.md`) and is the only column that is filled.

| Topic | Polyph (beta) | Amoeba | Murmell |
|---|---|---|---|
| Command line first | Yes. `polyph` CLI and terminal UI are the base; desktop, browser, phone and VS Code show the same state | not stated publicly | not stated publicly |
| Platforms | Linux x86_64 supported; macOS and Windows experimental (CI only); Linux arm64 CLI, `.deb`, `.rpm` | not stated publicly | not stated publicly |
| Immutable Linux (Bazzite, Fedora Atomic) | Installs into `~/.local`; Bazzite is where the team tests | not stated publicly | not stated publicly |
| Where agents run | On each member's own machine; optionally on the member's own server (`polyph agent-host`). Polyph does not run agents for you | not stated publicly | not stated publicly |
| End-to-end encryption | Yes. The relay forwards ciphertext; it sees room ids, device keys, message headers and sizes, and IP addresses (`docs/beta.md`) | not stated publicly | not stated publicly |
| Self-hosting | Relay is one binary with SQLite; Docker Compose, systemd and EC2 steps in `docs/self-hosting.md`; LAN mode with no internet | not stated publicly | not stated publicly |
| Relay memory | 71 MB peak, 50 rooms of 5 people, 2 messages per second per member (measured, `docs/self-hosting.md`) | not stated publicly | not stated publicly |
| Agents supported | Claude Code, Codex (through `codex-acp`), OpenCode, Gemini CLI, any ACP agent, local models through an OpenAI-compatible endpoint | not stated publicly | not stated publicly |
| Your own keys or subscriptions | Yes; Polyph does not proxy model calls | not stated publicly | not stated publicly |
| File claims | Files, folders and functions (functions found by pattern) | not stated publicly | not stated publicly |
| Planner | Yes (`polyph plan`) | not stated publicly | not stated publicly |
| Merge handling | Per-task worktrees, reviewer agent, merge queue; conflicts resolved from both tasks' intent by an agent | not stated publicly | not stated publicly |
| Board and chat | Kanban, threads, activity feed with replay, whiteboards | not stated publicly | not stated publicly |
| License | Engine and apps Apache-2.0; `cloud/` proprietary | not stated publicly | not stated publicly |
| Price | Free in the beta; plan prices not set | not stated publicly | not stated publicly |

Do not add a row for a feature unless the vendor's page can be cited for it,
or the Polyph cell can be cited from this repository.

## Polyph's beta limits

State these next to any comparison. They are from `docs/beta.md` and
`STUBS.md`.

- macOS and Windows are experimental, unsigned and tested in CI only. Linux
  x86_64 is the tested platform.
- Sandboxing on Windows is weaker than on Linux, and the Linux Landlock
  fallback has no seccomp filter.
- Late joiners do not see agents' earlier transcripts.
- Merges happen on the computer that ran the task; teammates get them through
  a git remote. Nothing rebases a teammate's branch.
- Function-level claims use pattern matching, not a parser.
- The built-in local-model agent gets no Polyph tools.
- No spectator links, no push notifications to a closed phone app, no
  accounts, billing or team dashboard in the beta.
- Inboxes are in memory and lost on a daemon restart.
- The beta relay is one machine on Fly.io in Singapore.
- No customers, testimonials, benchmarks or press exist. The relay load test
  is the only measurement.

## Wording for public use

When the table is filled, a safe form for posts and the website is a link to
this comparison with its date, not a claim about either vendor. Example
sentence once verified: "As of <date>, <vendor>'s site says <quote>." Avoid
"better than", "unlike", "only" and "first". The seven positioning points
in `PRODUCT.md` describe Polyph and can be stated without naming anyone.

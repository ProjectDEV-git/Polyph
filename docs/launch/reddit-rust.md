# r/rust draft

Draft. Posting is human-only (Kao). Check the subreddit's rules and its
current flair for project announcements before posting.

## Title

Polyph: a Rust daemon, CLI, TUI and single-binary relay for coordinating coding agents in an E2E-encrypted room (public beta, Apache-2.0)

## Body

Polyph lets several people run their own coding agents in one shared room.
The core is Rust: the daemon, the CLI, the ratatui terminal UI, the relay,
the sandbox layer and a local-model ACP agent. The desktop app is Tauri 2
with a React front end. The phone app runs the same room engine compiled to
WebAssembly (`polyph-phone`), so it joins a room from a browser with no
daemon.

Layout of the workspace, crate by crate, as it stands:

- `polyph-proto`: wire format and crypto; `docs/protocol.md` describes it.
- `polyph-core`: room state, claims, merge of room documents, simulation tests
  with real engines.
- `polyph-relay`: the relay, one binary with SQLite. It stores encrypted
  snapshots and forwards envelopes; it never holds keys.
- `polyph-daemon`, `polyph-cli`, `polyph-tui`, `polyph-term`.
- `polyph-agents`, `polyph-mcp`, `polyph-localagent`, `polyph-sandbox`,
  `polyph-plugins`, `polyph-merge`, `polyph-git`, `polyph-phone`,
  `polyph-i18n`, `polyph-testkit`.

Things that may interest this subreddit:

- The relay under load. `polyph-loadtest` (in `polyph-testkit`) runs the
  release relay as a separate process, 250 clients as certified members, and
  reads its peak resident set size. For 50 rooms of 5 at 2 messages per
  second per member: 71 MB peak, 40 MB after. A worst-case run at 10 messages
  per second with 1,000-byte payloads peaked at 118 MB. Idle is 8 MB. The
  machine was a 4 vCPU Xeon 2.1 GHz on Linux 6.18. Commands to reproduce are
  in `docs/self-hosting.md`.
- Agent sandboxing: bubblewrap on Linux with a Landlock fallback, Seatbelt on
  macOS, low integrity level on Windows. The Landlock fallback has no seccomp
  filter yet. macOS and Windows sandboxing has run in CI only.
- Symbol-level claims use heuristics, not a parser (`polyph-merge::symbols`).
  A tree-sitter based approach would be a reasonable contribution; I have
  not decided on it.
- i18n via Fluent: the CLI, TUI, apps and website share `.ftl` files in four
  languages.

Install (Linux and macOS, into `~/.local`, no root):

    curl -fsSL https://raw.githubusercontent.com/ProjectDEV-git/Polyph/Main/scripts/install.sh | sh

Or build from source; "How to build and test" is in `HANDOFF.md`. The
minimum Rust version the website states is 1.94.

Beta limits that matter here:

- macOS and Windows builds are experimental, unsigned and smoke-tested in CI
  only. Linux x86_64 is the tested platform.
- Musl (Alpine) and Windows on ARM have no prebuilt binaries.
- `STUBS.md` lists every partial piece. Read it before filing a bug that it
  already covers.

Apache-2.0 for the engine and apps. A separate `cloud/` directory
(accounts, billing) is proprietary and not part of the beta.

Beta page: https://github.com/ProjectDEV-git/Polyph/blob/Main/docs/beta.md
Site: https://projectdev-git.github.io/Polyph
Releases: https://github.com/ProjectDEV-git/Polyph/releases

Code review of the relay and the protocol crate is the most useful feedback
I could get.

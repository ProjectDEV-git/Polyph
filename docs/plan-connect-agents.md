# Plan: connect any agent, share context, clone on join

Status: **proposal, not started.** Nothing here is built. When work starts,
each phase becomes a milestone in `PLAN.md` (M12 onward), its gaps go in
`STUBS.md` and its choices in `DECISIONS.md`, as for earlier milestones.

## 1. Why

Polyph today works well for agents it starts itself (`polyph agent start`):
they run in its sandbox and get the Polyph tools through `polyph mcp`. Most
people already have an agent open, though: Claude Code in a terminal,
Cursor or VS Code with Copilot, Codex, Gemini CLI. Those cannot join a room
today, and getting a teammate going takes several manual steps.

What the code shows today:

| Area | Today | Where |
| --- | --- | --- |
| Agents people already run | Cannot connect. `polyph mcp` needs `POLYPH_MCP_PROXY` and a per-agent token that only exist inside an agent Polyph started; otherwise it exits with "runs only inside a Polyph agent". | `crates/polyph-mcp/src/bridge.rs`, `crates/polyph-daemon/src/coord.rs` (`internal`) |
| Setting up a client | Nothing writes MCP config for Claude Code, Cursor, VS Code, Codex and others. | none |
| Repository | A room does not record its repository. A teammate must find, clone and `cd` into it before `polyph join`. | `crates/polyph-cli/src/main.rs` (`Host`, `Join`) |
| Shared context | Memory notes pinned to code, skills, the team-state digest. No project-wide brief. Inboxes are in memory; late joiners get no older transcripts. | `crates/polyph-core/src/memory.rs`, `STUBS.md` M4 gaps |
| Several machines | `polyph sync` not built; merges are per device; nothing fetches teammates' pushes; a handoff to another device needs a manual fetch. | `STUBS.md` "Merges are per device" |
| Big projects | `auto_pickup` on cards is stored but ignored; no areas or ownership. | `crates/polyph-proto/src/crdt.rs`, `STUBS.md` |

## 2. Goals and how we measure them

1. **One command to connect.** A person with Claude Code, Cursor, VS Code,
   Codex, Gemini CLI, Windsurf, Zed or OpenCode installed runs
   `polyph connect` and, after confirming, every detected client has the
   Polyph tools. Measured: under 2 minutes from install on a clean machine,
   tested per client.
2. **One link to join.** A teammate with nothing but Polyph installed runs
   `polyph join <code>` in any folder; Polyph offers to clone the room's
   repository, opens it, and offers to connect their agents. No `cd`, no
   copying URLs.
3. **Same context for every agent.** Any connected agent, started or
   attached, gets the same project brief, team state, its task, and the
   notes for the files it touches, in its first tool call, within a size
   budget.
4. **Safe by default.** Attached agents cannot approve, invite or change
   settings; everything they send is redacted; the room shows clearly
   which agents are not sandboxed by Polyph; owners can turn attached
   agents off.
5. **Works for big projects.** 10 people with 3 agents each (30 agents) on
   a repository of 100,000 files: claims, the watcher and the context pack
   stay responsive (targets in section 9).

Out of scope: hosted agents run by Polyph (needs `cloud/`), a plugin
registry, and replacing any client's own permission system.

## 3. Words used here

- **Started agent**: an agent Polyph launches (today's `polyph agent start`).
  Sandboxed by Polyph.
- **Attached agent**: an agent the person runs themselves (Claude Code in
  their terminal, Cursor's agent) that connects to the room through MCP.
  Not sandboxed by Polyph; the client's own permissions apply.
- **Linked repository**: the git remote a room records, so joiners can
  clone it.
- **Project brief**: a shared document of the room that holds what every
  agent should know about the project.
- **Context pack**: what an agent gets when it asks for context: brief,
  team state, its task, relevant notes, within a token budget.

## 4. Phase 1: attached agents and `polyph connect`

The base for everything else. Ships as one milestone.

### 4.1 How an attached agent reaches the daemon

Two transports, both served by the person's own daemon, both local only.

**A. stdio (default, works with every client).** `polyph mcp` keeps its
current behavior when `POLYPH_MCP_PROXY` is set. When it is not set, it
becomes an attached-agent server:

1. It reads the local API token (`api.token`, already 0600 / per-user ACL)
   and connects to the daemon's local API, as the CLI does. If the daemon
   is not running it starts it, as the CLI does.
2. On MCP `initialize` it reads `clientInfo` (name and version, for
   example `claude-code`) and asks the client for its `roots` when the
   client supports them.
3. It works out the room, first match wins:
   `--room <id>` flag → `POLYPH_ROOM` env → the git repository of the first
   root → the git repository of its working directory → the only room
   open on this device. If none match, every tool answers with one line
   saying how to fix it (`polyph connect --room …` or `polyph start`),
   instead of failing to start. Clients hide servers that fail to start,
   which is confusing.
4. It calls a new daemon method `agent.attach {room, client, version, cwd,
   roots}` and gets back an attached-agent id and a session token. Every
   tool call afterwards goes through `agent.call {id, token, tool, args}`,
   which reuses `coord::call_tool` unchanged.
5. When stdin closes, it calls `agent.detach`.

**B. Streamable HTTP on localhost (for clients that prefer a URL).** The
daemon serves MCP at `http://127.0.0.1:<port>/mcp` (port in the daemon's
state file). Rules, all required:

- Bind to the loopback address only, never `0.0.0.0`.
- Bearer token per client, issued by `polyph connect` and stored in that
  client's config; tokens are listed and revoked with `polyph connect
  --list` / `polyph disconnect <client>`.
- Check `Host` and `Origin` (refuse anything but loopback names, refuse any
  browser `Origin`) to stop DNS-rebinding attacks from web pages. Reuse the
  checks in `crates/polyph-daemon/src/web.rs`.
- No CORS headers.

stdio is the default because it needs no port or token in config files.
HTTP exists for clients or setups where a stdio command is awkward.

### 4.2 What an attached agent is in the room

- It is a voice of its owner, like a started agent: label
  `<owner>/<client>-<n>` (for example `ann/claude-code-1`), a shade of the
  owner's color.
- Room state gains `kind: started | attached`, `client` and `sandboxed`
  for each agent. This is a minor protocol bump; older clients already
  ignore unknown fields (forward-compatible decoding, `docs/protocol.md`).
  Update `docs/protocol.md`.
- Every surface (TUI, app, phone, VS Code, pill) shows attached agents with
  a "not sandboxed by Polyph" label next to the state, per Product
  Principle 2.
- Presence: online while the MCP session is open, plus a heartbeat every
  30 s for HTTP sessions. When it goes offline its claims are kept for a
  grace period (default 10 minutes, `[attached] claim_grace`), then
  released with the usual revoked-claim notice. Reconnecting with the same
  client and folder within the grace period gets the same id back.
- What it may do: the same MCP tools as a started agent. What it may not
  do: approve, deny, invite, change roles or settings, start other
  agents. Those methods are not in the MCP surface, and `agent.call`
  refuses anything not in `polyph_mcp::tools`.
- Usage and budgets: attached agents do not report token use (Polyph does
  not see their model traffic). They show "usage not reported" rather
  than zero.

### 4.3 Where attached agents edit

Two modes, both supported from Phase 1:

1. **In your checkout (default, quick help).** The agent edits your normal
   working copy. Claims still work: explicit claims through the tools
   (layer 1) and the worktree watcher (layer 3, `crates/polyph-daemon/src/watch.rs`)
   claims files as they change and warns on overlap. Automatic claims on
   edits (layer 2) need the client's help, which the Claude Code plugin
   gives in Phase 4.
2. **In a task worktree (recommended for planned work).** New command
   `polyph task open <card> [--with claude|codex|gemini|opencode|cursor|code]`
   creates the task's worktree and branch exactly as started agents get
   them (`polyph-git`), then launches the chosen terminal agent in it with
   `POLYPH_ROOM` and `POLYPH_TASK` set, or opens the folder in Cursor or VS
   Code. `finish_task` then goes through review and the merge queue as for
   started agents. This is what makes attached agents safe on big
   projects.

### 4.4 `polyph connect`

One command that sets up every agent client found on the machine.

```bash
polyph connect                 # detect, show what will change, ask, write
polyph connect claude cursor   # only these
polyph connect --project       # write project files (.mcp.json, .cursor/mcp.json, .vscode/mcp.json) instead of user config
polyph connect --print cursor  # print the snippet only, change nothing
polyph connect --list          # what is connected, with config paths
polyph disconnect [client]     # remove only what connect added
```

Behavior, all required:

- **Shows exactly what will happen** before writing: file path, the lines
  added, and whether a backup is made. One confirmation for all; `--yes`
  for scripts.
- **Prefers each client's own command** where one exists (for example
  `claude mcp add` and `codex mcp add`) so the client owns its file format.
  Otherwise edits the file directly.
- **Edits safely**: keeps comments and formatting (JSONC-aware editing for
  JSON files, `toml_edit` for TOML), only adds or replaces the one `polyph`
  entry, writes atomically (temp file + rename), keeps a backup next to the
  file the first time (`*.polyph-backup`).
- **Never stores a secret** in a project file. Project-scoped entries use
  the stdio command only; HTTP tokens go only in user-scoped config.
- **Writes an absolute path** to `polyph` (GUI apps such as Cursor often do
  not inherit the shell's `PATH`), and rewrites it on `polyph update` when
  the path changes.
- **Tells the person what to do next** per client ("restart Cursor", "run
  /mcp in Claude Code to check").

Clients and where their MCP config lives. These paths change between
releases, so they live in one data table
(`crates/polyph-cli/src/connect/clients.rs`) with a golden-file test each,
and each must be checked against that client's current documentation when
the phase starts:

| Client | How Polyph registers | Notes |
| --- | --- | --- |
| Claude Code | `claude mcp add --scope user polyph -- <polyph> mcp`; project: `.mcp.json` | Phase 4 adds the plugin |
| Codex CLI | `codex mcp add` or `~/.codex/config.toml` `[mcp_servers.polyph]` | also used by `codex-acp` |
| Cursor | `~/.cursor/mcp.json` or `.cursor/mcp.json`, key `mcpServers` | GUI: absolute path needed |
| VS Code (Copilot agent mode) | `.vscode/mcp.json` key `servers`; user settings | Phase 4: the extension registers it with no file |
| Gemini CLI | `~/.gemini/settings.json` key `mcpServers` | |
| Windsurf | `~/.codeium/windsurf/mcp_config.json` | |
| Zed | `settings.json` key `context_servers` | JSONC |
| OpenCode | `opencode.json` key `mcp` | |
| Others (Cline, Roo, Continue, JetBrains) | `--print` only | their storage paths are not stable |

### 4.5 Rules text for agents without hooks

Agents only claim files if they know to. `polyph connect --project` also
offers to add a short Polyph section between markers to the repository's
agent instructions (`AGENTS.md`, plus `CLAUDE.md` and
`.cursor/rules/polyph.mdc` when present):

```text
<!-- polyph:start -->
This repository is worked on by several agents through Polyph.
Before editing files, call the polyph tool claim_files with the paths.
At the start of a task, call get_context. When done, call finish_task.
<!-- polyph:end -->
```

The text comes from a template in `polyph-plugins`, not hard-coded, and is
team content (English to start, like built-in skills). `polyph disconnect
--project` removes the block.

### 4.6 `polyph start`: one command from zero

Idempotent onboarding for the person who hosts:

```bash
cd my-repo
polyph start
```

1. Checks the daemon (starts it), the relay (reachable), the sandbox
   (`sandbox test`, quick mode).
2. Finds the room linked to this repository on this device, or creates
   one named after the repository (asks first).
3. Links the repository (Phase 3; until then it records the remote URL
   only).
4. Runs `polyph connect` for the clients it finds.
5. Prints the join code and a join link, and a one-line `polyph join`
   command to send.

Running it again changes nothing and prints the same summary, so it is
also the "where was I" command. The repository-to-room mapping is stored in
the daemon's local config, not in the repository.

### 4.7 `polyph doctor`

Replaces guessing with one report: daemon, relay, sandbox, linked repo,
each client's MCP config (present, path to `polyph` valid, last attach
time), started-agent CLIs (today's `agents doctor`). Each problem line ends
with the exact command that fixes it. `agents doctor` stays as an alias.

### 4.8 Phase 1 tests

- Unit: room resolution order; config writers for every client in the
  table, with fixtures that contain comments, other servers and odd
  formatting; disconnect restores the original bytes.
- `polyph-testkit/tests/m12.rs`: a scripted MCP client (extend
  `polyph-testkit/src/bin/mcp.rs`) attaches over stdio and over HTTP; its
  claims appear in a teammate's daemon through a real relay; tools refused
  without a token; HTTP refused with a browser `Origin` or a foreign
  `Host`; detach keeps claims for the grace period then releases them;
  reattach gets the same id; `agent.call` refuses non-MCP methods.
- `polyph task open --with <fake agent>` runs the fake agent in the task
  worktree and `finish_task` reaches the merge queue.
- Opt-in smoke tests (`POLYPH_SMOKE=1`) with real Claude Code, Codex and
  Gemini CLI.
- All new strings in four locales; `pnpm check:i18n` passes.

## 5. Phase 2: shared context

### 5.1 Project brief

A new CRDT document `Context` (a `yrs` doc like the board), with sections:
overview, architecture, conventions, commands (build, test, run), decisions,
glossary, and free sections the team adds.

- People edit it in the app (a new "Project" view), the TUI (read, and edit
  in `$EDITOR`), and `polyph context edit`.
- Agents add to it with a tool (`propose_context`): a proposal shows in the
  approvals stack for Owners and Maintainers, stating the exact text and
  section. The room can set `[context] agent_edits = "approve" | "direct" |
  "off"` (default `approve`), so agents cannot quietly rewrite what every
  other agent reads.
- `polyph context import` fills a first draft from `README.md`, `AGENTS.md`,
  `CLAUDE.md`, `CONTRIBUTING.md` (shown for approval, not applied blindly).
- Size limit per section (like `MAX_NOTE`), redaction before it leaves the
  daemon, as for every other document.

### 5.2 Context pack and new tools

New MCP tools in `polyph_mcp::tools`:

| Tool | Returns |
| --- | --- |
| `get_context` | the context pack: brief, team state, my claims, my task (card, plan, dependencies), memory notes for my task's files, skill names. Default budget 6,000 tokens (`[context] pack_tokens`), trimmed by priority with "call X for more" lines. |
| `search_context {query}` | matches across brief, memory notes, cards, threads and task summaries, ranked by a local keyword index (no embeddings, no network). |
| `log_progress {text}` | a short progress or handoff note on the agent's card thread. |
| `next_task` | Phase 5. |

MCP **resources** for clients that browse them (`polyph://context/brief`,
`polyph://team`, `polyph://claims`, `polyph://tasks/<id>`) and one MCP
**prompt** (`polyph-start`: "read the context, claim, work, finish"), so
clients that show prompts as slash commands get `/polyph-start`.

### 5.3 Close the gaps that lose context

- **Persistent inboxes**: store unread messages in the daemon's encrypted
  room files, so a restart loses nothing (fixes the M4 gap).
- **Task summaries for late joiners**: when an agent finishes, hands off or
  detaches, the daemon asks it (started agents) or offers the tool (attached
  agents) for a summary, and stores it on the card thread in the CRDT.
  Late joiners and successors get summaries, not full transcripts (full
  transcripts stay per device; see `STUBS.md`).
- **Handoff brings context**: the existing handoff brief also includes the
  previous agent's summaries and notes.

### 5.4 Agents without MCP

`polyph context export` writes the brief into the Polyph block of
`AGENTS.md` (section 4.5), so any agent that reads `AGENTS.md` gets it.
Opt-in, because it puts room content into the repository.

### 5.5 Phase 2 tests

Brief converges between three daemons and the phone; agent proposals need
approval under `approve`; the pack stays within budget for a room with
1,000 notes and 500 cards; search finds notes, cards and threads; inboxes
survive a daemon restart; a late joiner reads task summaries.

## 6. Phase 3: linked repository and clone on join

### 6.1 Linking

- `polyph start` and `polyph host` record the repository in the room's
  encrypted settings: remote URL, default branch, optional subdirectory.
  The relay never sees it (same as all room content).
- The URL is cleaned before it is stored: user info (tokens, passwords) is
  removed; only `https://`, `ssh://` and scp-style `git@host:path` are
  accepted.
- `polyph room repo set <url> | clear | show` changes it later (Owners and
  Maintainers).

### 6.2 Join and clone

`polyph join <code>`:

1. Inside a checkout whose `origin` matches the linked URL: use it (today's
   behavior).
2. Inside a checkout that does not match: say so and ask before going on.
3. Not in a checkout: show the URL and offer to clone into
   `~/polyph/<room-name>` (`--dir` to choose, `--no-clone` to skip).

Cloning, all required:

- Runs the person's own `git`, with their own credentials (credential
  helper, SSH agent, `gh`). Polyph never sends credentials through the
  room.
- If authentication fails and the remote is on GitHub, offers the GitHub
  device flow that `polyph github connect` already has, and uses that
  token for this clone through `GIT_ASKPASS` for this one command only.
- Safe arguments: the URL goes after `--`; `-c protocol.allow=never` with
  `protocol.https.allow=always`, `protocol.ssh.allow=always` (no `file`,
  `ext` or other transports); no submodule recursion unless the person
  asks (`--recurse-submodules`).
- Large repositories: `--filter=blob:none` by default (partial clone), plus
  `--depth <n>` and `--sparse <paths>` options. Task worktrees then share
  the one object store, as they do today.
- After cloning: links the folder to the room, runs `polyph connect`
  (asks first), prints the next step.

The same applies to `polyph agent-host join` (clones into `--workdir` when
it does not exist) and to the desktop app's join screen (a folder picker
with the clone shown as the default). The phone cannot clone; it shows the
command to run on a computer.

### 6.3 `polyph sync` and work across machines

Fixes the "merges are per device" gap:

- `polyph sync` fetches the linked remote, fast-forwards the default branch
  when the checkout is clean, and fetches the task branches that cards
  point at.
- When a merge is pushed (`[merge] push`), the room gets a `merged` event;
  other daemons fetch automatically (`[sync] auto_fetch = true`, default
  on; never rebases or touches a dirty working copy, only fetches and
  reports).
- A handoff to an agent on another device fetches the task branch first,
  so the successor can start without manual steps.

### 6.4 Phase 3 tests

Linking strips credentials; refused transports and option-looking URLs;
join outside a checkout clones from a local test server (a bare repository
served by `git daemon` or a test-only file transport flag that release
builds do not have); mismatch warning; partial clone; `agent-host join`
clones; auto-fetch after a pushed merge on a second daemon; cross-device
handoff starts without a manual fetch.

## 7. Phase 4: plugins and app surfaces

### 7.1 Claude Code plugin

A Claude Code plugin in this repository (`integrations/claude-code/`, the
directory `STUBS.md` already reserves), installable with
`/plugin marketplace add ProjectDEV-git/Polyph` and offered by
`polyph connect`. It contains:

- the MCP server entry (`polyph mcp`), so installing the plugin is enough;
- **hooks**, the part no config file can give:
  - SessionStart: adds a short context pack to the session;
  - PreToolUse on file edits: asks the daemon whether another agent holds
    the file; if so, blocks the edit with the holder's name and how to ask
    them (`ask_agent`), which gives attached Claude Code the same claim
    enforcement started agents have (layer 2);
  - PostToolUse on file edits: claims the edited file automatically;
  - Stop / SessionEnd: offers a summary and detaches;
- skills: Polyph's workflow (claim, ask, finish) and the existing
  `.polyph/skills/*` surfaced through `read_skill`;
- slash commands: `/polyph:status`, `/polyph:claims`, `/polyph:task`.

Hooks call a small new command, `polyph hook <event>`, that reads the hook
JSON on stdin and answers fast (one local API call, no daemon start; under
100 ms). If the daemon is down the hook warns and allows the edit
(fail-open), unless the room sets `[attached] enforce = "strict"`.

Other clients get the same behavior where they offer hooks; until then they
rely on the rules text (section 4.5) and the watcher.

### 7.2 VS Code extension

- Registers Polyph as an MCP server through VS Code's MCP server
  definition provider API, so Copilot agent mode gets the tools with no
  file written (check the API's current name and stability when the phase
  starts; fall back to writing `.vscode/mcp.json`).
- Command "Polyph: Connect AI agents" runs the same flow as
  `polyph connect`, with VS Code's own confirmation dialogs.
- Shows attached agents with their "not sandboxed" label in the sidebar.

### 7.3 Desktop app and browser client

- First-run checklist with three steps: open a repository (or clone the
  linked one), host or join, connect agents.
- Settings > Agents: detected clients with a switch each (calls
  `connect.*` daemon methods, which share code with the CLI), and the
  started-agent CLIs from `agents doctor`.
- Join links open the app (`polyph://join/<code>` registered by the
  desktop app) and go straight to the clone step.

### 7.4 Phase 4 tests

Hook command with recorded hook inputs (allow, block with holder, daemon
down fail-open, strict mode); plugin manifest validated in CI; VS Code
provider in the existing VS Code end-to-end suite; app checklist in the
browser end-to-end suite; four locales.

## 8. Phase 5: big projects

- **Agents pick up work**: implement `auto_pickup`. New tool `next_task`
  asks the acting host's arbiter (the same one that settles claims) for
  the next ready card that matches the agent's area and has no blocking
  dependency; the arbiter assigns it to exactly one agent, so two agents
  never take the same card.
- **Areas**: `[areas]` in `polyph.toml` maps paths to named areas (like
  CODEOWNERS). The planner splits work by area, the context pack includes
  the area's brief section and notes, and claims can be made per area.
- **Multi-repository rooms** (design first, build if needed): a room links
  several repositories; claims, notes and tasks carry a repository name;
  `polyph join` offers to clone each. Written as a design note in
  `DECISIONS.md` before any code, because it touches the claim path format
  in the protocol.
- **Hosted mode persistence** (the M4 gap), since big teams lean on it.
- **Scale work and targets** (section 9), measured with an extended
  `polyph-loadtest`.

## 9. Performance targets

| What | Target |
| --- | --- |
| `polyph mcp` (stdio) ready after start | under 300 ms with the daemon running |
| Tool call round trip on the same machine | under 50 ms at the median, excluding `ask_agent` |
| `polyph hook` answer | under 100 ms |
| Claim visible to a teammate through the relay | under 1 s at the median |
| Watcher on a 100,000-file checkout | under 5 % of one core idle; ignores `.gitignore`d paths |
| Context pack build | under 200 ms for 1,000 notes and 500 cards |
| Room with 30 agents (10 people × 3) | no drop in the above; relay memory within the measured budget in `docs/self-hosting.md` |

## 10. Security review

Each item gets a test or a written reason in `docs/security.md`:

1. **Local MCP access** is limited to the same OS user: stdio through the
   0600 API token; HTTP through loopback binding, per-client tokens, and
   `Host`/`Origin` checks against DNS rebinding.
2. **Attached agents are not sandboxed by Polyph.** Stated in
   `docs/sandbox.md`, labeled on every surface, and switchable off per room
   (`[attached] allowed = false`, Owners only) and per member.
3. **Prompt injection between teammates.** Text from other members (chat,
   notes, cards, the brief) reaches your agent. Tool outputs mark it as
   content written by others, not instructions; the brief needs approval
   for agent edits by default; attached agents cannot approve anything.
4. **Redaction** runs on everything attached agents send, as for started
   agents.
5. **Repository URLs** never carry credentials; cloning uses only safe
   transports and option-safe arguments.
6. **Config files**: only the `polyph` entry is touched, atomic writes,
   backups, no secrets in project files, full undo.
7. **Hooks** fail open by default (a down daemon must not stop someone's
   work), with a strict mode for teams that want hard enforcement.

## 11. Order and size

| Phase | Ships | Depends on | Size |
| --- | --- | --- | --- |
| 1 | attached agents (stdio + HTTP), `polyph connect` / `disconnect`, rules text, `polyph start`, `polyph doctor`, `polyph task open --with` | none | large |
| 2 | project brief, `get_context`, `search_context`, `log_progress`, resources and prompt, persistent inboxes, task summaries, `context export` | 1 | medium |
| 3 | linked repository, clone on join, `polyph sync`, auto-fetch, cross-device handoff | 1 | medium |
| 4 | Claude Code plugin with hooks, `polyph hook`, VS Code MCP provider, app checklist and agent switches, join links | 1, 2 | medium |
| 5 | `next_task` and auto pickup, areas, hosted-mode persistence, scale work; multi-repo design | 1–3 | large |

Phases 2 and 3 can run in parallel after Phase 1. Each phase ends like the
earlier milestones: tests green on Linux, macOS and Windows, docs updated
(`README.md`, `docs/extending.md`, `docs/protocol.md`, `docs/security.md`,
`docs/sandbox.md`), `HANDOFF.md`, `STUBS.md` and `DECISIONS.md` updated.

## 12. Decisions needed before starting

1. **Attached agents on by default?** Recommended: on, with the "not
   sandboxed" label and an Owner switch to turn them off.
2. **Agents editing the project brief.** Recommended: proposals that need
   approval (`approve`).
3. **Default clone folder.** Recommended: `~/polyph/<room-name>`.
4. **First clients to support.** Recommended for Phase 1: Claude Code,
   Codex, Cursor, VS Code, Gemini CLI; the rest in the same phase if their
   config formats check out, otherwise `--print` only.
5. **Hook enforcement.** Recommended: fail-open by default, `strict` as a
   room setting.
6. **Plugin location.** Recommended: `integrations/claude-code/` in this
   repository, published through this repository as a Claude Code
   marketplace.

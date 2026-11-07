# Beta journeys

How each launch audience in `PRODUCT.md` gets from nothing to useful work in
the beta, using only commands that exist in `polyph` today. The last
section is the "first ten minutes" script the product reviewer times before
each beta release.

Every journey assumes Linux or macOS with git installed. On Windows, use the
PowerShell install line from the README; everything after it is the same.
Limits that affect these journeys are listed in `docs/beta.md`.

Install, for everyone:

```bash
curl -fsSL https://raw.githubusercontent.com/ProjectDEV-git/Polyph/Main/scripts/install.sh | sh
polyph --version
```

To run an agent that Polyph starts, its ACP adapter must be installed.
`polyph agents doctor` lists each agent and the command that installs what
is missing (for Claude Code:
`npm install -g @zed-industries/claude-agent-acp @anthropic-ai/claude-code`).

## 1. Small startups

A team of three to six, each with their own Claude Code or Codex
subscription, working on one private repository.

1. One person sets up the room in the repository:
   `cd ~/code/payments && polyph start`. It asks for their name, makes a
   room for the folder, connects the agents it finds on the computer, and
   prints a reusable join code.
2. Write the project brief every agent reads:
   `polyph brief set docs/overview.md` (or `polyph brief edit`).
3. Teammates install, then join in an empty folder:
   `mkdir payments && cd payments && polyph join A1B2-CDEF --name Bo`.
   Polyph offers to clone the room's repository with their own git login.
4. The host lets them in: `polyph room knocks`, then
   `polyph approve <id>` for each person (or `--role maintainer`).
5. Each teammate checks their setup: `polyph doctor`, then
   `polyph connect` to bring in the agents they already use.
6. Plan a goal: `polyph plan new "Add refunds to the billing API"`, read
   the draft cards and overlaps it prints, then
   `polyph plan accept <plan>`.
7. Start agents on cards: `polyph board` shows each card's id;
   `polyph agent start claude --task <card>`. Agents connected with
   `polyph connect` take the next ready card with their `next_task` tool.
8. Watch and steer in the terminal UI (`polyph`) or the desktop app. Claims
   show with `polyph claims`; approve commands and merges as they come.
9. When an agent is done: `polyph task finish <agent>`. The reviewer agent
   reads the change, then the merge queue merges it; `polyph merge` shows
   the queue.
10. Share merged work through the team's remote: set `push = true` under
    `[merge]` in `polyph.toml`; teammates run `polyph sync` to move their
    checkout forward.
11. Keep work going with laptops closed: on a server,
    `polyph agent-host join ...` with the enrollment from
    `polyph agent-host invite`, then
    `polyph agent start claude --on <host> --task <card>`.
12. Keep an eye on spend: `polyph usage`, and
    `polyph budget set --tokens 2000000` for the room (or `--member <name>`
    for one person).

## 2. Students and hackathon teams

Two to five people, often on different networks or one campus network, who
need to be working within minutes. Some have no paid agent subscription.

1. One person makes the repository and pushes it to GitHub (or any remote
   the team can clone).
2. They run `polyph start` in it and paste the join code into the team
   chat. For a one-off event, `polyph host --quick` makes a throwaway
   session that is deleted when it ends.
3. Each teammate installs and joins in an empty folder:
   `mkdir hack && cd hack && polyph join A1B2-CDEF --name Mai --clone`.
   Target: under two minutes from receiving the code to being in the room.
4. A teammate with only a browser opens the link that `polyph room invite`
   prints. It opens the phone app on the website with the code filled in;
   no install.
5. The host approves each knock: `polyph room knocks`,
   `polyph approve <id>`. To skip approval for a short event:
   `polyph room settings --require-approval false`.
6. Teammates without a subscription can run a local model:
   `polyph agent start local --task <card>` (point it at an
   OpenAI-compatible endpoint such as Ollama). See `docs/beta.md` for what
   the local agent cannot do yet.
7. Split the work: `polyph plan new "<the hackathon idea>"`,
   `polyph plan accept <plan>`, then one agent per card as in the startup
   journey.
8. On a network without internet, host on the LAN instead:
   `polyph host --lan`, and teammates `polyph join A1B2-CDEF --lan`
   (or `polyph discover` to find the room).
9. At the end, save the board and chat into the repository:
   `polyph end --export .`

## 3. Open source maintainers

A maintainer and regular contributors, each with their own agents, on one
public repository, with the community watching.

1. In the repository: `polyph start --name <project>`.
2. Describe the code areas so the planner and `next_task` route work to the
   right people: Polyph reads `.polyph/AREAS` or `CODEOWNERS`. Check it with
   `polyph areas` and `polyph areas src/parser/lexer.rs`.
3. Set the brief from the contributing guide:
   `polyph brief set CONTRIBUTING.md`.
4. Make a code for contributors that lasts a week:
   `polyph room invite --reusable --expires 10080`. Joining stays behind
   approval.
5. Approve contributors as Members (`polyph approve <id>`) and community
   watchers as Viewers (`polyph approve <id> --role viewer`). Spectator
   links are not in the beta.
6. Contributors join in an empty folder (`polyph join <code>`), which
   clones the public repository, and connect their own agents
   (`polyph connect`).
7. Turn issues into cards: `polyph github connect` (paste a token), then
   `polyph github import --label "good first issue"` brings open issues in
   as backlog cards. Pull requests for tasks are checked about every minute.
8. Late joiners catch up with `polyph catchup` and `polyph search <words>`.
9. Change roles as people come and go: `polyph room role <member> member`,
   `polyph room remove <member>`. A removed device cannot join that room
   again; to let the person back in, they join from another machine or with
   a separate `POLYPH_HOME`.

## 4. Agencies and freelancers

A small team working on a client's repository, with the client watching
progress.

1. In the client repository: `polyph start --name <client>-site`.
2. Apply a template if one fits: `polyph template list`,
   `polyph template apply <name>`.
3. Teammates join and connect their agents as in the startup journey.
4. Invite the client as a Viewer: `polyph room invite` prints a link that
   opens in a browser with no install. When they knock,
   `polyph approve <id> --role viewer`. Viewers see the board, chat and
   agents' progress; they cannot start agents or approve anything.
5. Plan the milestone with the client watching:
   `polyph plan new "<milestone>"`, `polyph plan accept <plan>`.
6. Run overnight work on your own server with `polyph agent-host` (see the
   startup journey, step 11).
7. If the client uses Linear: `polyph linear connect` (paste an API key),
   then `polyph linear import <TEAM>`; their issues follow the cards.
8. At handoff, export the board and chat into the repository
   (`polyph end --export .`) or keep the room open for the next phase.

## First ten minutes (timed script)

The product reviewer runs this before each beta release, on three computers
on three different networks (for example two home connections and a phone
hotspot), with a stopwatch per person. The beta goals are: a student joins
in under two minutes, and a team of three gets its first task merged in
under ten minutes.

### Before the clock starts

- Three people: Ann (host), Bo and Cy. Ann and Bo on Linux (Bazzite counts),
  Cy on any platform.
- Each has git, a GitHub login that can clone the test repository, and
  Claude Code signed in with its ACP adapter installed
  (`polyph agents doctor` shows it ready).
- Polyph is not installed on any of the three computers
  (`polyph uninstall --purge -y` if it was).
- The test repository has a small web app and one passing test.

### Steps and target times

| # | Who | Do | Done when | Target | Total by |
|---|---|---|---|---|---|
| 1 | All three, at once | Run the install line, then `polyph --version` | The version prints | 1:00 | 1:00 |
| 2 | Ann | `cd <repo> && polyph start`; type her name; accept connecting agents | It prints `Teammates join with: polyph join <code>` | 1:00 | 2:00 |
| 3 | Bo, Cy | `mkdir app && cd app && polyph join <code> --name Bo --clone` | `Waiting for someone in the room to approve you…` | 0:30 | 2:30 |
| 4 | Ann | `polyph room knocks`, then `polyph approve <id>` for each | Bo and Cy see the room; the repository is cloned | 0:30 | 3:00 |
| 5 | All three | `polyph doctor` | `Everything looks right.` | 0:30 | 3:30 |
| 6 | Ann | `polyph plan new "Add a /health endpoint that returns the version, with a test"`, then `polyph plan accept <plan>` | Cards are in Ready on everyone's `polyph board` | 2:00 | 5:30 |
| 7 | Bo | `polyph board`, then `polyph agent start claude --task <card>` | `polyph claims` on Ann's and Cy's computers shows Bo's agent's claim | 0:30 | 6:00 |
| 8 | Bo's agent | Writes the change and its test | The agent says it is done | 2:30 | 8:30 |
| 9 | Bo | `polyph task finish <agent>` | `Committed … and queued it to merge …` | 0:15 | 8:45 |
| 10 | Bo, Ann | The reviewer agent reviews; Ann approves the merge in the terminal UI (`polyph`) | `polyph merge` shows the task merged | 1:15 | 10:00 |

Student join (steps 1, 3 and 4 for one person, from receiving the code to
seeing the room) must take under 2:00. Repeat it once with the browser link
from `polyph room invite` instead of installing.

### What to record

- The time for each step and the total; mark any step over its target.
- Every message that made someone stop and think, word for word.
- Every command someone had to look up or guess, and every error.
- Which computer and network each person used, and the Polyph version.

A run passes when the total is under 10:00, the student join is under 2:00,
and nobody needed help from outside this page. File each problem as a
GitHub issue with the `first-hour` label and the exact steps.

# Show HN draft

Draft. Posting is human-only (Kao). Check the links and the release tag on
the day.

## Title

Show HN: Polyph, a shared room for your team's coding agents (public beta)

## URL

https://projectdev-git.github.io/Polyph

## Text

I built Polyph because on teams that use coding agents, two agents end up
editing the same file and nobody knows what the other person's agent is
doing.

Each person runs their own agents (Claude Code, Codex, OpenCode, Gemini CLI,
anything that speaks ACP, or a local model) on their own machine, with their
own subscription or keys. Polyph puts them in one room. Agents claim files,
folders or single functions before they edit, so a second agent is told who
holds a file and works elsewhere. A planner splits a goal into cards, each
task runs in its own git worktree, a reviewer agent checks it, and a merge
queue lands it. When two tasks changed the same code, the merge tries to keep
what both were trying to do.

The room is end-to-end encrypted. The relay forwards ciphertext and cannot
read code, chat, the board or terminal output. It does see room ids, device
public keys, message sizes and IP addresses; the list is in the beta page.
You can run the relay yourself as one binary with SQLite. In a load test
with 50 rooms of 5 people (2 messages per second per member) it peaked at
71 MB of memory, and 118 MB in a worst-case run; the method and a
reproduction command are in docs/self-hosting.md. There is also a LAN mode
that needs no internet.

Joining is an eight-character code plus approval from an owner. Approvals for
agent commands show the command, files and hosts. The same room state is
available in a CLI and terminal UI, a desktop app, a browser client, a phone
app (installable from the browser) and a VS Code extension.

Try it, in a git repository:

    curl -fsSL https://raw.githubusercontent.com/ProjectDEV-git/Polyph/Main/scripts/install.sh | sh
    polyph start
    # a teammate, on another network:
    polyph join A1B2-CDEF

The engine and apps are Apache-2.0; the hosted account service is
proprietary, and it is not part of the beta.

What does not work yet, so you do not find out the hard way:

- Linux x86_64 is where we test by hand. macOS and Windows builds exist and
  pass smoke tests in CI, but nobody has used them extensively. They are
  experimental, and unsigned.
- Someone who joins late does not see agents' earlier transcripts.
- Merges happen on the computer that ran the task. Teammates get the result
  through your git remote.
- Function-level claims find functions by pattern, not a parser. Unusual
  formatting can turn one into a whole-file claim.
- The built-in local-model agent gets no Polyph tools yet.
- No spectator links, no push notifications to a closed phone app, no
  accounts or billing in the beta.
- The beta relay is a single machine on Fly.io in Singapore. You can point
  Polyph at your own.

The full list is in docs/beta.md. I have no users to quote, no benchmarks
beyond the relay load test, and the whole thing is young. I would like to
hear where the first ten minutes go wrong for you.

Links:
- Beta page and known limits: https://github.com/ProjectDEV-git/Polyph/blob/Main/docs/beta.md
- Site: https://projectdev-git.github.io/Polyph
- Releases: https://github.com/ProjectDEV-git/Polyph/releases

## First comment (post right after submitting)

Some context on the design choices people usually ask about.

Claims are advisory locks held in the room's state, not file-system locks.
They stop collisions between agents that go through Polyph, which includes
agents started by Polyph and agents you connect with `polyph connect`. An
agent that ignores Polyph and writes to the checkout directly is not
stopped. Each task has its own worktree and branch by default, which is the
main protection; claims tell the other agents and people early instead of at
merge time.

The relay never holds keys. Joining needs the whole code and an owner's
approval. The threat model, including what is not protected, is in
docs/security.md.

If something breaks in the first run, `polyph doctor` prints a fix for each
problem it finds, and issues go to
https://github.com/ProjectDEV-git/Polyph/issues.

## Likely questions, with answers grounded in the repo

- Why not just git branches? Branches keep the changes apart but do not tell
  an agent, at the moment it starts, that a teammate's agent is already in
  that file. Claims do that. Branches and a merge queue are still used.
- Does it send my code to you? Room content is encrypted on your devices. Model calls go from your
  machine to your model provider; Polyph never proxies them and sends no
  telemetry.
- What does it cost? Nothing in the beta, and no prices are set. The website
  says "Pricing coming soon".
- How does it differ from Amoeba or Murmell? Answer only from
  `positioning.md`, and only what is verified there.

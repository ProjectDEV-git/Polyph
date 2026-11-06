# Big projects

## Taking tasks

`next_task` (MCP tool) gives an agent the next card in `ready` that is not a
draft, has no assignee, whose dependencies are done and whose predicted files
nobody else holds. Cards in the agent's code areas come first, then board
order. No two agents get the same card, on any device: taking a card goes
through the room's claim arbiter (the acting host), which grants the card's
lease (`.polyph/tasks/<card>`) to one agent only; the others move on to the
next candidate. `release_files` without paths keeps that lease;
`finish_task` releases it.

## Code areas

`.polyph/AREAS` uses the same form as a CODEOWNERS file (and Polyph reads
`.github/CODEOWNERS`, `CODEOWNERS` or `docs/CODEOWNERS` when it is
missing): a pattern, then owners (people's names, with or without `@`, or
agent labels), then an optional `# note`. The last matching line wins.

```text
src/billing/   @ann Ann/claude   # money: every change needs a test
*.sql          @bo
```

Areas are advice, not locks. `get_context` and `claim_files` say whose area a
file is in, `next_task` prefers tasks in the caller's areas, and the planner
is asked to split work along areas and suggests their owners. Use
`polyph areas [path]` to see them.

## Scale test

`crates/polyph-testkit/tests/scale.rs`: three devices, 30 connected agents,
a 100,000-file git repository with 20 areas, and 150 cards (three files each;
every tenth shares a file with the previous card; a third depend on earlier
cards). Each agent loops `next_task`, `get_context`, `claim_files` (waiting
while another agent holds a file), `remember`, `finish_task`; a stand-in for
the merge queue moves finished cards to done. The test checks that each
card went to exactly one agent, that no file was granted to two agents at
once, and that every card was done.

The size is set by `POLYPH_SCALE_FILES`, `POLYPH_SCALE_AGENTS` and
`POLYPH_SCALE_CARDS`. CI runs 10,000 files, 9 agents and 45 cards. Results
(debug build, 4-core Linux container):

```text
files 10000, agents 9, cards 45:   all cards done in 45 s, no claim call failed
files 20000, agents 12, cards 60:  all cards done in 90 s, 2 claim calls retried
files 100000, agents 30, cards 150: does not finish; see below
```

At the full size, all three daemons share one process and a few cores in a
debug build. In one run, after about 120 cards the claim arbiter stopped
answering the other two devices, most likely because the overloaded host's
heartbeats fell behind and the devices no longer agreed on who the acting
host was. This has not been measured with one device per machine.

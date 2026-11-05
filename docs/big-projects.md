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
`POLYPH_SCALE_CARDS`. Results (debug build, 4-core Linux container):

```text
RESULTS
```

# Polyph for VS Code

Your Polyph room inside VS Code, on the daemon already running on your
computer (start it with `polyph`, the desktop app or `polyph daemon start`).

- **Claims:** files another voice holds get a gutter line in that voice's
  color and a badge in the explorer; hover for who holds it and why.
- **Approvals:** when one of your agents asks, a prompt says exactly what it
  wants to do, with Approve once and Deny. `Polyph: Answer approvals` lists
  them all.
- **Status bar:** the room, agents running and approvals waiting for you.
- **Panel:** `Polyph: Open` shows the whole app (chat, board, agents,
  activity, merges, terminals, whiteboards) beside your code.

Settings: `polyph.home` (the Polyph data directory, if not the default),
`polyph.claimDecorations`, `polyph.approvalPrompts`.

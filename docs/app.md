# App brief: desktop, browser client, status pill

The desktop app (Tauri 2), the browser client and the status pill are one
React app in `packages/ui`, talking to the person's own daemon over the local
API (JSON-RPC over a localhost WebSocket). They show the same state as the CLI
and the TUI (PRODUCT.md principle 4); only the shell around them differs.

## Who and when

An engineer who keeps Polyph open beside their editor all day, glancing at it
between edits, sometimes for an hour of planning with the team. Dense,
glanceable and quiet beats spacious (DESIGN.md dials: variance low, motion
low, density high).

**The screen's one question:** what are the voices in this room doing, and
what needs me? The approval waiting for this person is the most frequent job
and is always one click away, from every view, the pill and a native
notification.

**Its face:** the voices. Every person's name, dot and gutter is their hue;
their agents are lighter and darker steps of it. The rail lists people with
their agents nested under them, so a glance reads as "Ann and her two agents
are running, Bo's agent is waiting". Chrome stays neutral ink; colour is on
voices and status only.

**The signature element:** the voice stack in the rail: each person a row
with a hue dot, each of their agents an indented row in the person's lighter
or darker step with a status mark and word (running, waiting for approval,
blocked, error, done). It is the same data the pill compresses into one line.

## Shell

Ruled frame on one ground (dark default, designed light theme):

```
┌ rail 248 ───────┬ main ─────────────────────────────────┬ needs you 320 ┐
│ ≡ Polyph        │ payments (serif)       Chat Board ... │ Approvals      │
│ [room switcher] │                                       │  stack         │
│ Views           │  current view                         │ People online  │
│ People + agents │                                       │                │
│ ─────           │                                       │                │
│ Settings        │  composer (chat, or prompt an agent)  │                │
└─────────────────┴───────────────────────────────────────┴────────────────┘
```

- **Rail:** the mark and "Polyph", the room switcher (name in the serif),
  the views (Chat, Board, Agents, Activity, Merges, Terminals, Whiteboards),
  the voice stack, then Settings at the bottom. Active view: a neutral
  surface-3 pill, never an accent bar. Counts only on queues that need action
  ("Merges 2" when merges wait).
- **Title row:** the room name as the h1 in Newsreader (DESIGN: serif for room
  names only), a one-line finding beneath it in `--text-2` ("Ann/claude waits
  for your approval to run cargo publish", or "3 agents running, nothing
  needs you").
- **Right column, "Needs you":** the approval stack, each card stating
  exactly what will happen (command and directory, files, host and port,
  branch and target) with "Approve once", "Approve for this task" and "Deny
  with note". It floats (the one shadow in the app). Under it, who is online.
  Below 1100px it folds into a button with the count in the title row.
- **Phone width:** the rail becomes a top bar with the room name and a menu
  sheet; the approvals become a one-line summary row above the view.

## Views

| View | Question | Main block |
|---|---|---|
| Chat | What did people and agents say? | Room chat with voice names, threads list beside it |
| Board | Where is each task? | Six columns, cards with assignee voice, PR and branch |
| Agents | What is each agent doing right now? | Agent list, live transcript with tool calls and diffs; prompt box |
| Activity | What happened, in order? | The activity log, with a replay slider that shows the board at a time |
| Merges | What is about to land? | The merge queue, state per entry |
| Terminals | What is running in the shared terminals? | xterm.js, attach, ask for the keyboard |
| Whiteboards | What did we sketch? | Excalidraw, synced element by element |
| Settings | How is this device and room set up? | Integrations, usage and budgets, language and theme |

The command palette (Ctrl/Cmd K) reaches every view, room and action.

## States

- **Connecting / daemon unreachable:** the shell's frame with one sentence in
  the main area and a Retry; never a blank page.
- **No room:** the serif empty-state line with the two real actions (host a
  room, join with a code).
- **Empty views** keep their frame and say what to do, in the product's words
  (from the same strings as the TUI where they match).
- **Loading:** labels at once, values as flat bars at their real size; no
  shimmer.
- **Errors** sit inside the block that failed, with what failed and Retry.

## The pill

A 360×44 always-on-top window (desktop) and a compact route in the browser:
the mark, the room name, one status mark per running agent in its voice, and
the waiting approvals as "! 1 approval". Clicking the count opens the first
approval inline with Approve once and Deny; anything else opens the app. It
uses the same `status` feed as `polyph status --watch`.

## Type, colour, motion

DESIGN.md tokens as CSS variables (OKLCH), mapped into Tailwind v4's theme.
IBM Plex Sans 13px UI, IBM Plex Mono for code, paths, join codes and figures,
Newsreader only for room names and empty-state headlines. Lucide icons at
16px, 1.5px stroke. Motion only where DESIGN.md allows it: a card changing
column, a panel opening, an approval arriving, a voice joining; everything
else changes in place, and `prefers-reduced-motion` turns transforms into
opacity.

## Connecting

- **Desktop:** the Tauri shell starts the daemon if it is not running, reads
  its port and API token, and hands them to the webview. Native menus, tray,
  notifications from `notification` events, and the pill window.
- **Browser client:** `polyph web` asks the daemon for a one-time code and
  opens `http://127.0.0.1:<port>/app/#code=…`. The daemon serves the client's
  static files and trades the code for a session token, so the API token never
  appears in a URL or browser history. The WebSocket checks the page's origin.

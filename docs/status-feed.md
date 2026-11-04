# Status feed

One feed of what is happening in your current room, for anything that wants
to show it at a glance: the status pill, panel and bar widgets, Nublet and
other ambient displays, scripts. It carries **states, names, counts and voice
colors only** — never code, chat, file contents or terminal output — so it is
safe to show on a screen others can see.

The schema lives in `crates/polyph-proto/src/status.rs` and is versioned by
`v` (currently 1). New fields may appear; consumers must ignore fields they do
not know.

## Lines

Every line is a JSON object with `v`, `seq` (increasing per stream), `at`
(unix seconds) and `type`:

| `type` | Fields | Meaning |
|---|---|---|
| `snapshot` | `room`, `room_name`, `connected`, `members_online`, `agents[]`, `approvals[]`, `budgets[]` | The whole state; always first |
| `agent_started` | `agent` | An agent started |
| `agent_state` | `id`, `state`, `task` | `running`, `waiting_approval`, `blocked`, `error`, `done`, `idle` |
| `agent_stopped` | `id` | |
| `approval_pending` | `approval` (`id`, `agent`, `title`, `mine`, `created_at`) | `mine` is true when it is yours to answer |
| `approval_resolved` | `id`, `approved` | |
| `claims_changed` | `count` | Files currently claimed |
| `task_moved` | `task`, `title`, `column` | A card changed column |
| `budget_warning` | `budget`, `percent` | |
| `connection` | `connected` | The room's relay connection |

Agents carry `label` (`Ann/claude`), `owner`, `color` (the agent's voice as
CSS OKLCH), `state`, `task` (card title), `mine` and `cloud`.

## Where to read it

| Way | Use | Access |
|---|---|---|
| `polyph status --watch --json` | Scripts | Local user |
| D-Bus `org.polyph.Status1` (Linux) | Panels (Waybar, GNOME, KDE) | Session bus, read-only |
| HTTP status provider (below) | Widgets, Nublet, dashboards, other devices' apps on this machine | Read-only status token |
| The local API (`status.watch`) | Polyph's own apps | API token |

## HTTP status provider

The daemon serves the feed on its local port to anything holding the
**status token**, a read-only secret separate from the API token:

```bash
polyph status --token          # prints the token and the widget URL
polyph status --rotate-token   # replaces it; widgets using the old one stop
```

| Route | Returns |
|---|---|
| `GET /status` | The current snapshot as one feed line |
| `GET /status/events` | Server-Sent Events: the snapshot, then each line as it happens (`event:` is the line's `type`) |
| `GET /status/widget` | A ready-to-draw summary in your language (below) |

Pass the token as `?token=` or `Authorization: Bearer`. The token opens these
three routes and nothing else; requests must name a loopback host
(`127.0.0.1`, `localhost`), which keeps other machines and DNS-rebinding pages
out. Responses allow any origin to read them (`Access-Control-Allow-Origin:
*`), since a page still needs the token. Listening stays on `127.0.0.1`: a
display on another machine reaches it through a forwarder you run, such as an
SSH tunnel.

### The widget document

`/status/widget` is for displays that should not interpret the feed
themselves. Its texts follow your Polyph language setting.

```json
{
  "v": 1,
  "title": "payments",
  "subtitle": "Ann/claude waits for you: Run cargo publish",
  "state": "waiting",
  "badge": { "count": 1, "text": "1 approval" },
  "online": 3,
  "items": [
    { "label": "Ann/claude", "state": "waiting", "symbol": "!", "word": "waiting for approval",
      "color": "oklch(0.840 0.120 255)", "detail": "Add refunds", "mine": true }
  ],
  "updated_at": 1793812621
}
```

- `state` is the most important thing on screen, in this order: `offline`,
  `waiting` (something needs you), `running`, `idle`.
- `symbol` is one cell wide and readable without color (the TUI's symbols);
  `word` is the state in words. Never show state by color alone.
- `color` is the agent's voice; use it for the label, not as a fill.
- `badge` is null when nothing waits for you.

## Nublet

Nublet reads Polyph through the HTTP status provider: point it at
`http://127.0.0.1:<port>/status/widget?token=<status token>` (the URL
`polyph status --token` prints) for a polled summary, or at
`/status/events` for live updates. See `integrations/nublet/`.

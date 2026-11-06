# Nublet provider

[Nublet](https://github.com/kaorii-ako/nublet) is a GNOME Shell extension
that turns the top bar clock into a pill showing what needs you. Its Polyph
tab shows your open room: how many agents are running, waiting for approval
or blocked, each pending approval, and each busy agent. An approval waiting
raises the pill; clicking it opens Polyph. The code lives in Nublet
(branch `polyph-provider`: `providers.js`, `polyph.js`, `polyphParse.js`,
`polyph.test.js`). Nothing in Polyph changed for it.

## How it reads Polyph

Nublet reads the D-Bus status service the daemon already serves on Linux
(`crates/polyph-daemon/src/dbus.rs`). It adds no channel and needs no API
token.

1. It watches the bus name `org.polyph.Status1` on the session bus. While
   no one owns the name, the tab says Polyph is not running and the pill
   shows nothing from Polyph. Nublet never starts the daemon.
2. When the name appears it subscribes to the `Event(s)` signal on
   `/org/polyph/Status`, then calls `Snapshot() -> s`. Events that arrive
   before the snapshot reply are queued and applied after it.
3. Each `Event` argument is one status feed line, the same JSON as
   `polyph status --watch` (`polyph-proto::status`, `v: 1`). Nublet uses
   `snapshot`, `agent_started`, `agent_state`, `agent_stopped`,
   `approval_pending`, `approval_resolved` and `connection`, and skips
   other types and other feed versions.
4. When the name goes away it drops the state and goes quiet again.

It does not read the `RoomName`, `Connected`, `AgentsRunning` or
`ApprovalsPending` properties: `AgentsRunning` counts running and waiting
agents together, and Nublet shows them apart.

What it shows, from the feed's agents and approvals:

| Pill | When | Priority in Nublet |
|---|---|---|
| shield icon, amber, `room · Run cargo test` or `room · 2 approvals` | an approval is pending or an agent is `waiting_approval` | 95, just below a Claude Code permission (100), which Nublet can answer in place |
| barrier icon, orange, `room · 1 blocked` | an agent is `blocked` | 65 |
| a dim dot beside the clock, with a count when more than one | agents `running` or `starting` | 10 |

Status color is always paired with an icon and a word, as in DESIGN.md.
The tab lists "Connected · 2 running · 1 waiting for approval · 1 blocked",
then the pending approvals (title, agent, "Open in Polyph"), then agents
that are waiting, blocked, failed, running or starting, with their state
word and task.

Clicking an approval runs `polyph web`, which opens the browser client with
a one-time sign-in link. Nublet finds `polyph` on `PATH` or in
`~/.local/bin`, `/usr/local/bin` or `/usr/bin`. It does not approve or
deny: the D-Bus service is read-only on purpose, and the person decides
with the request's full context in Polyph. Approve and Deny buttons in the
pill could run `polyph approve <id>` and `polyph deny <id>`; that is a
product and security decision for the project owner, not part of this
provider.

## Try it

On a GNOME Shell 46 to 50 desktop:

```bash
# Polyph, started from a terminal in the desktop session so it sees the session bus
polyph daemon start
busctl --user introspect org.polyph.Status1 /org/polyph/Status   # the service is up
polyph status --watch                                             # the same lines Nublet reads

# Nublet, on its polyph-provider branch
git clone -b polyph-provider https://github.com/kaorii-ako/nublet ~/projects/nublet
cd ~/projects/nublet && glib-compile-schemas schemas/
ln -sfn ~/projects/nublet ~/.local/share/gnome-shell/extensions/nublet@kaorii-ako
```

Log out and in once, enable Nublet, and turn on the Polyph tab in Nublet's
settings under Presets. Open a room and start an agent
(`polyph agent start claude --task "..."`) on work that asks for an
approval, such as running a command. The pill should show
the room name and the approval title; clicking the approval in the tab
opens the browser client. Stop the daemon (`polyph daemon stop`) and the
tab should say Polyph is not running, with nothing in the pill and nothing
in `journalctl -o cat /usr/bin/gnome-shell`.

Nublet's own nested shell (`./dev/nested.sh`) runs a separate session bus,
so it does not see a daemon started on the host bus; start the daemon
inside it or test in the real session.

The parser checks run without GNOME: `node polyph.test.js` or
`gjs -m polyph.test.js` in the Nublet checkout.

## Gaps on the Polyph side

None of these change the wire format today. Each is a proposal for the
lead; the ones that touch `polyph-proto::status` need the architect
reviewer.

1. **No way to open one approval.** The browser client reads only
   `#code=` from its address (`apps/web/src/client.tsx`), the desktop app
   registers no URL scheme, and `polyph web` takes no approval id.
   `polyph approve <id>` and `polyph deny <id>` exist, but they decide
   rather than show. Nublet opens the browser client and the person finds
   the approval in the approvals view. Proposed: `polyph web --approval <id>`
   appends `&approval=<id>` to the sign-in link, and the app selects that
   approval once signed in. This is CLI and UI only; the status feed and
   D-Bus stay as they are.
2. **One room only.** `Daemon::status()` describes the room opened last
   (`open.last()`), and the feed translator looks agents and approvals up
   in that snapshot, so `agent_started` and `approval_pending` from another
   open room are dropped while `agent_stopped` and `approval_resolved` from
   it pass through. Nublet therefore shows counts for one room, not per
   room. Proposed (additive, feed stays `v: 1`): a `room` field on agent
   and approval events and a `Rooms() -> s` D-Bus method returning one
   snapshot per open room.
3. **Approvals name their agent by label.** `StatusApproval.agent` is the
   agent's label, not its id, so Nublet matches approvals to agents by
   label, which is ambiguous when two agents share one. Proposed
   (additive): `agent_id` on `StatusApproval`.
4. **No reason for blocked.** The daemon sets `blocked` with a reason
   (`budget`), but `agent_state` carries only state and task, so Nublet
   says "1 blocked" without why. Proposed (additive): optional `reason` on
   `agent_state` and `StatusAgent`.
5. **D-Bus depends on how the daemon started.** The service starts only
   when `DBUS_SESSION_BUS_ADDRESS` is in the daemon's environment. A daemon
   started over SSH, from a TTY or by a service manager without it has no
   D-Bus status, and Nublet shows "Polyph is not running" although
   `polyph status` works. Proposed: `polyph daemon status` says whether the
   D-Bus service is up; the docs say to start the daemon from the desktop
   session.
6. **`polyph web` needs the browser client installed.** With only the
   desktop app (its `polyph` sidecar is not on `PATH`) the click does
   nothing. Proposed: when an approval link exists (gap 1), the desktop
   app handles it too.

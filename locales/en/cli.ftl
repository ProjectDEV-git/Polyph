# Command help
help-root = Multiplayer coding for teams of humans and their agents.
help-daemon = Manage the background daemon.
help-daemon-run = Run the daemon in the foreground.
help-daemon-start = Start the daemon in the background.
help-daemon-stop = Stop the daemon.
help-daemon-status = Show whether the daemon is running.
help-daemon-logs = Print the daemon log.
help-whoami = Show this device, your name and your relay.
help-room = Create, open and manage rooms.
help-room-create = Create a persistent room.
help-room-open = Open a room and connect to it.
help-room-close = Close a room on this device (it stays in the room list).
help-room-list = List your rooms.
help-room-invite = Create a join code for a room.
help-room-members = List a room's members.
help-room-settings = Show or change room settings.
help-room-delete = Delete a room from this device.
help-room-role = Change a member's role.
help-room-remove = Remove a member and rotate the room key.
help-room-knocks = List people waiting to join.
help-host = Create a room (or a quick session) and print a join code.
help-join = Join a room with a code.
help-discover = Find rooms on the local network.
help-leave = Leave the current room on this device.
help-end = End a quick session or a room you own, deleting it for everyone.
help-chat = Send a chat message.
help-approve = Approve an agent's request or someone waiting to join.
help-deny = Deny an agent's request or someone waiting to join.
help-claims = List current file claims.
help-board = Show the board.
help-status = Show room status; with --watch, stream the status feed.
help-config = Read or change settings.
help-config-get = Print a setting.
help-config-set = Change a setting.
help-config-edit = Open the settings file in your editor.
help-relay = Run a relay.
help-relay-serve = Run a relay in the foreground (self-hosting, LAN, tests).
help-completions = Print shell completions.

# Daemon
daemon-starting = Starting the Polyph daemon…
daemon-started = Daemon running on port { $port }.
daemon-not-running = The daemon is not running. Start it with `polyph daemon start`.
daemon-running = Daemon running (version { $version }, port { $port }).
daemon-stopped = Daemon stopped.
daemon-start-failed = The daemon did not start. See the log with `polyph daemon logs`.
daemon-no-log = No daemon log yet.
api-unreachable = Cannot reach the Polyph daemon: { $reason }
api-error = { $message }

# Identity
whoami-device = Device: { $device }
whoami-name = Name: { $name }
whoami-member = Member: { $member }
whoami-relay = Relay: { $relay }
whoami-keystore = Keys stored in: { $store }
name-unset = (not set — run `polyph config set name "Your Name"`)

# Rooms
room-created = Created room { $name }.
quick-created = Started a quick session.
room-opened = Opened { $name }.
room-closed = Closed { $name } on this device.
room-deleted = Deleted { $name } from this device.
room-ended = Ended { $name }. It is gone for everyone.
room-exported = Saved the board and chat to { $path }.
rooms-empty = No rooms yet. Create one with `polyph room create <name>` or join one with `polyph join <code>`.
room-line = { $name }  { $role }  { $state }  { $online } online
room-state-connected = connected
room-state-connecting = connecting
room-state-closed = closed
quick-session-name = quick session
no-current-room = No room is open. Open one with `polyph room open <name>`.
invite-code = Join code: { $code }
invite-link = Link: { $link }
invite-expires = Expires { $when }.
invite-one-time = One person can use this code.
invite-reusable = This code can be used more than once.
invite-pending = The room is not connected yet; the code will work once it connects.
invite-lan = On this network: polyph join { $code } --relay { $url }
members-header = Members of { $room }
member-line = { $name }  { $role }  { $state }
member-online = online
member-offline = offline
member-you = (you)
knocks-empty = Nobody is waiting to join.
knock-line = { $id }  { $name } wants to join
knock-hint = Approve with `polyph approve { $id }` or deny with `polyph deny { $id }`.
settings-approval-on = Joining requires approval.
settings-approval-off = Anyone with a code joins without approval.
role-changed = Changed the role.
member-removed = Removed the member and rotated the room key.

# Joining
join-connecting = Connecting…
join-waiting = Waiting for someone in the room to approve you…
joined = Joined { $name } as { $role }.
discover-searching = Looking for rooms on this network…
discover-empty = No rooms found on this network.
discover-line = { $name }  { $url }
discover-hint = Join with `polyph join <code> --relay <url>`.

# Chat, approvals, claims, board
chat-sent = Sent.
approved = Approved.
denied = Denied.
admitted = Let them in.
not-admitted = Turned them away.
nothing-matches = Nothing waiting matches { $id }.
claims-empty = No files are claimed.
claim-line = { $target }  { $agent }  { $reason }
board-empty-column = (empty)

# Status
status-room = Room: { $name }
status-connected = Connected to the relay.
status-disconnected = Not connected to the relay.
status-members = { $count ->
    [one] 1 person online
   *[other] { $count } people online
}
status-approvals = { $count ->
    [0] No approvals waiting.
    [one] 1 approval waiting.
   *[other] { $count } approvals waiting.
}

# Config and relay
config-set = Saved.
config-unknown = Unknown setting.
relay-listening = Relay listening on { $addr }.
editor-failed = Could not open an editor. The settings file is { $path }.

when-minutes = { $n ->
    [one] in 1 minute
   *[other] in { $n } minutes
}
when-hours = { $n ->
    [one] in 1 hour
   *[other] in { $n } hours
}
when-days = { $n ->
    [one] in 1 day
   *[other] in { $n } days
}

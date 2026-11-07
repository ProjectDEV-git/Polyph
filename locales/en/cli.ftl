# Command help
help-root = Multiplayer coding for teams of humans and their agents.
help-daemon = Manage the background daemon.
help-daemon-run = Run the daemon in the foreground.
help-daemon-start = Start the daemon in the background.
help-daemon-stop = Stop the daemon.
help-daemon-status = Show whether the daemon is running.
help-daemon-logs = Print the daemon log.
help-whoami = Show this device, your name and your relay.
help-web = Open the browser client for this computer’s daemon.
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
invite-link = Join from a browser: { $link }
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

help-agent = Start, follow and stop agents.
help-agent-start = Start an agent in the current directory.
help-agent-list = List agents in the room.
help-agent-stop = Stop one of your agents.
help-agent-follow = Follow an agent's transcript live.
help-agent-prompt = Send one of your agents a prompt.
help-agents = Check installed agents.
help-agents-doctor = Check which agents are installed and logged in, and how to fix problems.
help-sandbox = Agent sandbox tools.
help-sandbox-test = Try forbidden writes and network calls and report what is blocked.
help-mcp = Polyph's MCP server for agents (claims, team state, messages, notes, context). Agents Polyph starts run it themselves; `polyph connect` sets up the agents you run.
help-usage = Show token usage and cost estimates.
help-budget = Manage budgets.
help-budget-set = Set a budget for the room or one member.

agent-started = Started { $label } (sandbox: { $sandbox }).
agent-stopped = Stopped.
agent-untrusted-project = Ignored { $ignored } in { $file }: this file is not trusted on this device, so the agent runs with the default sandbox and built-in agent definitions. Review the file, then run `polyph trust` in { $dir } to apply it.
help-trust = Trust this repository's polyph.toml on this device, so its sandbox, agent, check, merge and price settings apply.
trust-saved = Trusted { $file } on this device. Its sandbox, agent, check, policy preset, merge approval and check, and price settings now apply. After any edit to the file, run `polyph trust` again.
trust-already = { $file } is already trusted on this device.
trust-hash = SHA-256: { $hash }
trust-no-file = No polyph.toml in { $dir } or its parent folders.
trust-revoked = Stopped trusting the polyph.toml in { $dir }. Its sandbox, agent, check, policy preset, merge approval and check, and price settings are ignored until you run `polyph trust` again.
trust-revoke-none = Nothing to revoke: no polyph.toml in { $dir } is trusted on this device.
trust-review = Trusting { $file } applies these settings on this device:
trust-review-none = { $file } sets no sandbox, agent, check, policy preset, merge approval and check, or price settings.
trust-confirm = Trust this file? [y/N]
trust-cancelled = Not trusted. Nothing changed.
trust-needs-yes = Trusting a project file needs confirmation; run it in a terminal, or add --yes after reviewing the file.
agents-empty = No agents are running in this room.
agent-line = { $label }  { $state }
agent-prompted = { $label } finished ({ $reason }).
follow-prompt = { $who } → { $text }
follow-state = { $label } is { $state }.
doctor-ok = { $name }: ready
doctor-missing = { $name }: not installed
doctor-logged-out = { $name }: installed, not logged in
doctor-install-npm = Install with: { $cmd }
doctor-install-local = Install with: { $cmd }
doctor-login = Log in with: { $cmd }
doctor-sandbox = Sandbox: { $mechanism } — { $detail }
sandbox-mechanism = { $mechanism }: { $detail }
sandbox-check-write-inside = writes inside the worktree work
sandbox-check-write-outside = writes outside the worktree are blocked
sandbox-check-direct-network = direct network access is blocked
sandbox-check-proxy-unlisted = hosts not on the allowlist are refused
sandbox-pass = ok
sandbox-fail = FAILED
sandbox-none = No sandbox mechanism works on this machine. Agents will not start until one does; install bubblewrap on Linux.
usage-room = Room: { $tokens } tokens, ${ $cost } estimated
usage-line = { $who }: { $tokens } tokens, ${ $cost } estimated
usage-unknown = { $count ->
    [one] 1 turn reported no usage.
   *[other] { $count } turns reported no usage.
}
usage-subscription = Some usage is on a subscription plan; per-token prices do not apply to it.
budget-saved = Budget saved.
budget-line = { $scope }: { $used } of { $limit } { $unit } ({ $state })
budget-room = room
budget-state-ok = ok
budget-state-warning = warning
budget-state-exceeded = used up
help-mirror = Mirror the room's shared working copy into a directory (hosted mode). Agents you start there edit the host's files under claims.
hosted-sharing = Sharing { $dir } as the room's working copy ({ $files } files). Members mirror it with `polyph mirror <dir>`.
mirror-opened = Mirroring the room's working copy into { $dir }.
host-bad-mode = Unknown mode { $mode }: use distributed or hosted.
web-link = Open this link in your browser (it works once, for two minutes):
    { $url }
web-opened = Opened the browser client. The link works once.
help-phone = Pair your phone: show a link and QR code that open Polyph on the phone and join this room as you.
phone-scan = Scan this with your phone's camera, or open the link on the phone:
phone-link = { $url }
    Join code: { $code } (works once, for 15 minutes)
phone-approve = When the phone asks to join, approve it here or in the app. It joins as you, so it can answer your agents' approvals.
help-agent-hosts = List your agent hosts: your own servers that run your agents.
help-agent-host = Run agents on your own server, so work continues with your laptop closed.
help-agent-host-invite = On your computer: make a one-time enrollment command for a server.
help-agent-host-join = On the server: join the room as an agent host of the person who invited it.
help-agent-host-workdir = On the server: set the checkout agents work in.
agent-started-on = Started { $label } on agent host { $host }.
hosts-empty = You have no agent host in this room. Add one with `polyph agent-host invite`.
host-online = online
host-offline = offline
host-line = { $device }  { $state }  { $agents } running
host-invite = Run this on the server (it works once, for 15 minutes; replace <dir> with the checkout agents should work in):
host-invite-approve = Then approve the server's request to join. It joins as you, so its agents are yours.
host-joined = This server now runs agents for { $name }. Keep `polyph daemon run` going here.
help-plugin = Install and manage plugins: tools for agents and hooks on room events.
help-plugin-list = List installed plugins and what they may do.
help-plugin-add = Install a plugin from a folder with a polyph-plugin.toml.
help-plugin-remove = Remove an installed plugin.
help-skills = Skills: written instructions agents read for this team's kinds of tasks.
help-skills-list = List this repository's skills and your own.
help-skills-show = Show one skill.
help-skills-add = Copy a skill folder into this repository's .polyph/skills.
help-template = Start a repository from a template: polyph.toml and skills.
help-template-list = List templates.
help-template-apply = Write a template's files here; existing files are kept.
plugins-empty = No plugins installed. Add one with `polyph plugin add <folder>`.
plugin-line = { $name } { $version }  { $description }
plugin-gives-tools = gives agents tools
plugin-hooks = runs on: { $events }
plugin-hosts = may reach: { $hosts }
plugin-added = Installed { $name }. Its tools join agents started from now on; its hooks run on this device.
plugin-removed = Removed.
skills-empty = No skills yet. Add a folder with a SKILL.md to .polyph/skills/, or apply a template.
skill-source-project = project
skill-source-personal = yours
skill-line = { $name } ({ $source })  { $description }
skill-added = Added { $name } to .polyph/skills/. Commit it to share it with the room.
template-line = { $name }  { $description }
template-wrote = wrote { $file }
template-kept = kept { $file } (it already exists)
template-desc-startup = A small product team: agents work freely in their claims, a reviewer agent checks each task, a person approves merges.
template-desc-hackathon = Move fast for a weekend: agents run anything inside their sandbox, merges need no review.
template-desc-open-source = Contributors' agents on one public repository: every action asks first, reviews before merge, pull requests on GitHub.
template-desc-agency = Client work: careful defaults, a reviewer, and merges a person approves.
help-feedback = Open a prefilled GitHub issue with your version and system.
help-agent-message = Leave a message in an agent's inbox.
agent-messaged = Message left for { $label }. It reads it at its next turn.
feedback-opened = Opened a prefilled GitHub issue in your browser. Nothing is sent until you submit it.
feedback-link = Open this link to file an issue. It holds only your version, system and surface; nothing is sent until you submit it:
    { $url }
feedback-title = Feedback:
feedback-intro = Added by polyph feedback. No room names, paths, chat, code or relay addresses are included.
feedback-version = Polyph version: { $value }
feedback-os = System: { $value }
feedback-surface = Surface: { $value }
feedback-daemon = Daemon: { $value }
feedback-relay = Relay: { $value }
feedback-daemon-running = running
feedback-daemon-stopped = not running
feedback-relay-default = default relay
feedback-relay-custom = custom relay
feedback-happened = What happened
feedback-expected = What you expected

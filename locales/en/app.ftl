# Desktop app, browser client and status pill (docs/app.md).
app-name = Polyph
app-view-chat = Chat
app-view-board = Board
app-view-agents = Agents
app-view-activity = Activity
app-view-merges = Merges
app-view-terminals = Terminals
app-view-whiteboards = Whiteboards
app-view-settings = Settings
app-switch-room = Switch room
app-host-or-join = Host or join a room
app-people = People
app-menu = Menu
app-needs-you = Needs you
app-nothing-needs-you = Nothing needs you.
app-online-count = { $n } online
app-finding-running = { $n ->
    [one] 1 agent running. Nothing needs you.
   *[other] { $n } agents running. Nothing needs you.
}
app-finding-quiet = No agents running. Nothing needs you.
app-finding-more = { $first } (and { $n } more)
app-connecting = Connecting to the Polyph daemon on this computer.
app-reconnecting = Lost the connection to the daemon. Reconnecting.
app-unreachable = Cannot reach the Polyph daemon. Start it with `polyph daemon start`, then retry.
app-retry = Retry
app-load-failed = Could not load this.
app-cancel = Cancel
app-save = Save
app-saved = Saved.
app-send = Send
app-start = Start
app-stop = Stop
app-connected = Connected
app-not-connected = Not connected

# Signing in from the browser.
app-link-expired = This link has expired or was already used. Run `polyph web` again for a new one.
app-no-session = Open the browser client with `polyph web`. It signs this tab in with a one-time link.
app-signing-in = Signing in.

# No room.
app-no-room-title = No room yet
app-no-room-body = Host a room for your team, or join one with the code a teammate sent you.
app-host-room = Host a room
app-room-name = Room name
app-default-room-name = Our room
app-host = Host
app-join-room = Join a room
app-join-code = Join code
app-join = Join
app-join-waiting = Waiting for the room’s owner to let you in.
app-your-rooms = Your rooms

# Approvals and people waiting to join.
app-approve-once = Approve once
app-approve-task = Approve for this task
app-deny = Deny
app-deny-send = Deny with note
app-knock = { $name } asks to join this room.
app-knock-own-device = A new device asks to join as you ({ $name }). Let it in only if you just paired a phone or set up an agent host.
app-knock-approve = Let in
app-knock-let-in = Let in.

# Chat.
app-chat-placeholder = Message { $room }
app-thread-placeholder = Reply in { $thread }
app-threads = Threads
app-new-thread = New thread
app-thread-title = Thread title
app-thread-posts = { $n ->
    [one] 1 post
   *[other] { $n } posts
}
app-empty-chat-title = Quiet so far
app-empty-chat = No messages yet. Write the first one below.
app-empty-threads = No threads yet. Start one for a topic or a card.

# Board.
app-add-card = Add a card
app-card-title = Card title
app-card-draft = Draft
app-card-pr = PR { $n }
app-unassigned = Unassigned
app-empty-column = Nothing here
app-move-card = Move { $title }
app-move-to = Move to { $column }
app-plan-goal = Plan a goal
app-plan-placeholder = Describe the goal; a planner agent splits it into draft cards
app-plan = Plan
app-plan-accept = { $n ->
    [one] Accept the plan (1 card)
   *[other] Accept the plan ({ $n } cards)
}
app-plan-accepted = Plan accepted. The cards are ready.
app-workdir = Working folder
app-workdir-placeholder = /home/you/code/project
app-workdir-hint = The repository on this computer where your agents, plans and terminals start.
app-workdir-needed = Set your working folder in Settings first.

# Agents.
app-start-agent = Start an agent
app-agent-kind = Agent
app-agent-task = Task
app-agent-no-task = No task
app-agent-prompt = First prompt
app-select-agent = Select an agent to follow what it is doing.
app-empty-agents = No agents in this room yet.
app-send-inbox = Message { $agent } (goes to its inbox)
app-tr-started = Started.
app-tr-started-model = Started with { $model }.
app-tr-usage = { $input } tokens in, { $output } out

# Activity and merges.
app-replay = Replay
app-replay-live = Back to now
app-replay-hint = Select an entry to see the board as it was at that moment.
app-empty-merges-title = Nothing to land
app-merge-state = State
app-merge-change = Change
app-merge-by = By

# Terminals.
app-open-terminal = Open a terminal
app-term-ask = Ask for the keyboard
app-term-release = Give the keyboard back
app-term-revoke = Take the keyboard back
app-term-close = Close the terminal
app-term-watching = Watching. Ask for the keyboard to type.
app-term-you-type = You have the keyboard.
app-empty-terminals-title = No shared terminals
app-empty-terminals = Open one to run a command where the whole room can watch.

# Whiteboards.
app-new-whiteboard = New whiteboard
app-whiteboard-name = Whiteboard name
app-whiteboard-loading = Loading the whiteboard.
app-empty-whiteboards-title = Nothing sketched yet
app-empty-whiteboards = Create a whiteboard to sketch a design with the room. Agents can draw diagrams on it too.

# Command palette.
app-palette = Command palette
app-palette-placeholder = Go to a view, room or agent
app-palette-view = View
app-palette-room = Room
app-palette-agent = Agent
app-palette-theme = Switch between light and dark
app-palette-none = Nothing matches.

# Settings.
app-settings-device = This device
app-settings-device-hint = Your name is how teammates see you. Keys never leave this computer.
app-settings-name = Your name
app-settings-relay = Relay
app-settings-keystore = Keys stored in
app-settings-device-id = Device
app-settings-appearance = Appearance
app-theme = Theme
app-theme-system = Match the system
app-theme-dark = Dark
app-theme-light = Light
app-language = Language
app-language-system = Match the system
app-language-en = English
app-language-th = ไทย
app-language-ja = 日本語
app-language-zh-hans = 简体中文
app-settings-room = This room
app-settings-room-hint = Invite codes work once and expire in 15 minutes.
app-invite = Create an invite code
app-invite-hint = Send this code to one teammate. They run `polyph join` with it, or enter it in the app.
app-settings-role = Your role
app-settings-members = Members
app-role-owner = Owner
app-role-maintainer = Maintainer
app-role-member = Member
app-role-viewer = Viewer
app-settings-integrations = Integrations
app-settings-integrations-hint = Connect them from the terminal with `polyph github connect` or `polyph linear connect`.
app-github = GitHub
app-linear = Linear
app-settings-usage = Usage
app-settings-usage-hint = Estimated from each agent’s reported tokens and the prices in polyph.toml.
app-usage-who = Who
app-usage-turns = Turns
app-usage-tokens = Tokens
app-usage-cost = Estimated cost
app-usage-room = Whole room

# Status pill.
app-pill-open = Open Polyph
app-pill-agents = { $n ->
    [one] 1 agent running
   *[other] { $n } agents running
}
app-tray-show = Show Polyph
app-tray-pill = Show the status pill
app-tray-quit = Quit

# Phone (the PWA) and pairing a phone from the app.
app-phone-no-room-body = Join a room with the code a teammate sent you, or with the pairing link from your computer. Rooms are hosted from a computer running Polyph.
app-phone-loading = Starting Polyph on this phone.
app-phone-unsupported = This browser cannot run Polyph. It needs WebAssembly, WebSockets and IndexedDB.
app-phone-failed = Polyph could not start on this phone: { $error }
app-phone-install = Add Polyph to your home screen to open it like an app and get approval alerts.
app-phone-alerts = Turn on alerts
app-phone-leave = Leave this room on this phone
app-phone-pair = Pair a phone
app-phone-pair-hint = Open the link on your phone, or scan the code. The phone joins as you, so it can answer your agents' approvals. Approve its request when it arrives.
app-phone-pair-make = Make a pairing link
app-phone-pair-qr = QR code for the pairing link
app-phone-pair-expires = Works once, for 15 minutes.

# Agent hosts in the app.
app-agent-on = Run on
app-agent-on-here = This computer
app-agent-on-any = Any of my agent hosts
app-agent-on-host = Agent host { $host } ({ $n } running)
app-agent-on-host-offline = Agent host { $host } (offline)
app-agent-no-hosts = You have no agent host in this room. Add your own server with `polyph agent-host invite` on your computer.
app-agent-requested = Asked your agent host to start it. It appears here when it starts.

# Plugins in Settings.
app-settings-plugins = Plugins
app-settings-plugins-hint = Installed on this device with `polyph plugin add`. They run in the agent sandbox and reach only the hosts listed.
app-plugins-empty = No plugins installed.
app-plugin-tools = Tools for agents
app-plugin-hooks = Runs on
app-plugin-hosts = Network
app-plugin-no-network = None
app-yes = Yes

# First-run checklist (desktop)
app-setup-title = Getting started
app-setup-progress = { $done } of { $n } done
app-setup-hide = Hide
app-setup-step-done = (done)
app-setup-step-todo = (to do)
app-setup-name = Tell teammates your name
app-setup-name-how = Shown beside your messages and your agents' names.
app-setup-room = Host or join a room
app-setup-room-how = Host one for your project, or join with a code a teammate sends you.
app-setup-folder = Link your project folder to the room
app-setup-folder-how = In a terminal, in your project folder, run: polyph start. Agents started there then join this room.
app-setup-agents = Connect the agents you already use
app-setup-agents-how = Polyph adds itself to their MCP settings; you see each change before it is written.
app-setup-agents-found = Found: { $names }.
app-setup-agents-none = No supported agent found on this computer. Install Claude Code, Codex, Cursor, Gemini CLI, Windsurf, Zed or OpenCode, or run polyph connect <name>.
app-setup-agents-review = Review changes
app-setup-agents-write = Write changes
app-setup-agents-done = Connected. Restart those agents to load Polyph.
app-setup-change = { $client }: { $file }
app-setup-comments = Comments in this file are not kept; a backup next to it has them.
app-setup-running = Start an agent in the project folder
app-setup-running-how = Open Claude Code, Codex or another connected agent in the folder; it joins the room as you/agent.
app-setup-team = Invite a teammate
app-setup-team-how = They join with a code from any computer running Polyph.
app-setup-team-invite = Make a join code
app-setup-team-code = Send them:

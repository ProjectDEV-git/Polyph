# Terminal UI (docs/tui.md).
tui-view-chat = Chat
tui-view-board = Board
tui-view-agents = Agents
tui-view-activity = Activity
tui-view-merges = Merges
tui-view-terminals = Terminals
tui-rooms = ROOMS
tui-people = PEOPLE
tui-agents = AGENTS
tui-threads = THREADS
tui-main-chat = Room chat
tui-you = { $name } (you)
tui-quick-session = Quick session
tui-no-room = No room open
tui-no-agents = No agents running.
tui-connected = connected
tui-offline = offline
tui-online = { $n } online
tui-approvals = { $n ->
    [one] 1 approval
   *[other] { $n } approvals
}
tui-state-running = running
tui-state-waiting = waiting for approval
tui-state-blocked = blocked
tui-state-error = error
tui-state-done = done
tui-state-idle = idle
tui-state-stopped = stopped
tui-empty-no-room = No room yet. Create one with `polyph host`, or join with :join CODE.
tui-empty-chat = No messages yet. Press i to write one.
tui-empty-board = No cards yet. Add one with :card TITLE, or plan a goal with :plan GOAL.
tui-empty-agents = No agents in this room. Start one with :agent claude (or codex, local).
tui-empty-activity = Nothing has happened yet.
tui-empty-merges = Nothing in the merge queue. Finished tasks appear here.
tui-empty-terminals = No shared terminals. Open one with :term.
tui-card-pr = PR
tui-agent-stopped = The agent stopped.
tui-replay-at = Board at { $time }
tui-term-writer = { $name } is typing
tui-term-attached = attached · Ctrl-] detaches
tui-term-preview = preview · Enter attaches
tui-term-no-keyboard = You do not have the keyboard. Press a to ask the owner.
tui-term-asked = Asked for the keyboard.
tui-approve-command = { $agent } wants to run: { $command }  (in { $cwd })
tui-approve-edit = { $agent } wants to edit: { $files }
tui-approve-network = { $agent } wants to connect to { $host }:{ $port }
tui-approve-outside = { $agent } wants to use { $path }, outside its worktree
tui-approve-push = { $agent } wants to push { $branch } to { $remote }
tui-approve-merge = Merge { $branch } into { $into } (for { $agent })
tui-approve-terminal = { $agent } asks to type in your terminal
tui-approve-steer = Steer { $agent }: { $text }
tui-approve-other = { $agent }: { $title } { $detail }
tui-approve-keys = a approve once   t approve for the task   d deny   n next
tui-approved = Approved.
tui-denied = Denied.
tui-prompt-chat = message
tui-prompt-thread = reply
tui-prompt-agent = to { $agent }
tui-prompt-deny = note for the denial (optional)
tui-hint-normal = i write   : command   1-6 views   ? keys   q quit
tui-hint-input = Enter sends   Esc cancels
tui-hint-command = join CODE · thread NAME · card TITLE · plan GOAL · agent KIND · term · room NAME · quit
tui-hint-attached = Keys go to the terminal. Ctrl-] detaches.
tui-help-title = Keys
tui-help-views = switch view
tui-help-move = move the selection
tui-help-columns = board column; activity replay step
tui-help-move-card = move the selected card
tui-help-open = open a thread, prompt an agent, attach a terminal, replay
tui-help-input = write a message
tui-help-command = command line
tui-help-approve = approve once, for the task, or deny
tui-help-rooms = previous / next room
tui-help-threads = show threads
tui-help-rail = show or hide the side rail
tui-help-escape = leave input, close overlays
tui-help-quit = quit (the daemon keeps running)
tui-ago-now = just now
tui-ago-minutes = { $n ->
    [one] 1 min ago
   *[other] { $n } min ago
}
tui-ago-hours = { $n ->
    [one] 1 hour ago
   *[other] { $n } hours ago
}
tui-ago-days = { $n ->
    [one] 1 day ago
   *[other] { $n } days ago
}
tui-joined = Joined.
tui-thread-created = Thread started.
tui-card-added = Card added to the backlog.
tui-planned = Planned. Review the draft cards on the board.
tui-agent-started = Agent started.
tui-agent-stopping = Stopping the agent.
tui-sent-inbox = Sent to the agent's inbox; it reads it at its next turn.
tui-no-such-room = No room by that name.
tui-unknown-command = Unknown command. Press : and read the hint for the list.
tui-lost-daemon = Lost the connection to the daemon. Run `polyph` again to reconnect.
tui-help-next-approval = next approval
tui-help-stop = stop the selected agent (yours)
tui-help-close = close the selected terminal (yours)

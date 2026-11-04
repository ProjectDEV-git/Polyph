# The VS Code extension (apps/vscode). Keys used in package.json become
# package.nls*.json at build time (apps/vscode/scripts/nls.mjs).
vscode-display-name = Polyph
vscode-description = See which agent holds each file, answer your agents' approvals and follow the room from VS Code.
vscode-view-room = Room
vscode-cmd-open = Open the room
vscode-cmd-approvals = Answer my approvals
vscode-cmd-reconnect = Reconnect to the daemon
vscode-cmd-pair-phone = Pair a phone
vscode-color-other = Files claimed by another person's agent.
vscode-color-mine = Files claimed by your own agents.
vscode-setting-home = Polyph data directory. Empty uses the same one as the polyph command (or POLYPH_HOME).
vscode-setting-command = The polyph command, used to start the daemon when it is not running.
vscode-status-offline = Polyph: daemon unreachable
vscode-status-offline-hint = Start it with `polyph daemon start`, or set polyph.command. Click to try again.
vscode-status-connecting = Polyph: connecting
vscode-status-no-room = Polyph: no room open
vscode-status-agents = { $n ->
    [one] 1 agent
   *[other] { $n } agents
}
vscode-status-approvals = { $n ->
    [one] 1 approval
   *[other] { $n } approvals
}
vscode-status-approvals-hint = Your agents are waiting for you. Click to answer.
vscode-status-hint = Open the room
vscode-claim-mine = Claimed by your agent { $agent }: { $reason }
vscode-claim-other = Claimed by { $agent }: { $reason }
vscode-claim-note = Claimed by { $agent } ({ $target })
vscode-claim-editing = { $agent } holds a claim on { $file }. Your edits may collide with its work; check with its owner in the room.
vscode-approval = Polyph: { $action }
vscode-deny-note = Note for the agent (optional): why not, or what to do instead.
vscode-no-approvals = Nothing is waiting for you.
vscode-pick-approval = Choose an approval to answer
vscode-open-app = Open the room
vscode-phone-copied = Pairing link copied (code { $code }). Open it on your phone, then approve the phone's request to join.

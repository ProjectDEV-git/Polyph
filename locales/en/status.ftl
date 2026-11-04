# The status provider for widgets (docs/status-feed.md).
status-token = Widget token (read-only): { $token }
status-urls = Widget: { $url }
status-token-note = It shows states, names and counts only, never code or chat. Replace it with polyph status --rotate-token.
status-offline = Offline: not connected to the relay.
status-waiting = { $agent } waits for you: { $title }
status-running = { $n ->
    [one] 1 agent running
   *[other] { $n } agents running
}
status-quiet = Nothing running. Nothing needs you.
status-no-room = No room open.
status-approvals = { $n ->
    [one] 1 approval
   *[other] { $n } approvals
}

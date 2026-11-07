# Teamwork commands: terminals, threads, activity, whiteboards, planning, tasks, merges, notes, integrations.
help-term = Shared terminals: open one for the room, watch, ask to type.
help-term-open = Open a terminal on this machine and share it with the room.
help-term-list = List the room's terminals.
help-term-read = Print a terminal's recent output.
help-term-send = Type a line into a terminal (yours, or one whose keyboard you hold).
help-term-ask = Ask a terminal's owner for the keyboard.
help-term-release = Give the keyboard back.
help-term-revoke = Take the keyboard back from whoever holds it (your terminals).
help-term-close = Close one of your terminals.
help-thread = Threads: conversations about a topic or a card.
help-thread-new = Start a thread.
help-thread-list = List threads.
help-thread-show = Show a thread's posts.
help-thread-reply = Post in a thread.
help-activity = Show the room's recent activity.
help-whiteboard = Whiteboards.
help-whiteboard-list = List whiteboards.
help-whiteboard-new = Create a whiteboard.
help-plan = Plan a goal into tasks with a planner agent.
help-plan-new = Ask the planner to split a goal into draft cards.
help-plan-accept = Accept a plan's draft cards.
help-task = Finish or hand over an agent's task.
help-task-finish = Commit an agent's work and queue it to merge.
help-task-handoff = Hand an agent's task to another agent, with a brief.
help-merge = Show the merge queue.
help-memory = Show notes pinned to code.
help-github = GitHub: issues and pull requests.
help-github-connect = Connect your GitHub account (paste a token).
help-github-disconnect = Forget the GitHub token on this device.
help-github-import = Import open issues as backlog cards.
help-github-sync = Check pull requests for tasks now.
help-linear = Linear: issues follow their cards.
help-linear-connect = Connect your Linear account (paste an API key).
help-linear-disconnect = Forget the Linear key on this device.
help-linear-import = Import a team's open issues as cards.
help-webhook = Webhooks: send your notifications to Slack, Discord or any URL.
help-webhook-add = Add a webhook.
help-webhook-list = List webhooks.
help-webhook-test = Send a test notification to a webhook.
help-webhook-remove = Remove a webhook.
done = Done.
nothing-yet = Nothing yet.
token-prompt = Paste the token and press Enter:
token-empty = No token was given.
term-opened = Opened terminal { $id }: { $title }
term-line = { $id }  { $title }  ({ $owner }) { $status }
term-status-open = open
term-status-closed = closed (exit { $code })
term-status-writer = { $name } has the keyboard
term-asked = Asked the owner for the keyboard. You can type once they approve.
thread-created = Started thread { $id }.
thread-line = { $id }  { $title }  ({ $posts ->
    [one] 1 post
   *[other] { $posts } posts
})
post-line = { $name }: { $text }
activity-line = { $when }  { $who } { $text }
ago-now = just now
ago-minutes = { $n ->
    [one] 1 minute ago
   *[other] { $n } minutes ago
}
ago-hours = { $n ->
    [one] 1 hour ago
   *[other] { $n } hours ago
}
ago-days = { $n ->
    [one] 1 day ago
   *[other] { $n } days ago
}
whiteboard-line = { $id }  { $name }
whiteboard-created = Created whiteboard { $name }.
plan-working = The planner is reading the code. This can take a few minutes.
plan-task = - { $title }
plan-overlap = { $a } and { $b } may both change { $files }.
plan-created = Planned { $count } draft tasks. Accept them with `polyph plan accept { $id }`.
plan-accepted = Accepted { $count } tasks; { $ready } are ready to start.
task-queued = Committed { $commits } commit(s) on { $branch } and queued it to merge into { $into }.
handoff-to = Handed the task to { $label }.
handoff-left = Left a brief on the card for whoever takes the task next.
merge-line = { $branch } → { $into }: { $state }
merge-state-queued = queued
merge-state-reviewing = in review
merge-state-changes-requested = changes requested
merge-state-awaiting-approval = waiting for approval
merge-state-merging = merging
merge-state-resolving = resolving conflicts
merge-state-checking = running checks
merge-state-pull-request = pull request open
merge-state-merged = merged
merge-state-failed = failed
merge-checks-untrusted = Checks did not run: { $file } is not trusted on this device, so its checks ({ $ignored }) were ignored and the merge stopped instead of skipping them. Review the file, run `polyph trust` in { $dir }, then queue the task again.
project-untrusted-hint = { $file } is not trusted on this device, so its { $ignored } settings were ignored. If you rely on them, review the file and run `polyph trust` in { $dir }.
agent-data-dir-exposed = Refusing to start the agent: { $path } contains or is inside Polyph's data directory { $home }, which holds the device keys and the trust record, and the agent could read or change them. Start the agent in a folder that does not overlap { $home }.
budget-prices-untrusted = A dollar budget applies, but { $file } is not trusted on this device, so its prices ({ $ignored }) were ignored and every turn would count as $0. The agent stops until you review the file and run `polyph trust` in { $dir }, then start the agent again.
agent-path-unresolved = Refusing to start the agent: cannot resolve { $path } ({ $error }), so Polyph cannot check that it stays clear of its data directory.
merge-via-untrusted = { $file } is not trusted on this device, so its push and pull request settings ({ $ignored }) were ignored and this task merges locally. Review the file and run `polyph trust` in { $dir } to apply them.
memory-line = { $at }  { $text }  ({ $by })
github-connected = Connected to GitHub as { $login }.
github-synced = { $count ->
    [one] 1 pull request changed.
   *[other] { $count } pull requests changed.
}
linear-connected = Connected to Linear as { $name }.
imported = { $count ->
    [one] Imported 1 issue.
   *[other] Imported { $count } issues.
}
webhook-added = Added webhook { $id }.
webhook-line = { $id }  { $format }  { $url }
webhook-tested = Sent a test notification.

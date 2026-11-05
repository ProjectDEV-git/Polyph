# Activity feed entries. The actor's name and color are shown beside each line.
activity-card-created = created { $title } in { $column }
activity-card-moved = moved { $title } from { $from } to { $to }
activity-card-updated = edited { $title }
activity-card-removed = removed { $title }
activity-merged = merged { $branch } into { $into }
activity-merge-failed = could not merge { $branch } into { $into }
activity-changes-requested = review asked for changes to { $branch }
activity-agent-started = started a { $kind } agent
activity-agent-stopped = stopped an agent
activity-approval = { $approved ->
    [true] approved { $title } for { $agent }
   *[other] denied { $title } for { $agent }
}
activity-overlap = { $agent } changed { $path }, which { $holder } holds
activity-thread-created = started the thread { $title }
activity-whiteboard-created = created the whiteboard { $name }
activity-terminal-opened = opened the terminal { $title }
activity-agent-connected = connected { $client }
activity-task-finished = finished a task: { $summary }

# Notifications for the person at this device. Webhooks send these texts too.
notify-approval = { $agent } needs your approval in { $room }: { $action }
notify-merged = { $branch } was merged into { $into } in { $room }.
notify-merge-failed = { $branch } could not be merged into { $into } in { $room }.
notify-changes-requested = The reviewer asked for changes to { $branch } in { $room }.
notify-mention = { $from } mentioned you in { $room }.
notify-overlap = { $agent } changed { $path }, which { $holder } holds, in { $room }.
notify-budget = A budget in { $room } is { $percent }% used.
notify-loop-guard = Polyph stopped { $agent } from being woken again in { $room }; the agents need a person.
notify-test = This is a test notification from Polyph.

# polyph update: replaces an install made with the install script by a newer release.
help-update = Update polyph to the latest release.
update-unsupported = There are no release builds for this system; build polyph from source instead.
update-not-archive = This polyph ({ $path }) was not installed with the install script, so it cannot update itself. Update it the way you installed it; the desktop app and the VS Code extension bring their own polyph.
update-no-release = No release found at github.com/{ $repo }.
update-no-build = This release has no build for this system ({ $target }).
update-unreachable = Could not check for updates: { $error }
update-available = polyph { $version } is available (you have { $current }). Run `polyph update` to install it.
update-current = polyph { $version } is the latest release.
update-downloading = Downloading { $file }
update-checksum = The download did not match its checksum ({ $file }); nothing was changed.
update-failed = Could not update the files in { $dir }: { $error }
update-done = Updated polyph { $from } → { $to }.
update-daemon-restarted = The daemon restarted on the new version.
update-daemon-restart-failed = The daemon is stopped; it starts again with your next polyph command.
update-daemon-busy = { $count ->
    [one] The daemon keeps running the old version while an agent works. When it is done, restart it: polyph daemon stop, then polyph daemon start.
   *[other] The daemon keeps running the old version while { $count } agents work. When they are done, restart it: polyph daemon stop, then polyph daemon start.
}

# polyph uninstall
help-uninstall = Remove polyph from this computer.
uninstall-not-archive = This polyph ({ $path }) was not installed with the install script; remove it the way you installed it.
uninstall-needs-yes = Removing polyph needs confirmation; run it in a terminal, or add --yes.
uninstall-what = This removes polyph, polyph-relay and the browser client from { $dir } and stops the daemon, with any agents running in it.
uninstall-purge-what = It also deletes this device's Polyph data in { $dir }: its identity, rooms and settings. Rooms only this device holds are lost.
uninstall-confirm = Continue? [y/N]
uninstall-cancelled = Nothing was removed.
uninstall-failed = Could not remove the files in { $dir }: { $error }
uninstall-done = polyph is uninstalled.
uninstall-kept-data = Your Polyph data stays in { $dir }, for a later install. To delete it as well, remove that folder.

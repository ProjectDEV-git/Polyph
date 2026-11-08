# Connecting your own agents, getting started, checking the setup, the room's repository.
help-connect = Connect the agents you already use (Claude Code, Codex, Cursor, VS Code, Gemini CLI, Windsurf, Zed, OpenCode) to Polyph.
help-disconnect = Remove Polyph from your agents' settings.
help-start = Set up Polyph in this folder: daemon, your name, a room, a join code and your agents.
help-doctor = Check the setup and print the command that fixes each problem.
help-sync = Fetch the latest work and move your branch forward when nothing would be lost.

connect-no-home = Could not find your home folder; set HOME and try again.
connect-unknown = Polyph does not know the agent "{ $name }". Known: { $known }.
connect-none-found = Found none of the agents Polyph can connect on this computer. Name one to connect it anyway: { $known }.
connect-create = { $client }: create { $file }
connect-change-in = { $client }: change { $file }
connect-remove-from = { $client }: remove Polyph from { $file }
connect-drops-comments = Comments in this file are not kept; the backup has them.
connect-already = { $client } is already connected.
connect-needs-yes = Changing your agents' settings needs confirmation; run it in a terminal, or add --yes.
connect-confirm = Write these changes? [y/N]
connect-cancelled = Nothing was changed.
connect-done = Connected { $client }.
connect-backup = The previous file is saved as { $file }.
connect-write-failed = Could not write { $file }: { $error }
connect-restart = Restart those agents (or reload their MCP servers) to load Polyph.
disconnect-confirm = Remove Polyph from these settings? [y/N]
disconnect-done = Disconnected { $client }.
disconnect-nothing = None of your agents has Polyph in its settings.

start-ask-name = Your name, as teammates will see it [{ $guess }]:
start-needs-name = Polyph needs your name. Pass --me "Your Name" or run: polyph config set name "Your Name"
start-name-set = You are { $name } in Polyph.
start-name-git = Using your git name, { $name }. Change it with: polyph config set name "Your Name"
start-room-existing = This folder belongs to the room { $name }.
start-room-created = Made the room { $name } for { $dir }.
start-ready = Ready. Agents you start in { $dir } join { $room } as { $name }/<agent>.
start-teammates = Teammates join with: polyph join { $code }

doctor-fix = fix: { $command }
doctor-git-ok = git is installed.
doctor-git-missing = git is not installed; Polyph needs it for branches, merges and cloning.
doctor-daemon-ok = The daemon is running (version { $version }).
doctor-daemon-down = The daemon is not running.
doctor-daemon-version = The daemon runs version { $daemon } but this polyph is { $cli }; restart the daemon.
doctor-name-ok = Your name is { $name }.
doctor-name-missing = You have not set your name.
doctor-folder-unlinked = This folder is not linked to a room, so agents started here do not know which room to join.
doctor-folder-gone = This folder is linked to { $room }, which is no longer on this computer.
doctor-room-ok = Connected to the room { $room }.
doctor-room-closed = The room { $room } is closed on this computer.
doctor-room-offline = The room { $room } cannot reach its relay ({ $relay }).
doctor-repo-ok = This folder is a clone of { $url }.
doctor-repo-differs = This folder's origin is { $have }, but the room works on { $want }.
doctor-repo-missing = The room works on { $url }, but this folder is not a clone of it.
doctor-client-ok = { $client } is connected.
doctor-client-unconnected = { $client } is installed but not connected to Polyph.
doctor-client-missing-exe = { $client } starts { $path }, which does not exist.
doctor-client-port = { $client } points at an old web address; the daemon now listens on port { $port }.
doctor-problems = { $count ->
    [one] Found 1 problem.
   *[other] Found { $count } problems.
}
doctor-all-good = Everything looks right.

repo-remembered = The room remembers its repository: { $url }
repo-linked = Linked { $dir } to the room.
repo-clone-hint = The room works on { $url }. Clone it, then run polyph start in the clone.
repo-clone-confirm = Clone { $url } into { $dir } with your git login? [Y/n]
repo-clone-partial = The repository has { $files } files: cloning without file contents first, which git downloads as you need them.
repo-clone-failed = Could not clone { $url }: { $error }
repo-cloned = Cloned { $url } into { $dir } and linked it to the room.

sync-nothing = No folders are linked to a room here; run polyph start in your checkout.
sync-failed = { $dir }: { $error }
sync-no-upstream = { $dir }: { $branch } has no upstream branch, so there is nothing to move it to.
sync-updated = { $dir }: moved { $branch } forward by { $count } commit(s).
sync-held-dirty = { $dir }: { $branch } is { $count } commit(s) behind, but you have uncommitted changes; commit or stash them, then sync again.
sync-held-diverged = { $dir }: { $branch } has { $ahead } commit(s) of its own and is { $behind } behind; merge or rebase it yourself.
sync-ahead = { $dir }: { $branch } is up to date, with { $count } commit(s) not pushed yet.
sync-current = { $dir }: { $branch } is up to date.

help-brief = Read or change the project brief every agent reads: architecture, conventions, decisions.
help-brief-show = Print the brief.
help-brief-set = Replace the brief with a file (- reads standard input).
help-brief-edit = Edit the brief in your editor.
help-search = Search the room's notes, task cards and threads.
help-catchup = What was finished lately, with each agent's summary.
help-areas = Show the code areas (.polyph/AREAS or CODEOWNERS) and who looks after them.

brief-by = Last edited by { $name }.
brief-empty = This room has no brief yet. Write one with polyph brief edit; until then agents read .polyph/brief.md if the repository has one.
brief-saved = Saved the brief; every agent in the room reads it with get_context.
brief-unchanged = The brief is unchanged.
brief-template = # Project brief
    
    ## Architecture
    
    ## Conventions
    
    ## Decisions
search-none = Nothing matches "{ $query }".
search-kind-note = note
search-kind-card = task
search-kind-thread = thread
catchup-brief = From the brief:
catchup-finished = Finished lately:
catchup-nothing = No task has been finished yet.
areas-empty = This repository has no code areas. Add .polyph/AREAS (same form as CODEOWNERS): a pattern, then owners.
areas-from = Code areas from { $file }:
areas-of = { $path } is in { $pattern }, looked after by { $owners }.
areas-none-for = { $path } is in no code area.

help-repo = Show or change the repository the room works on.
help-repo-show = Show the room's repository.
help-repo-set = Set the room's repository; any credentials in the URL are removed.
help-repo-clear = Forget the room's repository.
repo-show = The room works on { $url } (branch { $branch }).
repo-none = The room has no repository yet. Set one with polyph repo set <url>.
connect-by-extension = { $client } gets Polyph from its Polyph extension or plugin, so there is nothing to change.
doctor-client-extension = { $client } gets Polyph from its Polyph extension or plugin.
help-hook = Hooks for agents' editors (the Claude Code plugin runs these).
help-hook-pre-edit = Before an agent edits a file: block the edit when another agent holds the file, otherwise claim it.

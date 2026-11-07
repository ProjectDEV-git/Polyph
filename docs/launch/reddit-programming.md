# r/programming draft

Draft. Posting is human-only (Kao). Read the subreddit's current rules on
self-promotion first and follow them; if the post would break them, do not
post it. r/programming usually prefers a technical write-up over a product
announcement, so this one leads with the problem and the design.

## Title

How we stop two coding agents from editing the same file: claims, worktrees and an intent-aware merge queue

## Body

If a team of five each runs a coding agent, the usual failure is quiet:
two agents change the same file in different branches, nobody knows until
merge time, and one of them did work that has to be redone.

Polyph is a tool we are releasing as a public beta that coordinates this. The
parts, with what each one does and does not do:

Claims. Before an agent edits a file, a folder or a single function, it
claims it. The claim lives in the room's shared state. A second agent that
asks for the same thing is told who holds it and can wait, work elsewhere or
ask the holder. Claims are advisory: they cover agents that go through
Polyph, not a process that writes to the checkout on its own.

Worktrees. Each task runs in its own git worktree and branch, so claims are
an early warning and branches are the actual isolation. Function-level claims
let two agents work in one file in separate worktrees. Functions are found by
pattern, not a parser, so unusual formatting can turn a function claim into a
whole-file claim.

A planner. `polyph plan new "<goal>"` has an agent draft cards and report
where they would overlap. You read it and accept it with `polyph plan accept`.

Review and merge. A reviewer agent reads each finished task, and a merge
queue merges it. When two tasks changed the same code, the conflict is
resolved by an agent given both tasks' intent, and the result is reviewed. The merge crate has unit tests. We have no data on how often the result is right on real
repositories, and we are not claiming it is always right.

Encryption. Rooms are end-to-end encrypted. A relay forwards ciphertext and
learns room ids, device public keys, message sizes and IP addresses. You can
run the relay yourself as one binary with SQLite.

Try it:

    curl -fsSL https://raw.githubusercontent.com/ProjectDEV-git/Polyph/Main/scripts/install.sh | sh
    cd your-repo && polyph start

Beta limits you should know about:

- Linux x86_64 is where we test by hand. macOS and Windows are experimental
  and have been tested in CI only.
- Merges land on the computer that ran the task. Teammates get them through
  your git remote and `polyph sync`. Nothing rebases a teammate's branch.
- In hosted mode (one shared working copy) two agents' whole-file writes can
  still overwrite each other.
- Late joiners do not see agents' earlier transcripts.
- There are no customers or benchmarks to point at. The only measurement we
  publish is the relay's memory under load (71 MB peak for 50 rooms of 5
  people).

The engine and apps are Apache-2.0. Known limits and what the relay can see:
https://github.com/ProjectDEV-git/Polyph/blob/Main/docs/beta.md

Site: https://projectdev-git.github.io/Polyph
Releases: https://github.com/ProjectDEV-git/Polyph/releases

Feedback, especially on the claim model and the merge step, is welcome as
issues on the repository.

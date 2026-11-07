# X thread draft

Draft. Posting is human-only (Kao). Each post is at most 280 characters;
check the counts in the composer before posting, since links count as 23.
Attach the demo video to post 1 once it exists. No screenshots of the real
app exist yet, so do not attach any.

1.
Polyph is in public beta. It puts every teammate's coding agents in one
end-to-end encrypted room, so agents claim files before they edit and do not
collide. Free in the beta. Demo and links below.
[video: docs/launch/demo-script.md, once recorded]

2.
The setup: each person runs their own agents on their own machine with their
own keys. Claude Code, Codex, OpenCode, Gemini CLI, any ACP agent, or a local
model. One person runs polyph start, teammates run polyph join with an
8-character code.

3.
Before an agent edits a file, folder or function, it claims it. A second
agent that wants the same file is told who holds it and works elsewhere. Each
task gets its own git worktree, a reviewer agent checks it, then a merge queue
lands it.

4.
Give the planner a goal with polyph plan new. It drafts cards and shows where
they overlap. Accept the plan, then start agents on the cards. Approvals show
the exact command, files and hosts, and can be answered from a phone.

5.
Privacy: the relay forwards ciphertext. It cannot read code, chat, the board
or terminals. It does see room ids, device keys, message sizes and IP
addresses. The full list is in the beta page. No telemetry.

6.
You can run the relay yourself: one binary, SQLite. In our load test with 50
rooms of 5 people it peaked at 71 MB of memory. There is also a LAN mode
with no internet.

7.
Limits, plainly. Linux x86_64 is the tested platform. macOS and Windows are
experimental, unsigned, and tested in CI only. Late joiners do not see earlier
agent transcripts. The local-model agent gets no Polyph tools yet. No
spectator links yet.

8.
Beta page with every known limit:
https://github.com/ProjectDEV-git/Polyph/blob/Main/docs/beta.md
Site: https://projectdev-git.github.io/Polyph
Downloads: https://github.com/ProjectDEV-git/Polyph/releases
Engine and apps are Apache-2.0.

9.
If you try it and the first ten minutes go wrong, I want the details:
polyph doctor output and what you ran, as an issue on GitHub. That is how the
beta gets better.

# Demo video script

Length: about 85 seconds. Audience: engineers who already use coding agents.
Draft only. Nothing here has been recorded.

The commands are the ones in `docs/beta-journeys.md` and `crates/polyph-cli`.
The terminal output on screen must be the real output from the recording
run. Do not mock, retype or edit output; if a step fails on the day, say so
in the notes and re-record it.

Cast: Ann (host), Bo and Cy, on three different networks. Each name is a
person at a laptop; Cy also has a phone.

## Shot list

| Time | On screen | Voiceover |
|---|---|---|
| 0:00 to 0:07 | Three laptops on three networks (home connection, phone hotspot, office). Title card: "Polyph. Your team's coding agents, in one room." Small label: "Public beta". | Three people, three networks, each with their own coding agent. |
| 0:07 to 0:20 | Ann's terminal: `cd ~/code/payments && polyph start`. The join code prints. Cut to Bo's terminal: `polyph join A1B2-CDEF --name Bo --clone`. It shows the wait for approval. Back to Ann: `polyph room knocks`, then `polyph approve <id>`. Bo's repository clones. Repeat quickly for Cy (cut at the same point, no need to show it in full). | Ann starts a room in her repository and gets an eight-character code. Bo and Cy join with it. Ann approves each one before they see anything. The relay only carries encrypted traffic. |
| 0:20 to 0:35 | Ann: `polyph plan new "Add refunds to the billing API"`. The draft cards and overlaps print. Ann runs `polyph plan accept <plan>`. Split screen of `polyph board` on all three machines showing the same cards in Ready. | Ann gives the planner a goal. It splits it into cards and shows where they overlap. She accepts the plan and everyone sees the same board. |
| 0:35 to 0:40 | Bo: `polyph agent start claude --task <card>`. Cy: `polyph agent start claude --task <card>` on the second card. | Bo and Cy each start their own agent on a card. |
| 0:40 to 0:52 | Cy's terminal: `polyph claims`. It lists Bo's agent's claim on a file. Then Cy's agent's transcript in the terminal UI (`polyph`) hits the same file and is told who holds it, then moves to other work. Zoom on the claim line and the name. | Before an agent edits a file it claims it. When Cy's agent reaches a file that Bo's agent holds, it is told who has it and picks other work. |
| 0:52 to 1:02 | Cy's phone, the Polyph phone app (opened with `polyph phone`). An approval card shows the exact command, files and hosts for an agent of Ann's. Finger taps "Approve once". Cut to Ann's terminal: the agent continues. | Approvals show the exact command, files and hosts. Ann approves one from her phone. |
| 1:02 to 1:17 | Bo: `polyph task finish <agent>`. Then `polyph merge`. The reviewer agent's review, then the merge queue merges two tasks that touched the same function. Show the result with both changes present (for example a retry on timeout and an idempotency key in the same handler). Use the real `polyph merge` output and `git diff`. | Two tasks changed the same code. The merge keeps what both were trying to do, and the reviewer checks it before it lands. |
| 1:17 to 1:25 | End card on a plain dark background: `curl -fsSL https://raw.githubusercontent.com/ProjectDEV-git/Polyph/Main/scripts/install.sh | sh`, then the site address and "Public beta. Free. Read the known limits." with the beta page address. | Polyph is in public beta and free. Linux is the tested platform, macOS and Windows are experimental. The known limits are on the beta page. |

Adjust times to the real footage. Total must stay under 90 seconds; cut the
second joiner and the end-card lines before cutting any limit statement.

## What the video must not claim

- No speed-up, productivity or "ships faster" claim, and no measured
  numbers other than ones read off the real screen.
- No claim that merges are always right. The merge in the video is a real
  case, not a statement about all cases.
- Nothing about spectator links, push notifications, accounts or Polyph-run
  cloud agents. None exist in the beta.
- Do not show the interface fragments from the website as if they were the
  real app.

## On-screen text and captions

Use the same sentences as the voiceover as captions, in English. Thai,
Japanese and Simplified Chinese captions are a later step and need a human
translator. Monospace for commands, the display serif only on the title card
(see `DESIGN.md`). No music notes, staves or piano imagery. Dark theme.

## Recording

HUMAN-ONLY. This needs three real computers or three VMs on different
networks, a phone, and people to run them. An agent cannot record it, and
no agent uploads it.

### Setup to reproduce

1. Three machines on three networks, for example a home connection, a phone
   hotspot and an office or second home connection. Ann and Bo on Linux
   (Bazzite counts); Cy on any platform, plus a phone with a browser (the phone app needs
   no install).
2. On each: git, a GitHub login that can clone the test repository, and
   Claude Code signed in with its ACP adapter installed
   (`npm install -g @zed-industries/claude-agent-acp @anthropic-ai/claude-code`).
   `polyph agents doctor` shows each agent as ready.
3. Polyph not installed beforehand, so the install line can be shown:
   `polyph uninstall --purge -y` if it was.
4. A test repository with a small payments-style service: a webhook handler
   with a refund function, one passing test, and two cards' worth of work
   that both touch that function (a retry on timeout and an idempotency
   key). The overlap is needed for the merge shot. Check on a dry run that
   the planner produces two cards that touch the same function; if it does
   not, write the two cards by hand on the board instead and say so in the
   video notes.
5. Screen recording per machine at 1080p or higher, terminal font size
   large enough to read at 720p, dark theme, no notifications, no private
   paths or tokens visible. `polyph` redacts secrets it recognizes in
   terminal output, but check each take before sharing.
6. For the phone shot: run `polyph phone` on Cy's computer and open the link
   on the phone; screen-record the phone. The phone app only shows room
   state and approvals. Starting agents and merging stay on the computers.
7. Do a full dry run first with the stopwatch script in
   `docs/beta-journeys.md` ("First ten minutes"). The video uses that run's
   steps in the same order.
8. Record each segment on its own so a failure does not cost the whole
   take. Keep failed takes. If a step fails or is slow, keep it in the
   notes for the issue tracker.

### Known limits that affect the recording

- The agent's work depends on the model. Output will differ between takes.
- A person joining after work has started does not see agents' earlier
  transcripts. Record the joins before starting agents.
- The planner may produce overlapping cards or not; see step 4.
- Phones cannot join LAN rooms, so the recording uses the internet relay.
- A new person outside a bridged LAN cannot join (`STUBS.md`), so do not use
  `--lan --bridge`.

### After recording

Kao reviews the cut and decides where it is published. Upload and posting
are human-only.

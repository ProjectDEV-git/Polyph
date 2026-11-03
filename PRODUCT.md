# Product

<!-- impeccable:product-schema 1 -->

> Source: every fact below comes from the written product brief (the build
> prompt). The brief's author is unavailable during the build and asked for no
> questions, so no interview took place. Items marked *(inferred)* are my
> reading of the brief, not statements it makes.

## Platform

web

The design language is shared across a Tauri desktop app (macOS, Windows,
Linux), a Next.js website with a browser client and installable phone PWA, VS
Code extension webviews, and a terminal UI. Desktop surfaces follow each OS's
own conventions for menus, shortcuts, tray and window behavior rather than
imitating macOS everywhere.

## Stack

Decided by the brief: Rust core, daemon, CLI and relay; ratatui TUI; Tauri 2
with React + TypeScript for desktop; Next.js App Router on Vercel for the
website, browser client and PWA; shared `packages/ui` with Tailwind CSS v4,
shadcn/ui primitives and motion/react; Excalidraw for whiteboards; xterm.js for
terminals; GSAP + ScrollTrigger and Lenis on marketing pages only.

## Users

Teams of 2 to 10 people who already use coding agents (Claude Code, Codex,
OpenCode, Pi, ACP agents, local models) and keep colliding: two agents editing
one file, nobody knowing what the other's agent is doing, context lost when
work changes hands. Four launch audiences, all from day one:

- **Small startups** — the main paying customers (Pro and Team plans).
- **Students and hackathon teams** — heavy free-plan users who need setup in
  under two minutes. They get the same serious tool as everyone else.
- **Open source maintainers** — contributors' agents coordinating on one
  public repo, with spectator links for the community.
- **Agencies and freelancers** — client projects, with spectator links so
  clients can watch progress without joining.

Secondary audience: spectators (clients, community members) who watch a room
read-only through a link.

## Product Purpose

Polyph lets several people, each on their own machine and network, run their
own coding agents in one shared space. Agents see each other's work, claim
files before editing, talk to each other, and get work split between them by a
planner. Humans chat, steer and approve each other's agents, plan on a shared
board and whiteboard, and watch everything live.

Success: a team of three on different networks installs it, creates a room,
joins by code, and their agents finish a planned goal together, with claims
preventing collisions, reviews before merge, and at least one conflict merged
by keeping both tasks' intent, while every human can follow and approve from
any surface.

## Positioning

Stated plainly on the website, against Amoeba and Murmell:

1. CLI first, with desktop app, browser client, phone app, VS Code extension
   and status pill on top of the same engine.
2. Linux, macOS and Windows from day one, including immutable Linux distros
   (Bazzite, Fedora Atomic).
3. End-to-end encrypted: the relay forwards ciphertext and never sees code,
   chat or board content.
4. Self-hostable relay as one small binary, and a LAN mode that works with no
   internet.
5. Bring any agent and your own subscriptions or keys.
6. A built-in board (kanban, threads, activity feed, whiteboard) as the team's
   home; no Slack or Linear required.
7. Merges that understand intent: conflicts are resolved by keeping what both
   tasks were trying to do.

## Operating Context

- Work happens in git repositories; each agent task runs in its own worktree
  and branch, goes through review, then a merge queue.
- Members are often on different networks; a relay (hosted, self-hosted, or
  embedded for LAN) routes encrypted traffic. Nobody needs port forwarding.
- Approvals arrive on whatever device the owner has: desktop, phone, status
  pill, VS Code, terminal.
- Long sessions: people leave the app open beside their editor and terminal
  all day *(inferred)*. Dense, glanceable status matters more than spectacle
  in the app.
- Some members run agents on their own servers so work continues with the
  laptop closed.

## Capabilities and Constraints

- Rooms (persistent) and sessions (live periods); quick throwaway sessions.
- Join with an 8-character code `A1B2-CDEF` or a link; approval to join on by default.
- Roles: Owner, Maintainer, Member, Viewer.
- Distributed mode (worktree per task) and hosted mode (one shared working copy).
- Claims on files, directories and symbols; overlap detection; planner;
  memory notes pinned to code; agent inboxes; handoff; intent-aware merge;
  reviewer agent.
- Board: kanban, threads, activity feed with replay, multiple whiteboards.
- Follow mode, shared terminals, usage and budgets, model router.
- Four locales at launch: English, Thai, Japanese, Simplified Chinese. No
  hard-coded user-facing strings anywhere.
- Open core: Apache-2.0 engine and apps; proprietary hosted accounts, billing,
  dashboards and push under `cloud/`.
- Plans: Free, Pro, Team. **Prices are undecided** and live in config; UI shows
  "Pricing coming soon" until set. Never invent price amounts.

## Brand Commitments

- Name: **Polyph**, from polyphony: independent voices that sound together.
  Each person and each agent is a "voice" with its own color; agents are
  lighter and darker steps of their owner's hue. Keep the idea subtle: **no
  music notes, staves or piano imagery anywhere.**
- Direction: calm, precise, dark-first, editorial; in the same family as
  Amoeba's site without copying it. Chrome stays quiet so agent and human work
  is the loudest thing on screen.
- Dark is the default theme on website and app, with a fully designed light
  theme. Dark backgrounds are tinted near-blacks, never `#000`.
- Typography: a large serif display face for website headlines and a few pull
  quotes; in the app the serif appears only for room names and empty-state
  headlines. A UI sans (not Inter) and a monospace for code, commands, join
  codes and aligned numbers. All self-hosted, commercially licensed, with
  fallbacks covering Thai, Japanese and Simplified Chinese.
- Status colors for running, waiting for approval, blocked, done and error,
  always paired with an icon or label.
- Logo: a simple geometric mark suggesting several lines moving together.
- Copy voice: direct and specific, written for engineers. No hype words, no
  exclamation marks, no emoji in product copy.
- Students and hackathon teams must not get a playful sticker-and-emoji look.

## Evidence on Hand

None yet. There are no customers, testimonials, benchmarks, press, or
screenshots. Do not fabricate any. The relay memory figure on the website must
come from the measured load test in `docs/self-hosting.md`. Prices are unset.
Legal pages are drafts that need legal review and must say so.

## Product Principles

1. **The work is the loudest thing on screen.** Interface chrome recedes;
   agents' and people's output carries the color.
2. **Show exactly what will happen.** Approvals, claims and merges state the
   command, files and hosts involved, never a vague summary.
3. **Ownership is visible at a glance.** Every action is attributable to a
   voice through color plus a name or label.
4. **Same engine, every surface.** CLI, TUI, desktop, browser, phone, VS Code
   and the pill expose the same state with each platform's conventions.
5. **Private by construction.** Nothing a team writes is readable by Polyph's
   servers; the product says so plainly and never weakens it for convenience.

## Accessibility & Inclusion

- WCAG 2.2 AA, full keyboard support, visible focus, screen reader labels,
  polite and throttled live regions for agent output.
- Respect `prefers-reduced-motion` everywhere (Lenis off, animations reduced
  to fades or nothing).
- Status never relies on color alone.
- Thai, Japanese and Chinese text render correctly: Thai tone marks unclipped
  (extra line height where needed), no italics for CJK, proper line breaking.
- The TUI honors `NO_COLOR` and works over SSH and in common terminals.

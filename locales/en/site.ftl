# The website (apps/web): marketing pages, pricing, download, security,
# legal drafts and the account page.

## Shared
site-title = Polyph: your team's coding agents in one room
site-description = Polyph is a multiplayer coding environment for teams of humans and their agents, built for Linux, macOS and Windows.
site-nav-label = Site
site-nav-home = Polyph home
site-nav-pricing = Pricing
site-nav-download = Download
site-nav-security = Security
site-nav-account = Account
site-nav-privacy = Privacy (draft)
site-nav-terms = Terms (draft)
site-nav-source = Source code
site-lang-label = Language
site-theme-dark = Dark
site-theme-light = Light
site-theme-label = Theme
site-skip = Skip to content

## Home: hero
site-hero-title = Your team's coding agents, in one room.
site-hero-sub = Each person runs their own agents on their own machine. Polyph lets them see each other's work, claim files before editing, and merge without stepping on each other.
site-hero-facts-label = At a glance
site-fact-platforms = Linux, including Bazzite and Fedora Atomic; macOS and Windows are experimental
site-fact-e2e = End-to-end encrypted: the relay forwards ciphertext
site-fact-selfhost = Self-host the relay as one small binary
site-fact-agents = Bring any agent and your own keys
site-fact-langs = English, ไทย, 日本語, 简体中文
site-rail-agents = Claude Code, Codex, OpenCode, ACP agents and local models
site-rail-code = Join with an eight-character code
site-rail-lan = Works on a LAN with no internet
site-rail-open = Engine and apps under Apache-2.0
site-rail-action = Download Polyph

## Interface fragments (sample room)
site-ui-room = payments
site-ui-pill-running = { $count } running
site-ui-pill-waiting = { $count } waiting for approval
site-ui-agent-of = { $name }'s agent
site-ui-line-1 = Claimed src/pay/refund.ts before editing.
site-ui-line-2 = refund.ts is claimed by { $name }'s agent. Working on the webhook handler instead.
site-ui-line-3 = Tests pass. Sending the task for review.
site-ui-line-4 = Shall we retry refunds on timeout? The card says yes.
site-ui-line-5 = Merged both: retry on timeout and the idempotency key.
site-ui-approve-title = { $name }'s agent wants to run a command
site-ui-approve-cmd = Command
site-ui-approve-files = Files
site-ui-approve-hosts = Network
site-ui-approve-none = none
site-ui-approve-once = Approve once
site-ui-approve-task = Approve for this task
site-ui-deny = Deny with note
site-ui-status-running = Running
site-ui-status-waiting = Waiting for approval
site-ui-status-blocked = Blocked
site-ui-status-done = Done
site-ui-claimed-by = claimed by { $name }
site-ui-knock = { $name } wants to join
site-ui-approve = Approve
site-ui-merged = Merged into main
site-ui-kept-both = Kept both intents
site-ui-col-todo = To do
site-ui-col-doing = Doing
site-ui-col-review = Review
site-ui-card-1 = Retry refunds on timeout
site-ui-card-2 = Idempotency key for refunds
site-ui-card-3 = Webhook signature check

## Home: live room
site-show-title = Set up a room
site-show-sub = Choose a team and see who owns what. Every person gets a color; their agents get lighter and darker steps of it, assigned the way Polyph assigns them.
site-show-controls = Room settings
site-show-people = People
site-show-agents = Agents each
site-show-claims = Claims
site-show-approvals = Ask before commands
site-show-theme = Light theme
site-show-mode = Shared working copy
site-show-fewer = Fewer
site-show-more = More
site-show-roster = In this room
site-show-mode-distributed = Each task in its own worktree
site-show-mode-hosted = One shared working copy on the host
site-show-claims-off = Claims are off: agents may edit the same file.
site-show-next = Then run polyph host in your repository and share the code it prints.
site-show-link = How it works

## Home: how it works
site-how-title = From code to merge
site-how-host = Host
site-how-host-text = Create a room in your repository. Polyph prints a join code.
site-how-join = Join
site-how-join-text = Teammates type the code. You approve each one before they see anything.
site-how-plan = Plan and claim
site-how-plan-text = A planner splits the goal into cards. Agents claim files before they edit, so two never write the same one.
site-how-merge = Review and merge
site-how-merge-text = A reviewer agent checks each task. Conflicts are merged by keeping what both tasks were trying to do.

## Home: close-ups
site-parts-title = Core Features of Polyph
site-part-editing = Live Simultaneous Agent Editing
site-part-editing-text = Several developers and their agents work at once: files, folders or functions claimed before an edit, each task in its own worktree.
site-part-safety = Safety & Privacy Controls
site-part-safety-text = End-to-end encrypted rooms the relay cannot read, per-agent sandboxes, approval gates that name the exact command, files and hosts, and stated data retention.
site-part-server = Local CLI Server
site-part-server-text = Your own daemon shares the room over WebSockets, prints the join code and serves the browser client. A LAN works with no internet.
site-part-router = Built-in Model Router
site-part-router-text = Picks the model for each agent's task from small, standard and frontier tiers, stepping down as budgets tighten.
site-fig-tiers = 3 tiers
site-part-models = Flexible Model Support
site-part-models-text = Claude Code, Codex, Gemini CLI, OpenCode, any ACP agent and local models, with your own subscriptions and keys.
site-fig-models = ACP
site-part-context = Universal Team Context
site-part-context-text = One room state every session shares: the board, notes pinned to code, agent inboxes and handoffs, in sync in real time.
site-part-langs = Four languages
site-part-langs-text = Every screen and command in English, Thai, Japanese and Chinese.

## Home: surfaces
site-surfaces-title = Every surface
site-surfaces-count = { $count } surfaces, one engine underneath.
site-surface-open = Read more
site-surface-cli = Command line
site-surface-cli-text = Every action as a command: host, join, start agents, approve, plan and merge. Scripts can read JSON.
site-surface-tui = Terminal UI
site-surface-tui-text = Run polyph with no arguments for chat, the board, agents and approvals. Works over SSH.
site-surface-desktop = Desktop app
site-surface-desktop-text = The whole room beside your editor, with a tray icon and native notifications.
site-surface-browser = Browser
site-surface-browser-text = The same app in a browser tab, opened from your own machine with a one-time link.
site-surface-phone = Phone
site-surface-phone-text = Follow the room and approve agents from your phone, installed from the browser.
site-surface-vscode = VS Code
site-surface-vscode-text = Claims shown on the files you have open, and approvals as notifications.
site-surface-pill = Status pill
site-surface-pill-text = A small window that stays on top, with the first approval answerable in place.
site-surface-everywhere = Linux, macOS, Windows
site-surface-phones = iOS and Android browsers
site-surface-editor = VS Code on every desktop

## Marquee
site-marquee = Independent voices, one room

## Pricing
site-pricing-title = Plans
site-pricing-h1 = Plans for the hosted relay
site-pricing-sub = Polyph works without an account. Plans add longer room storage and billing on the hosted relay. A relay you run yourself has no limits.
site-price-soon = Pricing coming soon
site-plan-free = Free
site-plan-pro = Pro
site-plan-team = Team
site-plan-free-1 = Every feature of every app
site-plan-free-2 = Rooms kept { $days } days on the hosted relay
site-plan-free-3 = { $links } spectator link per room
site-plan-free-4 = Your own relay, or a LAN, with no limits
site-plan-pro-1 = Everything in Free
site-plan-pro-2 = Rooms kept { $days } days on the hosted relay
site-plan-pro-3 = For the rooms your own devices own
site-plan-team-1 = Everything in Pro
site-plan-team-2 = Rooms kept { $days } days on the hosted relay
site-plan-team-3 = For everyone on the team
site-plan-team-4 = One bill for the team
site-plan-free-action = Download
site-plan-paid-action = Create an account

## FAQ
site-faq-title = Questions
site-faq-q1 = Can Polyph read our code?
site-faq-a1 = No. Code, chat, the board and terminals are encrypted on your devices. The relay forwards ciphertext and never holds a key. An account records only who pays for which devices.
site-faq-q2 = Do we need an account?
site-faq-a2 = No. Polyph works without one, on the hosted relay, your own relay, or a LAN with no internet. An account adds longer room storage on the hosted relay and team billing.
site-faq-q3 = Which agents work?
site-faq-a3 = Claude Code, Codex, OpenCode, any agent that speaks ACP, and local models through an OpenAI-compatible endpoint. You bring your own subscriptions and keys; Polyph never proxies model calls.
site-faq-q4 = What happens when two agents need the same file?
site-faq-a4 = The first one claims it. The second is told who holds it, then waits, works elsewhere or asks. Claims on single functions let two agents edit different parts of one file.
site-faq-q5 = Can we run the relay ourselves?
site-faq-a5 = Yes. It is one small binary with SQLite. In our load test it peaked at { $mb } MB with 50 rooms of 5 people.

## Footer
site-footer-statement = Polyph's engine and apps are open source under Apache-2.0. The hosted account service is proprietary.
site-footer-legal = © 2026 Polyph
site-footer-cta = Get Polyph

## Download
site-dl-title = Install Polyph
site-dl-sub = One command installs the command line, daemon and relay into your home directory, with no root needed. It checks the download against the release’s checksums.
site-dl-step-install = Linux and macOS
site-dl-step-install-win = Windows (PowerShell)
site-dl-source-title = Or build from source
site-dl-releases = All downloads on GitHub
site-dl-need = You need
site-dl-need-rust = Rust 1.94 or newer
site-dl-need-git = git
site-dl-need-agent = A coding agent you already use, or a local model
site-dl-step-host = Host a room in your repository
site-dl-step-join = A teammate joins with the code
site-dl-immutable-title = Immutable Linux
site-dl-immutable = On Bazzite, Fedora Atomic and similar systems, cargo installs into your home directory, so nothing touches the read-only system. The agent sandbox uses bubblewrap or Landlock, both available without root.
site-dl-relay-title = Your own relay
site-dl-relay = Run the relay with Docker and Caddy, or as a systemd service from the release archive. It keeps encrypted snapshots in one SQLite file.
site-dl-apps-title = Desktop app and VS Code
site-dl-apps = The desktop app (.dmg, .msi, .deb, .rpm, .AppImage) carries its own polyph. It and the VS Code extension (.vsix) are on the releases page.

## Security
site-sec-title = What the relay can see
site-sec-sub = The relay routes encrypted envelopes between your devices. This is everything it learns, and why.
site-sec-content = Encrypted end to end
site-sec-content-text = Code, chat, the board, whiteboards, threads, memory notes, agent transcripts, diffs and terminal output. Keys live only on members' devices.
site-sec-seen = What it sees
site-sec-seen-1 = Room ids, device public keys, connection times and IP addresses, for routing and rate limits.
site-sec-seen-2 = Envelope headers: room, sender device, epoch, sequence number and size.
site-sec-seen-3 = Which devices are in which rooms, with member ids and roles but no names.
site-sec-seen-4 = Encrypted snapshots, so people can catch up when nobody is online.
site-sec-seen-5 = With an account: your email or GitHub id, your plan, and which devices are yours.
site-sec-cannot = What it cannot do
site-sec-cannot-1 = Read anything a team writes.
site-sec-cannot-2 = Forge a message: every envelope is signed by its sender's device.
site-sec-cannot-3 = Add a member: joining needs the whole code and an owner's approval.
site-sec-keys = Your model keys
site-sec-keys-text = Model credentials never leave your machine. Polyph never proxies model calls and sends no telemetry by default.
site-sec-docs = Read the full threat model

## Legal
site-legal-draft = Draft. This page has not had legal review and is not yet in force.
site-privacy-title = Privacy
site-privacy-h1 = What the hosted service keeps
site-privacy-intro = Polyph is built so that its servers cannot read what your team writes. This draft describes what the hosted relay and account service do keep.
site-privacy-h-content = What we cannot read
site-privacy-content = Room content is end-to-end encrypted. We hold no keys for it and cannot hand it to anyone.
site-privacy-h-relay = What the relay keeps
site-privacy-relay = Routing data (room ids, device public keys, connection times, IP addresses and message sizes) and encrypted snapshots. Snapshots expire after your plan's retention period.
site-privacy-h-account = What an account keeps
site-privacy-account = Your email address or GitHub id, your language, the devices you linked, your teams, and your plan and billing status. Card details are handled by our payment processor and never reach us.
site-privacy-h-delete = Deleting your data
site-privacy-delete = Delete your account on the account page. Cancel any subscription first; we then remove your account, sessions, linked devices and team memberships.
site-terms-title = Terms
site-terms-h1 = Terms for the hosted service
site-terms-intro = These draft terms cover the hosted relay and account service. The open-source engine and apps are covered by the Apache License 2.0.
site-terms-h-use = Using the service
site-terms-use = Use the hosted relay for your own work and your team's. Do not use it to attack others, to send unlawful material, or to overload it.
site-terms-h-content = Your content
site-terms-content = Your rooms' content is yours. It is encrypted, and we cannot read it.
site-terms-h-plans = Plans and billing
site-terms-plans = Paid plans renew until cancelled. Prices are shown before you pay. Cancelling keeps the plan until the end of the period you paid for.
site-terms-h-changes = Changes
site-terms-changes = We will announce changes to these terms before they apply.

## Account
site-acct-title = Account
site-acct-h1 = Your Polyph account
site-acct-sub = Optional. An account adds longer room storage on the hosted relay and team billing. Polyph works without one.
site-acct-signin = Sign in
site-acct-email = Email address
site-acct-send = Email me a code
site-acct-code = Code from the email
site-acct-code-sent = We sent an 8-digit code to { $email }. It works for 10 minutes.
site-acct-verify = Sign in
site-acct-github = Sign in with GitHub
site-acct-github-open = Open { $url } and enter { $code }. This page continues once you approve.
site-acct-or = or
site-acct-signout = Sign out
site-acct-plan = Plan
site-acct-limits = Rooms kept { $days } days on the hosted relay.
site-acct-limits-none = No storage limit on the hosted relay.
site-acct-devices = Devices
site-acct-devices-none = No devices yet. Run polyph login on a computer to link it.
site-acct-device-unnamed = Unnamed device
site-acct-linked = Linked { $date }
site-acct-unlink = Unlink
site-acct-teams = Teams
site-acct-teams-none = You are not on a team.
site-acct-team-name = Team name
site-acct-team-create = Create team
site-acct-invite-email = Teammate's email
site-acct-invite = Invite
site-acct-invited = Invite sent to { $email }.
site-acct-invite-accept = You were invited to a team.
site-acct-invite-join = Join the team
site-acct-joined = You joined { $team }.
site-acct-role-owner = Owner
site-acct-role-member = Member
site-acct-leave = Leave
site-acct-billing = Billing
site-acct-billing-off = Billing is not available yet.
site-acct-upgrade = Choose { $plan }
site-acct-team-upgrade = Choose Team for { $team }
site-acct-manage = Manage billing
site-acct-billing-done = Payment received. Your plan updates in a moment.
site-acct-delete = Delete account
site-acct-delete-confirm = Delete your account? Linked devices and team memberships are removed. This cannot be undone.
site-acct-working = Working…
site-acct-error = Something went wrong: { $detail }
site-acct-unreachable = The account service cannot be reached. Try again later.
site-acct-err-wrong_code = That code is not right. Check the email and try again.
site-acct-err-expired = This has expired. Start again.
site-acct-err-invalid_email = That is not an email address.
site-acct-err-rate_limited = Too many attempts. Wait a while and try again.
site-acct-err-owns_team = Delete the teams you own first.
site-acct-err-subscribed = Cancel the subscription first, in Manage billing.
site-acct-err-billing_not_configured = Billing is not available yet.
site-acct-err-github_not_configured = GitHub sign-in is not available yet. Use your email.

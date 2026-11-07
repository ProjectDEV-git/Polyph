# Security findings for the public beta (M-B0)

Audit of the tree at `/home/user/Polyph` on 2026-11-06, by code reading only.
Nothing here was run against a live system unless the finding says so. Where
a finding depends on how the operating system or a library behaves rather
than on code in this repository, it says so.

Severity: critical (an agent or outsider defeats a core boundary
deterministically), high (a core promise in `docs/security.md` does not hold
under a realistic attacker), medium (a weaker or conditional break), low,
info.

"Wire change" means the fix changes bytes on the wire or crypto that
existing rooms or clients depend on. Those need Kao.

Summary:

| Id | Severity | Area | Title |
|---|---|---|---|
| SEC-1 | high | E2EE / room core | Relay snapshots skip role, revocation and replay checks |
| SEC-2 | high | E2EE / room core | Demotion is not enforced: an older, higher-role certificate stays valid |
| SEC-3 | high | Join | A joiner chooses its own member id and can become an existing member |
| SEC-4 | high | Join / PAKE | Join-code lockout is bypassed by abandoning a session after the host's confirmation |
| SEC-5 | medium | E2EE / room core | Epoch u64::MAX makes later key rotations silently fail |
| SEC-6 | medium | E2EE / room core | Revocation is per device the remover saw, not per member |
| SEC-7 | medium | E2EE / replay | Replay window is not persisted; any writer can re-inject old envelopes after a restart |
| SEC-8 | medium | Relay | Anyone who knows a room id can read its ciphertext stream and snapshots; removed members stay writers |
| SEC-9 | medium | Relay | Unbounded room entries and chain verification per frame allow memory and CPU exhaustion |
| SEC-10 | low | Relay | Relay authentication is not bound to the relay |
| SEC-11 | low | Relay | First `X-Forwarded-For` entry is trusted from loopback |
| SEC-12 | critical | Sandbox | `polyph.toml` in the worktree turns the sandbox off and defines agent commands |
| SEC-13 | critical | Sandbox | Agents can write git configuration that the daemon then executes unsandboxed |
| SEC-14 | critical | Sandbox | Host IPC (D-Bus session bus, Unix sockets, LaunchServices) is reachable from the sandbox |
| SEC-15 | high | Sandbox (Landlock) | Landlock mode hides nothing and silently drops its network rule on older kernels |
| SEC-16 | high | Sandbox / daemon | The daemon follows agent-made symlinks and returns hidden files to the agent |
| SEC-17 | high | Sandbox (Windows) | Low-integrity agents can read the API token and drive the daemon API |
| SEC-18 | medium | Sandbox | Hidden-path list is short; allowlisted hosts and any port allow exfiltration |
| SEC-19 | medium | Redaction | `.polyphignore` does not exist; hosted-mode file mirrors are not redacted |
| SEC-20 | medium | Redaction | Terminal redaction fails on non-UTF-8 batches, split PEM blocks and split key/value pairs |
| SEC-21 | medium | Install / update | Releases are checked against a checksum from the same origin; nothing is signed |
| SEC-22 | low | Install / update | Multi-file swaps have no rollback; `curl \| sh` can run a truncated script |
| SEC-23 | low | MCP | `forget` and `move_task` act on other people's notes and cards |
| SEC-24 | info | MCP | Tool calls are bound to the calling agent and its room (no cross-room path found) |
| SEC-25 | low | Docs | `docs/security.md` claims protections the code does not have |

---

## SEC-1: Relay snapshots skip role, revocation and replay checks

- Severity: high
- Area: E2EE / room core
- Evidence:
  - `crates/polyph-core/src/room.rs:1632-1673` (`apply_relay_snapshot`):
    opens the blob with any epoch key the client still holds
    (`self.keyring.get(env.epoch)`), and when the sender is not a known peer
    it accepts any chain inside the snapshot with `c.verify(now, &|_| false)`,
    that is, ignoring revocations. It never checks `self.revoked` for
    `env.sender` and never checks the sender's role. It then unions
    `snap.revoked` into the local revocation set, applies every document
    update, and merges claims.
  - The normal message path refuses document writes from viewers
    (`room.rs:623`, `RoomMessage::Crdt { .. } if writer`); the snapshot path
    has no such gate.
  - Old epoch keys are never dropped: `Keyring::retain_from`
    (`crates/polyph-proto/src/group.rs`) has no callers outside tests.
  - The relay accepts `PutSnapshot` from any writer subscription
    (`crates/polyph-relay/src/hub.rs:222-237`), and a writer is anyone with a
    chain that verifies without revocations (`hub.rs:281-309`), Viewers
    included.
  - Clients fetch and apply snapshots on every `Gap` and on first catch-up
    (`crates/polyph-daemon/src/runtime.rs:481-503`;
    `crates/polyph-phone/src/lib.rs:390-404`). The relay decides when to send
    `Gap`.
- Exploit:
  1. A Viewer seals a snapshot under the current epoch with Board, Threads,
     Memory and Settings updates of their choosing (cards, chat, memory
     notes, budgets, `require_approval = false`) and `revoked` listing every
     other device. Every client that next catches up applies it, drops all
     other members' envelopes from then on (`room.rs:544`), and shows the
     injected content to people and agents (memory notes go into agent
     context: a prompt-injection channel).
  2. A removed member keeps all old epoch keys. Their chain still verifies
     at the relay (no revocations there) and in the snapshot path (no
     revocation check), so they can do the same under an old epoch.
- Fix:
  - In `apply_relay_snapshot`: drop snapshots whose sender is in
    `self.revoked`; require the sender's chain to verify against
    `self.revoked` with role at least Member; only accept the current epoch
    (or current minus one during rotation).
  - Never take revocations from a snapshot on trust. Either accept
    `revoked` entries only from a sender whose role can rotate keys, or
    (better) carry the signed `MemberRemoved` statements and verify them.
  - Relay: refuse `PutSnapshot` from Viewer chains.
  - Call `retain_from` once peers have moved to a new epoch.
- Wire change: the client checks and the relay role check, no. Carrying
  signed revocation statements inside snapshots changes the encrypted
  snapshot format; that part needs Kao.

## SEC-2: Demotion is not enforced

- Severity: high
- Area: E2EE / room core (certificates)
- Evidence:
  - `crates/polyph-proto/src/cert.rs:191-240`: a chain is valid if every
    link verifies and is not expired. Nothing makes a newer certificate for
    the same device or member supersede an older one.
  - `crates/polyph-core/src/room.rs:980-1008` (`change_role`) issues a new
    chain for the first matching device only and broadcasts `RoleChanged`.
    Other devices of the same member keep their old role.
  - `room.rs:656-667` accepts any `RoleChanged` chain that verifies, from any
    verified sender, and replaces the stored chain. `on_hello`
    (`room.rs:781-797`) also replaces the stored chain with whatever the
    device presents.
- Exploit: a Maintainer is demoted to Member. Their client keeps the old
  Maintainer chain and sends it in `Hello` or `RoleChanged`. Every peer
  accepts it and treats them as Maintainer again: they can rotate keys
  (`room.rs:680`), approve joins, issue certificates and remove members
  (`room.rs:669`). The relay also accepts the old chain.
- Fix: within a room, for each (member, device), keep the certificate with
  the highest `issued_at` signed by a valid issuer and reject chains whose
  leaf is older than the one held. Re-issue for all devices of the member in
  `change_role`. A stronger option: demotion revokes the device and issues a
  certificate to a fresh device key.
- Wire change: "newest wins" is client logic, no. A signed revocation or
  supersession record would be; needs Kao.

## SEC-3: A joiner chooses its own member id and can become an existing member

- Severity: high (critical in rooms with `require_approval = false`)
- Area: Join
- Evidence:
  - `crates/polyph-proto/src/join.rs` `JoinRequest.member` is chosen by the
    joiner. The host copies it into the knock
    (`crates/polyph-daemon/src/runtime.rs:620-633`).
  - `crates/polyph-core/src/room.rs:859-907` (`approve_knock`) issues a
    certificate with `member: k.member` without checking whether that member
    id already belongs to someone in the room. It also grants the
    `agent-host` capability if the joiner asked for it.
  - Authorization elsewhere is by member id: agent hosts act on requests
    from any device of their member (`crates/polyph-daemon/src/hosts.rs:207`);
    terminal input is accepted when `typist == member` and that member holds
    the keyboard (`crates/polyph-daemon/src/terms.rs:161-168`); inbox
    honesty checks compare member ids (`room.rs:750-760`).
  - Member ids are visible to every member and to the relay (in chains).
- Exploit: someone with a join code (a contractor invited as a plain
  Member, or anyone if the code leaks) sends `member = <Alice's member id>`
  and `display_name = "Alice"`. If approval is off the device is admitted
  automatically; if it is on, the approver sees "Alice" asking to join,
  which looks like Alice adding a device. The new device then starts agents
  with arbitrary prompts on Alice's agent host, types into any terminal
  where Alice holds the keyboard (code execution on the terminal owner's
  machine), and receives inbox messages addressed to Alice's member.
- Fix: in `approve_knock` (and before auto-approval) refuse a knock whose
  member id already exists in the room unless the request is proven to come
  from that member, for example a signature by one of the member's existing
  devices over the joiner's device key (the phone pairing flow can carry
  this). Show "claims to be an existing member" in the approval UI. Show
  requested capabilities explicitly.
- Wire change: refusing collisions, no. Adding a device-link proof to
  `JoinRequest` is a new optional field; needs Kao.

## SEC-4: Join-code lockout is bypassed by abandoning a session

- Severity: high
- Area: Join / PAKE
- Evidence:
  - The host sends its confirmation tag as soon as it has the joiner's PAKE
    message (`crates/polyph-daemon/src/runtime.rs:592-596`), before it has
    seen the joiner's tag.
  - The host counts a failure only on a malformed PAKE message or a wrong
    tag (`runtime.rs:597-616`). `PakeEnded` just forgets the session
    (`runtime.rs:551-553`).
  - The relay counts a failure on host-reported failure and on joiner
    disconnect (`crates/polyph-relay/src/hub.rs:518-546, 598-630`), but not
    when a session expires in `sweep` (`hub.rs:632-645`, 15-minute TTL).
  - Per-IP join limits key on the full address
    (`crates/polyph-relay/src/limiter.rs:57`, `hub.rs:449`), so each IPv6
    /128 gets its own budget.
- Exploit: the attacker opens a session, sends a SPAKE2 message for one
  guessed code, receives the host's message and tag, checks the guess
  offline, and keeps the connection open until the session expires. No
  failure is counted anywhere. Repeating this from many IPv6 addresses, or
  as the relay (which knows the first half), searches the remaining 2^20
  codes with no lockout. Reusable codes live up to 30 days
  (`crates/polyph-relay/src/config.rs:78`). This voids the "about 5 in 2^20 per code" bound in
  `docs/security.md`. Approval-by-default limits the damage to a pending
  knock, which combines badly with SEC-3.
- Fix:
  - Host: do not send the confirmation tag until the joiner's tag has
    verified. Both existing joiners (`crates/polyph-daemon/src/daemon.rs:2206-2211`,
    `crates/polyph-phone/src/lib.rs:452-458`) already send their tag without
    waiting for the host's, so this reorder is compatible.
  - Host and relay: count every session that ends without a verified tag
    (timeout, `PakeEnded`, sweep) as a failure.
  - Relay: key per-IP limits on /64 for IPv6 and cap concurrent sessions
    per code.
- Wire change: no.

## SEC-5: Epoch u64::MAX makes later key rotations silently fail

- Severity: medium
- Area: E2EE / room core
- Evidence: `crates/polyph-core/src/room.rs:680-697` installs whatever
  epoch the signed delivery names; `Keyring::insert`
  (`crates/polyph-proto/src/group.rs`) makes the highest epoch current.
  `rotate` computes `self.epoch() + 1` (`room.rs:954`). Release profiles do
  not enable overflow checks (`Cargo.toml`), so `u64::MAX + 1` wraps to 0;
  the new key is inserted under epoch 0 and `current` stays at `u64::MAX`.
  Debug builds panic instead.
- Exploit: a Maintainer (or a demoted one, see SEC-2) rotates to epoch
  `u64::MAX`. Later, after they are removed, the owner's rotation produces
  epoch 0, which no client uses for sending. Everyone keeps sealing under
  the `u64::MAX` key, which the removed member holds.
- Fix: accept a delivered epoch only if it is exactly the current epoch
  plus one (or within a small bound); use `checked_add` in `rotate` and
  refuse on overflow.
- Wire change: no.

## SEC-6: Revocation is per device the remover saw, not per member

- Severity: medium
- Area: E2EE / room core
- Evidence: `remove_member` revokes only devices present in the remover's
  `peers` (`crates/polyph-core/src/room.rs:934-942`). Receivers revoke only
  the listed devices (`room.rs:669-679`). Any later `rotate` by anyone seals
  the new key to every non-revoked peer (`room.rs:956-963`).
- Exploit: Bob has a laptop and a server device. The server was offline
  when Alice removed Bob, so Alice never had it in `peers`. The server's
  chain still verifies everywhere; when it comes back it says `Hello`, is
  added as a peer by other members, and receives the next epoch key from
  whoever rotates next.
- Fix: revoke by member id. On `MemberRemoved`, revoke every chain whose
  leaf names that member, now and in the future, and persist it.
- Wire change: no; `MemberRemoved` already carries the member.

## SEC-7: Replay window is not persisted

- Severity: medium
- Area: E2EE / replay
- Evidence: `SavedRoom` (`crates/polyph-core/src/room.rs:268-286`) stores no
  replay state, and `load` starts with an empty `ReplayGuard`
  (`room.rs:1586-1590`). The relay's `Forward` frame lets any writer
  publish envelopes signed by another member
  (`crates/polyph-relay/src/hub.rs:221, 342-346`).
- Exploit: after a client restarts, the relay, or any member through
  `Forward`, re-delivers envelopes of the current epoch that the client has
  already processed. They pass the replay check once. CRDT updates are
  idempotent, but chat events, knocks, approval requests and inbox messages
  (which agents act on) are re-emitted.
- Also: `load` skips own `seq` ahead by 1,000 (`room.rs:1590`); more than
  1,000 sends between saves followed by a crash reuses sequence numbers,
  which peers then drop as replays.
- Fix: persist the highest sequence per (sender, epoch) with the room and
  treat anything at or below it as a replay after restart (the 1,024-wide
  window can restart empty above that mark). Persist own `seq` before
  sending, or skip ahead by a time-derived amount.
- Wire change: no.

## SEC-8: Anyone who knows a room id can read its traffic; removed members stay writers

- Severity: medium
- Area: Relay (metadata)
- Evidence: `Subscribe` without a chain gives a read-only subscription to
  any room id, including the full replay buffer
  (`crates/polyph-relay/src/hub.rs:272-331`). `GetSnapshots` needs only a
  read-only subscription (`hub.rs:238-250`). Chain verification at the relay
  ignores revocations (`hub.rs:282`).
- Exploit: anyone who learns a room id (it appears in links, logs and
  pairing flows) gets every envelope header (sender device, epoch, seq,
  size, timing) and every encrypted snapshot, from any machine, without
  being a member. A removed member can still publish, store snapshots
  (feeding SEC-1) and `Forward` (feeding SEC-7).
- Fix: spectators are not in the beta (`STUBS.md`), so require a verified
  chain for `Subscribe` and `GetSnapshots`. Let the relay learn revocations
  (a signed removal frame from a Maintainer) and refuse revoked devices.
- Wire change: making the chain mandatory is a behaviour change, not a
  format change. A relay revocation frame is a new frame; needs Kao.

## SEC-9: Relay memory and CPU exhaustion

- Severity: medium
- Area: Relay (rate limits)
- Evidence: every `Subscribe` creates a `Room` entry and a subscription
  for any room id (`crates/polyph-relay/src/hub.rs:310-312`), with no cap
  per connection or in total; entries go only when the connection closes.
  Each `Subscribe` with a chain runs up to nine Ed25519 verifications and a
  database call (`hub.rs:281-307`). The per-connection budget is 200 frames
  per second (`crates/polyph-relay/src/config.rs:75`) and 120 new connections per IP per minute.
- Exploit: one client subscribes to millions of random room ids, or sends
  maximum-length chains at 200 frames per second per connection across many
  connections, until the relay runs out of memory or CPU.
- Fix: cap rooms per connection (for example 64) and total rooms; charge
  chain-bearing frames more of the bucket; cache verified chains by hash.
- Wire change: no.

## SEC-10: Relay authentication is not bound to the relay

- Severity: low
- Area: Relay (device auth)
- Evidence: the device signs `nonce || device`
  (`crates/polyph-proto/src/relay_wire.rs:238-242`), with no relay URL or
  TLS identity.
- Exploit: a malicious relay that a member connects to (for example a
  self-hosted one in another room) forwards another relay's challenge and
  authenticates there as that device. It cannot sign envelopes, but it can
  overwrite that device's snapshot slot (`INSERT OR REPLACE` by room and
  device, `crates/polyph-relay/src/store.rs:133`), revoke its join codes, or
  subscribe as it.
- Fix: sign the relay's origin (and, with TLS, a channel binding) together
  with the nonce.
- Wire change: yes; needs Kao.

## SEC-11: First X-Forwarded-For entry trusted from loopback

- Severity: low
- Area: Relay (rate limits)
- Evidence: `crates/polyph-relay/src/server.rs:54-72` takes the first
  `X-Forwarded-For` entry when the peer is loopback.
- Exploit: behind a proxy that appends to an incoming `X-Forwarded-For`
  (nginx with `$proxy_add_x_forwarded_for`), a client picks its own address
  for every limit. The shipped Caddy setup replaces the header by default,
  so the default deployment is not affected.
- Fix: take the last entry (the one the local proxy added), or document
  that only header-replacing proxies are supported.
- Wire change: no.

## SEC-12: `polyph.toml` in the worktree turns the sandbox off and defines agent commands

- Severity: critical
- Area: Sandbox
- Evidence:
  - `crates/polyph-daemon/src/agents.rs:349-360`: `[sandbox] mode = "off"`
    in the project file gives `Mechanism::None`, unless the person set a
    daemon-level override.
  - `agents.rs:382-410`: the project file is read from the agent's
    worktree (`ProjectConfig::load(&worktree)`,
    `crates/polyph-agents/src/project.rs:112-125`), which the agent can
    write. The same file supplies `[agents.*]` definitions, which replace
    built-in agents of the same name (`crates/polyph-agents/src/registry.rs:143-161`)
    including `command`, `env`, `writable` and `hosts`; `[sandbox]
    allow_hosts`; `checks` (commands that run without approval); and
    `[policy] preset`.
  - No trust prompt or per-user record gates any of this.
- Exploit:
  1. A person opens a cloned repository whose `polyph.toml` says
     `[sandbox] mode = "off"` or redefines `claude` with
     `command = "sh"`. The next agent runs unsandboxed as the person.
  2. A prompt-injected agent writes those lines into `polyph.toml` in its
     worktree. The next agent started there (the same person's, or a
     teammate's after the branch merges) runs unsandboxed.
- Fix: never take security-relevant settings (`sandbox.*`, `agents.*`,
  `checks`, `policy.preset`) from the worktree without a per-user trust
  record keyed to the file's hash, set by a person through the CLI or UI
  (similar to `direnv allow`). Read the project file from the base commit,
  not from the agent's worktree, and mount it read-only inside the sandbox.
- Wire change: no.

## SEC-13: Agents can write git configuration that the daemon executes unsandboxed

- Severity: critical
- Area: Sandbox
- Evidence:
  - The whole worktree is writable inside every mechanism
    (`crates/polyph-sandbox/src/lib.rs:150-167`, `198-200`, `300-308`,
    `233-235`). Without a task the worktree is the person's own checkout,
    including `.git/` (`crates/polyph-daemon/src/agents.rs:380`). In a task
    worktree, `.git` is a file that names the git directory.
  - The daemon runs the system `git` in the agent's worktree, outside any
    sandbox, with no `-c core.fsmonitor=` or `core.hooksPath` override
    (`crates/polyph-git/src/lib.rs:70-80`), for example `commit_all` on
    hand-off (`crates/polyph-daemon/src/tasks.rs:879-880`).
- Exploit: the agent writes `core.fsmonitor = /path/in/worktree/x.sh` into
  `.git/config` (own checkout), or replaces its worktree's `.git` file with
  `gitdir: <worktree>/.evil` pointing at a git directory it built with that
  config and hooks. The next `git status`, `commit` or `diff` the daemon or
  the person runs executes `x.sh` as the person, with no sandbox.
- Fix: mount `.git` (the file and the directory it points to) read-only
  inside the sandbox, or keep agents' git directories outside the
  worktree; in `polyph-git`, run every command with
  `-c core.fsmonitor= -c core.hooksPath=/dev/null` (except where hooks are
  wanted and the repository is trusted), resolve and verify the `gitdir`
  before running, and set `GIT_CONFIG_NOSYSTEM`-style protections where
  possible.
- Wire change: no.

## SEC-14: Host IPC is reachable from the sandbox

- Severity: critical on desktop Linux and macOS (not reproduced here; this
  environment has no session bus and no Mac)
- Area: Sandbox
- Evidence:
  - bubblewrap mode bind-mounts `/` read-only and replaces only `/tmp`,
    `/dev`, `/proc` and the hidden paths
    (`crates/polyph-sandbox/src/lib.rs:135-167`). `/run/user/<uid>` (the
    D-Bus session bus, `systemd --user`, gpg and keyring agent sockets) and
    `/var/run/docker.sock` stay visible. A read-only mount does not stop
    `connect()` on a Unix socket, and `--unshare-net` does not affect path
    sockets.
  - Landlock mode does not restrict Unix sockets at all and shares the host
    network namespace, so abstract sockets (X11) are reachable too
    (`crates/polyph-sandbox/src/helper.rs:132-155`).
  - The Seatbelt profile starts from `(allow default)` and explicitly
    allows `network-outbound (remote unix-socket)`
    (`lib.rs:296, 312`); Mach lookups and LaunchServices stay allowed.
- Exploit: on Linux, `systemd-run --user sh -c '...'` (or
  `gdbus call ... org.freedesktop.systemd1.Manager.StartTransientUnit`)
  starts a process outside the sandbox as the person; the Secret Service on
  the same bus hands out stored secrets, including Polyph's device key if
  it is kept there; `docker.sock`, where present, gives root. On macOS,
  `open -a Terminal <worktree>/x.command` runs a script through
  LaunchServices outside the profile.
- Fix: bubblewrap: add `--tmpfs /run` (re-binding only what is needed),
  hide `/var/run/docker.sock`, add `--unshare-ipc --unshare-uts`, and pass
  no D-Bus address; if D-Bus is needed, use a filtering proxy as Flatpak
  does. Landlock: add the seccomp filter from `STUBS.md` and deny
  `connect` on `AF_UNIX` except the proxy. Seatbelt: start from
  `(deny default)` and allow what agents need; deny `process-exec` of
  `/usr/bin/open` and `osascript` and restrict `mach-lookup`.
- Wire change: no.

## SEC-15: Landlock mode hides nothing and silently drops its network rule on older kernels

- Severity: high
- Area: Sandbox (Landlock)
- Evidence:
  - `crates/polyph-sandbox/src/lib.rs:190-205`: the Landlock wrapper passes
    only writable paths; `policy.hidden` (`~/.ssh`, `~/.aws`, Polyph's own
    data directory, and so on) is ignored. `helper.rs:143-144` grants read
    on `/`.
  - `helper.rs:137-154`: the ruleset uses the `landlock` crate's default
    best-effort mode with ABI V5. On kernels without Landlock network
    support (before 6.7, which includes Debian 12 and Ubuntu 22.04), the
    TCP rule is dropped and `restrict_self` reports partial enforcement,
    which the helper accepts (only `NotEnforced` is refused). Detection
    probes only the file rules (`helper.rs:199`, `lib.rs:351-357`). The
    self-test that would notice (`selftest.rs`) is not used to gate agent
    start (only `polyph sandbox test` runs it, `daemon.rs:1825`).
  - UDP and raw sockets are unrestricted (known, `STUBS.md`).
- Exploit: on a machine without working bubblewrap, an agent reads
  `~/.ssh/id_*`, cloud credentials and Polyph's home (`api.token`, the
  device key file or the local secret that protects it) and sends them out
  directly over TCP on older kernels, or over DNS/UDP on any kernel.
- Fix: refuse Landlock mode unless the network rule is fully enforced
  (`CompatLevel::HardRequirement` for the network access rights); hide
  paths by granting read only on an explicit allow list instead of `/`;
  add the seccomp filter (deny `socket(AF_INET*, SOCK_DGRAM|SOCK_RAW)` and
  `AF_UNIX` connects); run the self-test before the first agent and refuse
  mechanisms that fail it.
- Wire change: no.

## SEC-16: The daemon follows agent-made symlinks and returns hidden files

- Severity: high
- Area: Sandbox / daemon (confused deputy)
- Evidence: the daemon, outside the sandbox, reads files at fixed paths in
  the agent's worktree and returns their text to the agent:
  `crates/polyph-daemon/src/context.rs:52-57` (`.polyph/brief.md`, used by
  the context tool at `context.rs:215-218`), and similar reads in
  `crates/polyph-daemon/src/areas.rs:176-182` and skill discovery
  (`crates/polyph-plugins/src/skills.rs:90-96`). `std::fs::read_to_string`
  follows symlinks.
- Exploit: the agent runs `ln -s ~/.ssh/id_ed25519 .polyph/brief.md` (or
  points it at Polyph's `api.token` or key file) and calls the context tool.
  The daemon returns the hidden file's contents as the "brief". The
  agent can then send it to the model provider or an allowlisted host.
- Fix: open worktree files with `O_NOFOLLOW` on every component (or
  canonicalize and require the result to stay under the worktree and not
  be a symlink), and cap sizes. Apply the same rule to every daemon read
  under an agent's worktree.
- Wire change: no.

## SEC-17: Windows agents can read the API token and drive the daemon API

- Severity: high (inferred from code; not run on Windows)
- Area: Sandbox (Windows low integrity)
- Evidence: `crates/polyph-sandbox/src/lowil.rs:15-19` states that reads
  are not restricted and the network is not isolated. The daemon API
  token lives in Polyph's home (`crates/polyph-daemon/src/paths.rs:41`) and
  the API listens on loopback TCP. Low integrity does not stop reading
  medium-integrity files under the default no-write-up policy, and does
  not stop loopback connections.
- Exploit: an agent reads `api.token`, then calls the daemon's local API
  directly: approves its own pending requests, starts unsandboxed agents
  (with SEC-12), joins or creates rooms, or reads every room's content.
- Fix: before the Windows beta, run agents in an AppContainer (which
  blocks loopback by default and file reads outside granted folders), or
  protect `api.token` with an explicit DACL denying the low-integrity
  label (a mandatory label with no-read-up on Polyph's home). Until then,
  say in the UI that the Windows sandbox does not protect Polyph itself.
- Wire change: no.

## SEC-18: Short hidden-path list; allowlist and ports allow exfiltration

- Severity: medium
- Area: Sandbox
- Evidence: `crates/polyph-sandbox/src/lib.rs:56-66` hides `.ssh`, `.aws`,
  `.gnupg`, `.config/gcloud`, `.azure`, `.kube`, `.docker`, `.netrc`,
  `.git-credentials` and Polyph's home. Not hidden: `~/.config/gh` (GitHub
  CLI token), `~/.npmrc`, `~/.pypirc`, `~/.cargo/credentials.toml`,
  `~/.local/share/keyrings`, browser profiles, shell history, and other
  agents' login files (Codex's when running Claude Code, and the reverse).
  The proxy allows any port on allowlisted hosts
  (`crates/polyph-sandbox/src/proxy.rs:254`), and the default allowlist
  (`proxy.rs:51-80`) includes hosts that accept writes from any account
  (`github.com`, `api.openai.com`, `openrouter.ai`, package registries).
- Exploit: a prompt-injected agent reads `~/.config/gh/hosts.yml` and
  creates a gist, or posts it to a model API with an attacker's key.
- Fix: hide the listed paths and other agents' credential directories;
  restrict proxy ports to 443 (and 80) unless a host entry names a port.
  Exfiltration to allowlisted hosts cannot be fully closed by a host
  allowlist; say so in `docs/sandbox.md`.
- Wire change: no.

## SEC-19: `.polyphignore` does not exist; hosted-mode mirrors are not redacted

- Severity: medium
- Area: Redaction
- Evidence: `docs/security.md` promises that `.polyphignore` paths
  (default `.env*`, `*.pem`, `*.key`, `id_*`, `.aws/`, `.ssh/`) are never
  shared and that file mirrors are redacted. No code reads `.polyphignore`
  (the name appears only in `docs/security.md`). Hosted mode mirrors every
  file that is not git-ignored and not in a skip directory
  (`crates/polyph-daemon/src/mirror.rs:92-131`); if the folder is not a git
  repository, nothing is ignored. `mirror.rs` never calls the redactor.
- Exploit: a host shares a working copy with an untracked `.env` or a
  committed `deploy.pem`. Every member receives the file as-is.
- Fix: implement `.polyphignore` with the documented defaults, applied to
  mirrors, diffs and file tools; run the redactor over text files before
  mirroring (or refuse files that match a rule).
- Wire change: no.

## SEC-20: Terminal redaction gaps

- Severity: medium
- Area: Redaction (terminal batches)
- Evidence:
  - `crates/polyph-daemon/src/terms.rs:102-107`: a batch that is not valid
    UTF-8 is sent unredacted. Batches are up to 64 KB
    (`terms.rs:31`), and a split inside a multi-byte character, or one
    stray non-UTF-8 byte from any program, disables redaction for the whole
    batch.
  - `terms.rs:93-99`: only the last word (text after the last whitespace)
    is held for the next batch. Rules that need context across whitespace
    lose it when the boundary falls there: `generic-secret`
    (`password = <value>`) and `aws-secret-key`
    (`crates/polyph-core/src/redact.rs:38-61`).
  - The private-key rule (`redact.rs:31-36`) needs `BEGIN` in the same
    batch; when a key spans batches, the first batch is redacted to its
    end but the next batch (remaining base64 lines and the `END` line) is
    sent as-is.
  - Held text waits 150 ms (`terms.rs:89`); keys typed slower than that are
    sent a character at a time.
- Fix: redact on bytes (`regex::bytes`) or on valid UTF-8 segments; hold the
  last line rather than the last word; keep "inside a PEM block" state
  across batches and redact until `END`; strip ANSI sequences before
  matching. Add tests for each case.
- Wire change: no.

## SEC-21: Releases are not signed

- Severity: medium
- Area: Install / update
- Evidence: `scripts/install.sh:61-73`, `scripts/install.ps1:34-42` and
  `crates/polyph-cli/src/update.rs:219-247` fetch `SHA256SUMS` from the same
  release as the archive and compare. This catches corruption and partial
  downloads (verified in code: the archive is hashed in memory before it is
  written, `update.rs:243-247`; `STUBS.md` records a tampered-checksum test
  for `install.sh`), but anyone who can publish a release (a stolen
  maintainer token, a compromised CI job) ships matching sums.
- Fix: sign `SHA256SUMS` (minisign, or Sigstore keyless with GitHub
  artifact attestations), embed the public key or identity in `update.rs`
  and the install scripts, and refuse unsigned releases. Choosing and
  holding the key is a human decision.
- Wire change: no.

## SEC-22: No rollback for multi-file swaps; piped script can run truncated

- Severity: low
- Area: Install / update
- Evidence: `install.sh:78-87`, `install.ps1:47-50` and `update.rs:167-199`
  replace `polyph`, `polyph-relay` and `share/polyph` one after another;
  a failure part-way (disk full, a locked file on Windows) leaves mixed
  versions, and the old Unix binaries are not kept. `install.sh` is not
  wrapped in a function, so `curl | sh` cut off after line 86 runs the
  `rm -rf "$PREFIX/share/polyph"` without the following `mv`.
  `POLYPH_UPDATE_BASE` and `POLYPH_INSTALL_BASE` accept `http://` and local
  folders (`update.rs:220-222`).
- Fix: keep the previous files as `.old` until every rename succeeds and
  restore them on error; wrap `install.sh` in `main() { ...; }; main "$@"`;
  warn when a base URL is not HTTPS.
- Wire change: no.

## SEC-23: `forget` and `move_task` act on other people's notes and cards

- Severity: low
- Area: MCP
- Evidence: `crates/polyph-daemon/src/coord.rs:909-930` (`forget`) deletes
  any memory note in the room by id prefix, regardless of author;
  `coord.rs:1115-1150` (`move_task`) moves any card without an assignee.
- Exploit: a prompt-injected agent deletes the room's memory notes or
  shuffles unassigned cards. Activity entries record it, but the notes are
  gone for everyone.
- Fix: let agents delete only notes they wrote (or their member's), and
  move only cards assigned to them; route the rest through an approval.
- Wire change: no.

## SEC-24: MCP tool calls are bound to the calling agent (info)

- Severity: info
- Area: MCP
- Evidence: each agent has its own proxy with its own internal handler and
  token (`crates/polyph-daemon/src/agents.rs:423-436`); calls check the
  token in constant time and run as that agent in that room
  (`crates/polyph-daemon/src/coord.rs:383-402`). There is no room
  parameter, so I found no path to another room. Inbox messages take the
  sender from the handle, not from arguments. Path arguments are checked
  to stay inside the worktree (`coord.rs:316-341`), although the daemon's
  own reads follow symlinks (SEC-16).
- Note: the token is in the environment of every process in the sandbox,
  including plugins' tool servers, so a plugin can call Polyph tools as
  the agent. `docs/security.md` already says plugins can do what an agent
  can.
- Telemetry: there is no telemetry or crash reporting code in `crates/`,
  so there is nothing to review against "private by construction" yet.

## SEC-25: `docs/security.md` claims protections the code does not have

- Severity: low (it misleads people choosing what to trust)
- Area: Docs
- Evidence and corrections needed:
  - "bubblewrap or Landlock + seccomp": there is no seccomp filter
    (`STUBS.md`; SEC-15).
  - "a restricted token and job object on Windows": it is low integrity
    with no read or network isolation (SEC-17).
  - "Paths in `.polyphignore` ... are never shared" and file mirrors are
    redacted: not implemented (SEC-19).
  - Redaction marker is `[REDACTED:<rule>]` in code
    (`crates/polyph-core/src/redact.rs:117`), not `[redacted: <rule>]`.
  - "messages are dropped even under old epochs": not for snapshots
    (SEC-1).
  - "about 5 in 2^20 per code": not true until SEC-4 is fixed.
  - "Refusing writes from non-members": removed members are still writers
    at the relay (SEC-8).
- Fix: correct the page now so it describes current behaviour, and update
  it again as each fix lands. I own `docs/security.md` and will make these
  edits in a follow-up branch unless the lead prefers otherwise.
- Wire change: no.

---

## Not covered in this pass

- The browser client and local web API (DNS rebinding, CSRF, session
  token), the phone pairing link, the desktop app, `polyph web`, plugin
  hooks, container mode, and the cloud account service.
- `crates/polyph-proto/src/wasm.rs` beyond confirming the phone uses the
  same `apply_relay_snapshot` (SEC-1 applies there too).
- Nothing was fuzzed or run. Envelope and join-code parsing have proptests
  already; I did not add any.

## Sign-off status

Not signed off. For a beta tag I need:

1. SEC-12, SEC-13 and SEC-14 fixed (no critical open), each with a test
   that shows the escape is blocked: a repository `polyph.toml` cannot turn
   the sandbox off or replace an agent command without a person's trust
   record; an agent cannot change git configuration the daemon executes;
   an agent cannot reach the D-Bus session bus or other host Unix sockets
   under bubblewrap.
2. SEC-1, SEC-2, SEC-3, SEC-4, SEC-15, SEC-16 and SEC-17 fixed or, where a
   fix needs a wire change Kao has not approved, the affected feature turned
   off for the beta: for example refusing Landlock mode until it enforces
   its network rule, and shipping Windows agents behind an explicit "no
   sandbox for Polyph itself" warning or not at all.
3. `docs/security.md` corrected to match the code (SEC-25), and every
   remaining medium finding listed in `STUBS.md` with its id.
4. A security contact address set in `docs/security.md` (human-only).
5. A decision on release signing (SEC-21): either signed releases, or a
   recorded decision by Kao to ship the beta with checksums only.

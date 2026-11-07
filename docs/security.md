# Security

This page describes what Polyph protects, from whom, and what it does not.
The wire details are in `docs/protocol.md`.

## Summary

- Code, chat, board, whiteboards, threads, memory notes, transcripts, diffs
  and terminal output are end-to-end encrypted between room members' devices.
- The relay (hosted, self-hosted or embedded for LAN) forwards ciphertext. It
  cannot read content, forge messages, or add members.
- Model credentials never leave members' own machines. Polyph never proxies
  model calls.
- No telemetry by default. Crash reports are opt-in.

## What the relay can see

The relay sees only this, and the hosted service's privacy page says the same:

| Data | Why |
|---|---|
| Room ids, device public keys, connection times and IP addresses | Routing and rate limiting |
| Envelope headers: room, sender device, epoch, sequence number, sizes | Routing, replay buffers |
| Which devices are in which rooms; member ids and roles from certificate chains (no names) | Refusing writes from non-members; plan member limits |
| The owner device of a registered room | Authorizing the room to be ended |
| Encrypted snapshot blobs and their sizes | Catch-up when nobody is online |
| Rendezvous id of a join code (a hash of its first half) and join attempt counts | Matching joiners to hosts; lockouts |
| **Opt-in** signed usage summaries: token counts, cost estimates, task counts | Team dashboard (accounts only, off by default) |
| **Web push** subscriptions; push payloads say only "open Polyph" | Phone notifications (accounts only) |
| Account email or GitHub id, plan and billing status | Accounts (optional; incognito use has none) |
| Device public keys linked to an account (each link signed by the device) | Applying a plan to the rooms those devices own |

## Threat model

### A malicious or compromised relay

Can:

- drop, delay, reorder or replay envelopes;
- learn who talks to whom, when, and how much (metadata);
- refuse service, or split members into partitions;
- see the rendezvous id of a join code and therefore its first four characters;
- keep ciphertext forever, regardless of stated retention.

Cannot:

- read any content: envelopes are XChaCha20-Poly1305 under epoch keys that
  only member devices hold, delivered with HPKE to each device's X25519 key;
- forge content: every envelope is signed by its sender's Ed25519 device key
  and clients drop senders without a verified certificate chain;
- replay content undetected: clients keep a per-sender sliding replay window;
- add a member: certificates chain back to the owner key that the room id
  itself is derived from, and joining needs the full code (PAKE) plus approval;
- complete a join itself: the relay knows 20 of the code's 40 bits; the PAKE
  needs all of them and confirms both sides in constant time.

Effects of what it can do: dropped or delayed messages are recovered by CRDT
sync when connectivity returns; a split causes claim conflicts that are
reconciled when it heals (earlier claim wins, loser notified, overlap shown on
the board); reordering within 1,024 messages is harmless.

### A removed member

Keeps everything they already received. Cannot read anything sent after the
rotation that follows their removal: the new epoch key is not delivered to
their devices. Their certificate is in every client's revocation set, so their
messages are dropped even under old epochs.

### A spectator

Holds a view key for one link, not the room key. Sees only what the owner's
filter republishes for that link. Anything a spectator sends is dropped by the
relay (no certificate) and by clients. Revoking the link stops new content.

### Someone guessing a join code

Online guessing only: each attempt is a full PAKE with the host. With the
first half known (worst case: the relay is the attacker), 20 bits remain;
five failures kill a one-time code or lock a reusable one for 10 minutes and
notify the owner, and approval to join is on by default. Per-IP limits slow
spraying across codes. Probability of a correct guess before lockout: about 5
in 2^20 (≈ 0.0005%) per code.

### A network attacker between a member and the relay

TLS to the relay (Caddy with a public certificate, or a self-signed
certificate in LAN mode whose fingerprint is bound into the join PAKE). Even
without TLS, content stays end-to-end encrypted; TLS protects metadata.

### Other local users and processes

- The daemon's local API listens on localhost only and requires a random token
  stored in the user's config directory with mode 0600 (owner-only ACL on
  Windows).
- The browser client never sees that token: `polyph web` opens a one-time
  link (two minutes, single use) that the page trades for a session token
  held in the daemon's memory. Browser-facing routes answer only to loopback
  host names (against DNS rebinding), the API refuses pages from other
  origins, and the client's pages carry a strict Content Security Policy.
- Device secret keys are stored in the OS keychain, or in a file encrypted
  with a key held in the keychain or, where none exists, derived from a local
  secret with owner-only permissions.
- Logs never contain message content, code, keys or tokens.

### Agents

Agents are untrusted code. Every agent and everything it spawns runs in a
sandbox (bubblewrap or Landlock + seccomp on Linux, Seatbelt on macOS, a
restricted token and job object on Windows, or a rootless container when
enabled) that limits writes to its worktree and temp directories. Outbound
network goes through the daemon's allowlisting proxy; anything else becomes an
approval request. `polyph sandbox test` reports which protections work on a
given machine. See `docs/sandbox.md` (M3).

The daemon treats an agent's worktree as hostile input:

- Under bubblewrap and Seatbelt, an agent working in the person's own
  checkout (no task) gets the whole `.git` read-only, mounted so it cannot
  be renamed, removed or replaced. It can edit files and run read-only git
  commands (`git status`, `git diff`, `git log`); it cannot commit, change
  the config, hooks or info, or add a `commondir` file. A task agent's
  branch is `polyph/<task id>/<name>`. It gets the shared `.git` read-only
  too, `objects/` included, except its task's folder in
  `refs/heads/polyph/` and in `logs/refs/heads/polyph/`, and its own
  `.git/worktrees/<name>/` folder (its HEAD, index, reflog, per-worktree
  refs and its own object store, `objects/`), so it can run `git add` and
  `git commit` on its task branch. Its objects go into its own store: the
  daemon sets `GIT_OBJECT_DIRECTORY` to it and
  `GIT_ALTERNATE_OBJECT_DIRECTORIES` to the shared store, which it reads
  but cannot change. Other entries in its task folders when it starts stay
  read-only; so do `main`, every other task's folder and every other ref
  and reflog. Inside its own folder the `commondir`, `gitdir` and
  `config.worktree` files and `objects/info/` stay read-only (the daemon
  creates an empty `config.worktree`, the task folders and the object
  store first), as do its worktree's `.git` file and the folders of other
  worktrees. An agent whose branch is not in its own task's folder (a
  branch with no `/`, or one a card names elsewhere) cannot commit at all;
  the daemon commits its work at finish.
- The daemon takes a task's commits into the shared object store at the
  end of every agent turn, on finish and on hand-off, never by trusting
  the task's files: `git pack-objects` packs them from the task's store
  and `git index-pack --strict` takes them into the shared one, naming
  every object by hashing its content and checking that every object it
  names is there; a blob the agent swapped fails the copy. Merges,
  checkouts and the fast-forward of the person's checkout read only the
  shared store. Until the turn that made them ends, commits an agent made
  itself exist only in its task's store, and the person's `git fetch`,
  `git gc` and `git log --all` in that repository fail with "bad object".
- An existing branch is given to an agent only if the daemon created it
  for a task (it records them in `task-branches.json` in the Polyph home):
  a card's branch is room data, so a card naming `main` or a branch
  someone else made is refused. The merge approval names the commit the
  queue merges and its diff stat ("polyph/1a2b3c4d/login at 0123456789ab
  (2 files changed, 5 insertions(+))"); the queue merges that commit, not
  whatever the branch points at by then.
- The daemon commits a task's work (on finish and hand-off) onto the task
  branch itself (`commit-tree` on the branch's tip, then `update-ref`),
  never through the worktree's HEAD, which the agent writes; if HEAD names
  another branch it refuses. Before merging into a branch, and again
  before moving it, it checks that the branch points where the last entry
  of its reflog says git set it; a branch written without git (as an
  agent with `refs/` writable would) is refused with an error naming the
  reflog. A branch without a reflog is not checked, and the check proves
  nothing for a branch inside a task folder, whose reflog its agent writes
  along with the ref.
- Before every git command that writes (commit, add, update-ref, merge,
  fetch, push, worktree, branch and the like) and before it adds a line
  to `info/exclude`, the daemon walks `logs/`, `refs/`, `worktrees/`,
  `objects/info/` and `info/` of the shared git directory, and the
  worktree's git directory, and refuses if any entry is a symbolic link
  or (Linux and macOS, outside `objects/`) a file with more than one hard
  link, with an error naming the path: git appends to reflogs through
  links, so a linked `logs/HEAD` would let an agent append a line of its
  choosing to any file the person can write. Before every git command it
  refuses an `objects/info/alternates` or `http-alternates` file; Polyph
  never creates one, so a repository cloned with `--reference` or
  `--shared` must be repacked (`git repack -a -d`) and the file removed
  before Polyph works in it. The same goes for a task's own store.
- The agent keeps running while the daemon commits its work, so it could
  swap a file for a link between that check and git's write. Every daemon
  git command that writes, except `fetch`, `pull` and `push` (which need
  the network and the person's credentials, and write only
  `refs/remotes/`, `FETCH_HEAD` and the shared `objects/`, none of which
  an agent writes), runs in a sandbox: bubblewrap on Linux, Seatbelt on
  macOS. Only the folders the command writes are writable. In a task
  worktree that is the worktree and its own `worktrees/<name>/` (not its
  `commondir`, `gitdir`, `config.worktree` or `objects/info/`), plus its
  branch's folders for an `update-ref`: what its agent writes anyway. In
  the person's checkout it is the checkout and `.git` except `config`,
  `hooks/` and `info/`. The person's home and everything else are
  read-only, so a link leads nowhere git can write. Where neither exists
  (Landlock, Windows, container mode, or bubblewrap that cannot create
  namespaces) the daemon's git runs unconfined; there task agents cannot
  write the shared git directory at all, and only an agent in the
  person's own checkout, which starts with a warning, can plant links.
  Bubblewrap is tested by `crates/polyph-sandbox/tests/sandbox.rs`;
  Seatbelt's rules by a unit test of the profile only, not yet run on a
  Mac. Landlock, Windows and container mode do not enforce any of this;
  see "Git settings an agent can write" below.
- Every git command the daemon runs names the git directory itself
  (`--git-dir`, `--work-tree`) and sets `GIT_COMMON_DIR`, after checking
  the checkout: a main git directory must not contain a `commondir` file,
  and a linked worktree's `.git` file must point into the main repository's
  `.git/worktrees/`, whose `commondir` must name that same repository. The
  daemon refuses to run git otherwise, with an error that names the file.
  Before a fetch or push with the repository's own settings, git, finding
  the repository by itself (`rev-parse --git-common-dir`), must arrive at
  `<main working copy>/.git`, or the command is refused.
- Every local git command the daemon runs (status, commit, diff, merge)
  overrides `core.fsmonitor`, `core.hooksPath`, `core.pager`,
  `protocol.ext.allow`, `commit.gpgSign`, `core.sshCommand` and
  `core.askPass` (with the person's own system or global value, `GIT_SSH`
  or `SSH_ASKPASS` carried over), and resets `credential.helper` to the
  person's system and global helpers. Diffs pass `--no-ext-diff
  --no-textconv`. `core.gitProxy` is disabled with an empty
  `GIT_PROXY_COMMAND`, because git keeps the first value it reads.
- Fetches and pushes use those overrides, plus `--upload-pack=git-upload-pack`
  on fetch, only for a checkout an agent ran in without a task under
  Landlock, Windows or container mode. Any other checkout is fetched and
  pushed with its own settings, so per-repository `core.sshCommand`,
  `includeIf` and pre-push hooks (Git LFS) keep working. Fetches never
  recurse into submodules.
- Git runs commands inside a nested repository or submodule with that
  repository's own config, which the agent can write (its clean filter runs
  during `git status`, for one). The daemon's `status`, `add` and `commit`
  never look inside submodules (`--ignore-submodules=all`,
  `diff.ignoreSubmodules=all`, and submodule entries left out of `git add`
  by path, so the daemon's commits never update a submodule's commit). In a
  task worktree it also refuses to read changes or commit when there is a
  `.git` file or folder below the worktree's root that is not a submodule
  in HEAD's `.gitmodules`, or a submodule entry in the index that HEAD does
  not have.
- Filter drivers (`filter.*`) and merge drivers in the repository's own
  config cannot be overridden by setting names; pointing
  `core.attributesFile` elsewhere does not stop them either, since
  `.gitattributes` in the worktree and `.git/info/attributes` still apply.
  They rely on `.git` being read-only, as above. A relative `include.path`
  in `.git/config` resolves inside `.git/`, which is read-only under
  bubblewrap and Seatbelt, so an agent cannot add an included file there.
- Files it reads from a worktree (the brief, areas, skills, mirrored files,
  merge resolutions) must not be symbolic links on any part of their path,
  nor (on Linux and macOS) hard links, and are capped in size (1 MiB unless
  the caller sets less). The hosted-mode mirror skips such a file; it
  reports a file as deleted only when it is gone.

#### Git settings an agent can write

Under Landlock, on Windows and in container mode, the mounts above are not
enforced. An agent working in the person's own checkout (no task, or
`worktree = false`) can write that checkout's `.git`: its config, hooks,
info and a `commondir` file. The daemon then refuses to run git in that
checkout while a `commondir` file is there, never looks inside submodules,
and fetches and pushes it with the overrides above, but a filter or merge
driver in that config still runs when the daemon checks status, commits or
fast-forwards there, as does any other setting that names a program and is
not listed above. The person's own git commands in that checkout run all of
it. The agent starts with a warning saying so. Under those mechanisms, use
tasks (each in its own worktree, outside the shared `.git`, which the agent
cannot write there) or bubblewrap and Seatbelt. Files in the working copy
itself remain writable everywhere, including a hooks folder that a
repository's `core.hooksPath` points into the working copy.

### A lost phone

The phone app keeps its device key and room state in the browser's IndexedDB,
sealed with an AES-GCM key the browser generated as non-extractable. Copies
of the stored bytes (a backup, a synced profile) cannot be opened elsewhere;
someone holding the unlocked phone can use the app. Remove the phone's
membership from another device (`polyph room remove`), which rotates the
room key. The pairing link carries a one-time code, the person's member id
and the relay in the URL fragment, which browsers never send to the server
hosting the page; the page clears it from the address bar at once.

### An agent host

An agent host holds its member's agents and a checkout of the code, like
that member's laptop. It accepts work only from devices of its own member
(checked against their certificates), and its certificate capability is
visible to the room. Treat it as one of your own devices: keep its OS
patched and its disk encrypted, and remove its membership if it is lost.

### Plugins

Plugins are code a person chose to install on their own device; a
repository cannot install one. Their tool servers run inside each agent's
sandbox and their hooks in a fresh sandbox of their own, reaching only the
network hosts the manifest declares. Within those limits a plugin can do
what an agent can (read the worktree, call Polyph tools through the agent),
so install only plugins you trust. They are not signed yet (`STUBS.md`).

### Secrets in shared content

Before diffs, file mirrors, tool output, terminal streams or chat leave a
member's daemon, a redactor (gitleaks default rules plus `.env`-style
detection) replaces matches with `[redacted: <rule>]`. Paths in
`.polyphignore` (default: `.env*`, `*.pem`, `*.key`, `id_*`, `.aws/`,
`.ssh/`) are never shared. Redaction is a safety net; it cannot recognize
every secret.

## Not protected

- Metadata (who, when, how much) from the relay operator.
- Members of a room: they can read everything in it, by design. Choose members
  and roles accordingly.
- A compromised member device: it holds that member's keys.
- Denial of service by the relay or the network.

## Reporting

Report vulnerabilities privately to the maintainers (address to be set before
launch; listed in `STUBS.md`). Do not open public issues for security bugs.

## Cryptography used

| Purpose | Primitive | Crate |
|---|---|---|
| Device signatures | Ed25519 (strict verification) | `ed25519-dalek` 2 |
| Key agreement for key delivery | HPKE RFC 9180, DHKEM(X25519, HKDF-SHA256), ChaCha20-Poly1305 | `hpke` 0.13 |
| Message encryption | XChaCha20-Poly1305 | `chacha20poly1305` 0.10 |
| Key derivation | HKDF-SHA256, BLAKE3 derive_key | `hkdf`, `blake3` |
| Join | SPAKE2 (Ed25519 group), HMAC-SHA256 key confirmation | `spake2` 0.4 |
| Transport | TLS 1.2/1.3 | `rustls` |

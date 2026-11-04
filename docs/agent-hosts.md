# Agent hosts: agents on your own server

An agent host is a computer you own (a server, a spare desktop, a cloud VM)
that runs your agents, so their work continues when your laptop is closed.
It joins your room as another device of yours: same person, its own device
key, and a certificate with the `agent-host` capability. Polyph's servers
never run your code; the relay forwards only ciphertext, as for any device.

## Set one up

On your computer, in the room:

```bash
polyph agent-host invite
```

It prints a one-time command. On the server, with Polyph installed and a
checkout of the repository for agents to work in, run that command with the
checkout as `--workdir`, and keep the daemon running (a service, or
`polyph daemon run` in a terminal multiplexer):

```bash
polyph agent-host join A1B2-CDEF --member <your member id> --relay 'wss://…/ws' --name 'Ann' --workdir /srv/payments
```

Approve the server's request to join on your computer (or phone). The knock
says it asks to be an agent host.

## Use it

```bash
polyph agent hosts                                   # your hosts, online or not
polyph agent start claude --on cloud --prompt "…"    # any online host of yours
polyph agent start claude --on 3f9a1c2b              # a specific host
polyph agent prompt <agent> "…"                      # forwarded to its host
polyph agent stop <agent>
```

In the apps, Agents > Start offers "Run on" with your hosts. The phone app
can start, prompt and stop agents on your hosts, which is how you start work
with no laptop at all. Approvals from those agents come to you on every
device, as for any of your agents. Status feeds mark them as cloud agents.

## Rules

- A host acts only on requests from devices of its own person. Teammates see
  its agents' work like any agent's, but cannot start or steer them there.
- Agents on a host run in the host's sandbox, under its `polyph.toml`, in its
  working directory; task work gets its own worktree and branch there, and
  reaches teammates through the merge queue and `[merge] push`.
- One daemon is an agent host for one person. Use a separate `POLYPH_HOME`
  (and user account) per person on a shared server.
- Change the working directory later with `polyph agent-host workdir <dir>`.

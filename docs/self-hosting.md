# Self-hosting the relay

The relay is one binary (`polyph-relay`, or `polyph relay serve`). It stores
encrypted snapshots in a SQLite file and needs no other services. Self-hosted
relays have no plan limits.

## Memory

Target: under 150 MB of RAM with 50 active rooms of 5 people each.

Measured with `polyph-loadtest` (in `crates/polyph-testkit`), which runs the
release relay as a separate process and reads its peak resident set size
(`VmHWM`). 250 clients each subscribe as a certified member and publish
encrypted envelopes; every envelope fans out to the other 4 members.

| Load | Per member | Envelopes sent | Deliveries | Relay peak RSS | Relay RSS after |
|---|---|---|---|---|---|
| Normal: 50 rooms × 5, 60 s | 2 msg/s, 600 B | 30,210 | 120,143 | **71 MB** | 40 MB |
| Worst case: 50 rooms × 5, 90 s | 10 msg/s, 1,000 B | 225,215 | 900,096 | **118 MB** | 86 MB |

Idle relay: 8 MB. Measured on Linux 6.18, 4 vCPU Xeon 2.1 GHz, release build,
0 errors in both runs. In the worst case every room's replay buffer is full and
the 64 MiB global buffer budget is in effect.

Reproduce:

```bash
cargo build --release -p polyph-relay -p polyph-testkit
target/release/polyph-loadtest --relay-bin target/release/polyph-relay --seconds 60 --msgs-per-sec 2
target/release/polyph-loadtest --relay-bin target/release/polyph-relay --seconds 90 --msgs-per-sec 10 --payload 1000
```

## Configuration

Environment variables override the TOML file given with `--config`.

| Variable | TOML key | Default |
|---|---|---|
| `POLYPH_RELAY_BIND` | `bind` | `127.0.0.1:8787` |
| `POLYPH_RELAY_DATABASE` | `database` | `polyph-relay.db` |
| `POLYPH_RELAY_METRICS` | `metrics` | `false` (Prometheus at `/metrics` when on) |
| `POLYPH_RELAY_MAX_MEMBERS` | `limits.max_members` | unlimited |
| `POLYPH_RELAY_RETENTION_DAYS` | `limits.retention_days` | unlimited |
| `POLYPH_RELAY_PUBLIC_URL` | `public_url` | none |
| `POLYPH_RELAY_CONNECTIONS_PER_IP_PER_MIN` | `connections_per_ip_per_min` | `120` |
| `POLYPH_RELAY_TLS_CERT`, `POLYPH_RELAY_TLS_KEY` | `tls_cert`, `tls_key` | none (use Caddy) |

Other TOML keys: `buffer_secs` (600), `buffer_max_envelopes` (5000),
`buffer_max_bytes` (2 MiB per room), `buffer_total_max_bytes` (64 MiB),
`snapshots_per_room` (10), `join_attempts_per_ip` (20 per 10 min),
`frames_per_sec` (200), `code_max_failures` (5), `code_lock_secs` (600).

Endpoints: `/ws` (clients), `/healthz` (JSON status), `/metrics` (opt-in).
Logs are JSON on stderr and never include content.

Point clients at your relay:

```bash
polyph config set relay wss://relay.example.com/ws
```

## Agent hosts

To run agents on your own server, so work continues with your laptop
closed, see `docs/agent-hosts.md`. An agent host needs no relay of its own;
it connects to the room's relay like any device.

## EC2 walkthrough and LAN mode

Written in milestone 11 together with `deploy/relay/`. See `STUBS.md`.

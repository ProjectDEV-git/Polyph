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
| `POLYPH_RELAY_CLIENT_IP_HEADER` | `client_ip_header` | none (trust `X-Forwarded-For` from the same machine only) |

Other TOML keys: `buffer_secs` (600), `buffer_max_envelopes` (5000),
`buffer_max_bytes` (2 MiB per room), `buffer_total_max_bytes` (64 MiB),
`snapshots_per_room` (10), `join_attempts_per_ip` (20 per 10 min),
`frames_per_sec` (200), `code_max_failures` (5), `code_lock_secs` (600),
`join_handshake_secs` (120).

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

## Running it

The project's own relay (the default in every client) runs on Fly.io; its
config and steps are in `deploy/fly/`. Behind a proxy that every connection
passes through, set `POLYPH_RELAY_CLIENT_IP_HEADER` to the header that names
the client (`fly-client-ip` on Fly.io) so per-address limits see real
addresses; unset, only `X-Forwarded-For` from the same machine is trusted.

To run your own, there are three ways, all from `deploy/relay/`. Whichever you choose, the relay listens
on plain HTTP and something in front of it terminates TLS; clients connect
with `wss://`. Caddy is used here because it gets and renews certificates by
itself.

### Docker Compose with Caddy

```bash
cd deploy/relay
POLYPH_DOMAIN=relay.example.com docker compose up -d
```

`compose.yaml` builds the image from source (`deploy/relay/Dockerfile`), or
use the published image: replace `build:` with
`image: ghcr.io/projectdev-git/polyph-relay:<version>`. Snapshots live in the
`relay-data` volume, certificates in `caddy-data`. Set limits with the
commented environment variables if you want them; a self-hosted relay has
none by default.

### systemd with a release archive

```bash
tar -xzf polyph-<version>-x86_64-unknown-linux-gnu.tar.gz
sudo install -m 755 polyph-<version>-*/bin/polyph-relay /usr/local/bin/
sudo cp deploy/relay/polyph-relay.service /etc/systemd/system/
sudo systemctl enable --now polyph-relay
curl -s http://127.0.0.1:8787/healthz
```

The unit runs the relay as a throwaway user with a private state directory
(`/var/lib/polyph-relay`) and a locked-down sandbox; it listens on
`127.0.0.1:8787`. Put Caddy in front with `deploy/relay/Caddyfile` and
`POLYPH_RELAY_UPSTREAM=127.0.0.1:8787`, or any TLS proxy that passes
WebSocket upgrades on `/ws`.

### EC2 walkthrough

The load test above peaked at 71 MB for 50 rooms of 5 people, so the
smallest general-purpose instances are enough for a team or several.

1. Launch an instance with Ubuntu Server 24.04 LTS (arm64 or x86_64). A
   `t4g.micro` or `t3.micro` is plenty to start.
2. Security group: allow inbound TCP 22 from your own address, and TCP 80
   and 443 (and UDP 443 for HTTP/3) from anywhere. Nothing else; the relay
   port stays private.
3. Give it an Elastic IP and point a DNS `A` record (for example
   `relay.example.com`) at it. Caddy needs the name to resolve before it can
   get a certificate.
4. Install Docker and start the relay:

   ```bash
   sudo apt-get update && sudo apt-get install -y docker.io docker-compose-v2 git
   git clone https://github.com/ProjectDEV-git/Polyph.git && cd Polyph/deploy/relay
   sudo POLYPH_DOMAIN=relay.example.com docker compose up -d
   curl -s https://relay.example.com/healthz
   ```

5. On every member's machine: `polyph config set relay
   wss://relay.example.com/ws`. Existing rooms keep the relay they were
   created with; new rooms use this one.

Backups: the relay holds only encrypted snapshots, which members' devices
can re-upload, so losing them costs catch-up for people who were offline,
not data. To keep them anyway, back up the `relay-data` volume (one SQLite
file plus its `-wal`). Upgrades: `git pull && sudo docker compose up -d
--build`; clients reconnect on their own.

## LAN mode (no internet)

For a room on one network with no internet at all, a member's own daemon
runs the relay:

```bash
polyph host --lan --name payments
```

The daemon starts an embedded relay on all interfaces with a self-signed
certificate and prints the join line for this network, for example
`polyph join A1B2-CDEF --relay wss://192.168.1.20:41871/ws`. The
certificate's fingerprint travels inside the join handshake, so joiners pin
it without trusting anything else. Others on the network can also find the
room with `polyph discover` (mDNS, `_polyph._tcp`).

- The port is chosen fresh each time the room is hosted; allow incoming
  connections to `polyph` in the host's firewall rather than one port.
- The room lives as long as the hosting daemon runs. `polyph host --lan
  --bridge` also copies traffic to the internet relay, so members who joined
  earlier can keep working from elsewhere (see `STUBS.md` for its limits).
- Phones cannot join LAN rooms: a browser cannot pin a self-signed
  certificate.

## The hosted service and the website

`deploy/cloud/` runs Polyph's own hosted service (proprietary, `cloud/`):
the relay with plan limits and the account API, behind Caddy, configured by
`cloud.toml` and secrets in `.env`. See `cloud/README.md`.

The website is the static export of `apps/web` (`pnpm --filter @polyph/web
build`, output `apps/web/out`). `apps/web/vercel.json` holds its headers for
Vercel; `deploy/web/Caddyfile` serves it with the same headers anywhere else.

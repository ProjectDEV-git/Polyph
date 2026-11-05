# The relay on Fly.io

`polyph`, the desktop app and the phone app connect to
`wss://polyph-relay.fly.dev/ws` unless told otherwise. This folder runs that
relay on [Fly.io](https://fly.io): one small machine in Singapore (`sin`)
with a 1 GB volume for encrypted room snapshots, so rooms survive restarts.

Fly bills by usage. This setup (one `shared-cpu-1x` machine with 256 MB that
never sleeps, plus a 1 GB volume) costs a few US dollars a month; check
[fly.io/pricing](https://fly.io/docs/about/pricing/) for current rates.

## First deploy

Install `flyctl` ([fly.io/docs/flyctl/install](https://fly.io/docs/flyctl/install/)),
then from the repository root:

```bash
fly auth login
fly apps create polyph-relay
fly volumes create relay_data --app polyph-relay --region sin --size 1 --yes
fly deploy --config deploy/fly/fly.toml --dockerfile deploy/fly/Dockerfile
```

The first deploy builds the relay from source on Fly's builder, which takes
a few minutes. Then check it:

```bash
curl https://polyph-relay.fly.dev/healthz
```

If the name `polyph-relay` is taken, pick another, use it in the commands
above and in `fly.toml`, and change the default relay in
`crates/polyph-daemon/src/config.rs` and `apps/web/src/phone/client.tsx`.

## Updates

Run the same `fly deploy` command from the repository root. Rooms reconnect
on their own after the restart.

## Logs and limits

```bash
fly logs --app polyph-relay
fly status --app polyph-relay
```

The relay sets no plan limits by default. To cap room size or how long
snapshots are kept, add `POLYPH_RELAY_MAX_MEMBERS` or
`POLYPH_RELAY_RETENTION_DAYS` under `[env]` in `fly.toml` and deploy again
(all settings: `docs/self-hosting.md`).

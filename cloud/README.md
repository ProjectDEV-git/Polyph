# polyph-cloud

Polyph's hosted service. Proprietary: see `LICENSE` in this directory; the
rest of the repository is Apache-2.0 and never depends on this crate.

One process runs:

- **the hosted relay** (`polyph-relay` embedded) with a plan policy: each
  room gets the limits of the plan its owner device's account has. Devices
  with no account get Free. The relay still sees only ciphertext.
- **the account API** the website and daemons call: sign-in by emailed code
  or GitHub (device flow), devices linked to an account by a signature from
  the device key (context `account-link`), teams and invites, plans, and
  Stripe checkout, customer portal and signed webhooks.

## Build and test

Built on its own (it is excluded from the open workspace), sharing the
workspace's target directory:

```bash
cargo test --manifest-path cloud/Cargo.toml --target-dir target
cargo clippy --manifest-path cloud/Cargo.toml --target-dir target --all-targets -- -D warnings
```

`tests/cloud.rs` runs the API against local stand-ins for Stripe and GitHub;
`tests/daemon_login.rs` has a real daemon sign in and link its device.

## Run

```bash
cargo run --manifest-path cloud/Cargo.toml --target-dir target -- serve --config cloud.toml
```

```toml
api_bind = "127.0.0.1:8790"
database = "/var/lib/polyph/cloud.db"
site_url = "https://polyph.dev"
allowed_origins = ["https://polyph.dev"]

[relay]                      # omit to run the account API alone
bind = "127.0.0.1:8787"
database = "/var/lib/polyph/relay.db"

[mail]                       # a Resend-compatible endpoint
endpoint = "https://api.resend.com/emails"
from = "Polyph <signin@polyph.dev>"
# api_key from POLYPH_CLOUD_MAIL_API_KEY

[github]                     # an OAuth app with the device flow enabled
client_id = "Iv1.xxxxxxxx"

[plans.free]
max_members = 5              # example; member limits are not decided
[plans.pro]
stripe_price = "price_..."
# price = "…"                # display text; leave unset until decided
[plans.team]
stripe_price = "price_..."
```

Secrets come from the environment: `POLYPH_CLOUD_MAIL_API_KEY`,
`POLYPH_CLOUD_STRIPE_SECRET_KEY` and `POLYPH_CLOUD_STRIPE_WEBHOOK_SECRET`.
`dev = true` writes sign-in codes to the log instead of mailing them, for
local testing only.

The Stripe webhook endpoint is `POST /v1/billing/webhook`; subscribe it to
`checkout.session.completed` and `customer.subscription.created`, `.updated`
and `.deleted`.

## What is stored

Accounts (email or GitHub id and login, language), session tokens, sign-in
codes and invite tokens as SHA-256 hashes only, linked device public keys,
teams and memberships, Stripe customer ids and subscription states. Nothing
about rooms beyond what the relay already stores.

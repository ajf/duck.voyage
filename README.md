# duck-tracker

A QR-scannable registry for rubber ducks planted on cruise ships. Each duck
carries a printed code; scanning it shows the duck's story and lets finders
log sightings. The full design rationale lives in `duck-voyage.md`.

This is self-hostable software, not a hosted service: it runs anywhere a
container (or bare binary), a PostgreSQL database, and an OIDC login
provider are available. Nothing in it is specific to any cloud platform.

## Local development

Requires Rust (pinned via `rust-toolchain.toml`) plus Docker or podman.

```sh
docker compose up -d       # Postgres :5432, MinIO :9000, Keycloak :8081
                           # (podman without compose: ./scripts/dev-up.sh)
cp .env.example .env       # defaults match the dev stack
./scripts/seed-vessels.sh  # a handful of cruise ships to pick from
cargo run -p web           # http://localhost:3000
```

Log in with the dev Keycloak: users `andrew` / `stranger`, password `duck`
(imported from `dev/keycloak-realm.json`). Tests: `cargo test --workspace`.

The `domain` crate is pure (no I/O) and carries the load-bearing tests: FF1
codec round-trips, the Damm-36 check-character table (frozen forever — see
the fingerprint test), and golden code vectors pinning the exact printed-code
mapping across upgrades.

## Configuration

Everything is configured through environment variables (a `.env` file is
read in development). The app fails fast at boot on missing required values.

| Variable | Required | Default | Purpose |
|----------|----------|---------|---------|
| `DATABASE_URL` | yes | — | PostgreSQL connection string. Migrations run automatically at startup. |
| `BASE_URL` | yes | — | Public URL of the instance. QR labels and OIDC redirect URIs derive from it; `https://` also turns on Secure cookies. |
| `DUCK_KEY_GEN_0` (`_1`, …) | yes | — | FF1 code keys, 32 bytes hex each (`openssl rand -hex 32`). **Append-only forever**: printed codes are bound to their key generation. |
| `DUCK_KEY_CURRENT` | yes | — | Generation new flocks mint under. |
| `LISTEN_ADDR` | no | `[::]:3000` | Listen address. The default is dual-stack (IPv6 + IPv4-mapped). Use e.g. `0.0.0.0:3000` on IPv6-less hosts. |
| `PORT` | no | `3000` | Port shorthand when `LISTEN_ADDR` is unset. |
| `STORAGE_ENDPOINT` / `_BUCKET` / `_ACCESS_KEY` / `_SECRET_KEY` | no | — | S3-compatible photo storage (MinIO, Tigris, AWS, …). |
| `STORAGE_LOCAL_PATH` | no | `./photos` | Directory-on-disk photo storage, used when `STORAGE_ENDPOINT` is unset. |
| `OIDC_GOOGLE_CLIENT_ID` / `_SECRET` | no | — | "Sign in with Google". |
| `OIDC_ENTRA_CLIENT_ID` / `_SECRET` / `_TENANT` | no | — | Microsoft Entra ID. |
| `OIDC_APPLE_CLIENT_ID` / `_TEAM_ID` / `_KEY_ID` / `_PRIVATE_KEY` | no | — | Sign in with Apple (the client secret is minted at runtime from the key). |
| `OIDC_<SLUG>_ISSUER` / `_CLIENT_ID` / `_SECRET` / `_DISPLAY_NAME` | no | — | **Any other OIDC provider** — Keycloak, Authentik, Authelia, Zitadel, Okta, …. Pick a slug; it becomes `/login/<slug>`. Display name defaults to the capitalized slug. |
| `TRUST_PROXY_HEADERS` | no | `false` | Key rate limits on `X-Forwarded-For`-style headers. Set `true` **only** behind a trusted reverse proxy / load balancer; otherwise clients could spoof their IP. |
| `ADMIN_IDENTITIES` | no | empty | Comma-separated `issuer\|subject` pairs granted admin on login. |
| `CAP_FLOCKS_PER_USER`, `CAP_MINT_BATCH_MAX`, `CAP_UNORIGINATED_MAX`, `MISSING_AFTER_DAYS`, `FRONT_PAGE_LIMIT` | no | 10, 100, 200, 365, 20 | Product knobs. |

At least one OIDC provider must be configured or nobody can log in. Any
number can be active at once; the login page lists whatever is configured.

## Deployment (generic)

Prebuilt container images are published to **`ghcr.io/ajf/duck.voyage`** by
CI: `latest` and `main` track the main branch, `vX.Y.Z` tags track releases.
Building yourself works too — the multi-stage `Dockerfile` produces a
self-contained image (compile-time SQL checks use the committed `.sqlx`
metadata, so no database is needed at build time). To run it you need:

1. **PostgreSQL** — reachable via `DATABASE_URL`. The app applies its own
   migrations at startup.
2. **Photo storage** — an S3-compatible bucket, or nothing: the local-disk
   default is fine for a single-node install (mount a volume at
   `STORAGE_LOCAL_PATH` so photos survive redeploys).
3. **TLS termination** — run the container behind your reverse proxy /
   load balancer of choice, set `BASE_URL` to the public `https://` URL, and
   set `TRUST_PROXY_HEADERS=true` so rate limits see real client IPs.
4. **Secrets** — inject the environment variables however your platform
   does secrets. Generate a fresh `DUCK_KEY_GEN_0` for production and treat
   it like a signing key: printed labels die with it.

`GET /healthz` is a cheap liveness endpoint for health checks. The listener
is dual-stack by default, so IPv6-native platforms and plain IPv4 hosts both
work without configuration.

### Example: Fly.io

Keep your `fly.toml` in your own (private) deploy repo — deployment config
is yours, not the software's — and deploy the published image:

```sh
fly deploy -c fly.toml --image ghcr.io/ajf/duck.voyage:v0.1.0
```

A minimal `fly.toml`: `internal_port = 3000`, `force_https = true`, a
`/healthz` HTTP check, and `TRUST_PROXY_HEADERS=true` plus
`LISTEN_ADDR="[::]:3000"` in `[env]` (Fly's private network is IPv6).
Secrets go through `fly secrets set`.

## After changing SQL

The `query!` macros type-check against a live database. `.sqlx/` is
committed so builds work without one (`SQLX_OFFLINE=true`). After changing
any query or migration:

```sh
cargo sqlx prepare --workspace
```

## License

[AGPL-3.0-or-later](LICENSE).

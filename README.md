# duck-tracker

A QR-scannable registry for rubber ducks planted on cruise ships. The full
design lives in [`../duck-voyage.md`](../duck-voyage.md) — read that first;
this file is just the local quickstart.

## Local development

Requires Rust (pinned via `rust-toolchain.toml`) and podman.

```sh
./scripts/dev-up.sh        # Postgres :5432, MinIO :9000, Keycloak :8081
cp .env.example .env       # defaults match the dev stack
./scripts/seed-vessels.sh  # a handful of cruise ships to pick from
cargo run -p web           # http://localhost:3000
```

Log in with Keycloak: users `andrew` / `stranger`, password `duck`
(imported from `dev/keycloak-realm.json`).

Tear down with `./scripts/dev-down.sh` (add `--wipe` to drop the volumes).

## Tests

```sh
cargo test --workspace
```

The `domain` crate is pure (no I/O) and carries the load-bearing tests: the
FF1 codec round-trips, the Damm-36 check-character table (frozen forever —
see the fingerprint test), and the golden code vectors that pin the exact
printed-code mapping across upgrades.

## sqlx offline metadata

The `query!` macros type-check against a live database. `.sqlx/` is committed
so CI/Docker builds work without one (`SQLX_OFFLINE=true`). After changing
any query or migration:

```sh
cargo sqlx prepare --workspace
```

## Deployment

Fly.io per duck-voyage.md §11 — **not wired up yet by intent**; everything so
far is local-only.

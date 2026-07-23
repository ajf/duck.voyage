# Multi-stage build with cargo-chef for cached dependency layers.
# Build-time DB access is avoided via the committed .sqlx offline metadata
# (SQLX_OFFLINE=true).

FROM rust:1.97-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
ENV SQLX_OFFLINE=true
RUN cargo build --release -p web

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/web /usr/local/bin/duck-web
EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/duck-web"]

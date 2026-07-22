# syntax=docker/dockerfile:1
# Build context: backend/

FROM rust:1.97-slim-bookworm AS chef
ARG UID=1000
ARG GID=1000
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential pkg-config curl \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
RUN (groupadd -g ${GID} appuser || true) \
    && (useradd -u ${UID} -g ${GID} -m -s /bin/sh appuser || true) \
    && mkdir -p /app/target \
    && chown -R ${UID}:${GID} /app /usr/local/cargo
WORKDIR /app
USER ${UID}:${GID}

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release -p api --recipe-path recipe.json --locked
COPY . .
RUN cargo build --release -p api --locked

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --no-create-home --uid 10001 pwnforge

COPY --from=builder /app/target/release/api /usr/local/bin/api

USER pwnforge
EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/api"]

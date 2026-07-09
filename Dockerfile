# syntax=docker/dockerfile:1.7
#
# VAST — multi-stage Dockerfile
#   Stage 1: build the React/TS frontend with Bun/Vite  -> frontend/dist
#   Stage 2: compile the Rust backend (frontend embedded at compile time)
#   Stage 3: minimal runtime image (single binary + dynamic libs)
#
# Build:  docker build -t vast/im-server .
# Run:    docker compose up -d   (see docker-compose.yml)

# ============================================================
# Stage 1 — Frontend (Bun + Vite)
# ============================================================
FROM oven/bun:1.2 AS frontend
WORKDIR /build

# Install deps first (layer cache). --frozen-lockfile requires frontend/bun.lock.
COPY frontend/bun.lock ./bun.lock
COPY frontend/package.json ./package.json
RUN bun install --frozen-lockfile

# Build the SPA. Output lands in /build/dist.
COPY frontend/ ./
RUN bun run build

# ============================================================
# Stage 2 — Backend (Rust / Axum)
# ============================================================
FROM rust:1.93-bookworm AS backend
WORKDIR /app

# Build toolchain + native libs for: aws_lc_rs (cmake), openssl (libssl-dev),
# libsqlite3-sys (libsqlite3-dev). These are build-time only.
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential pkg-config cmake ca-certificates \
        libssl-dev libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

# rust-embed reads `frontend/dist/` at COMPILE time (relative to CARGO_MANIFEST_DIR=/app),
# so the frontend output must be in place before `cargo build`.
COPY --from=frontend /build/dist ./frontend/dist

# Manifests first for better layer caching of dependency fetches.
COPY Cargo.toml Cargo.lock ./

# Source + build. BuildKit cache mounts keep cargo registry + target artifacts
# across builds, making incremental rebuilds fast. Requires DOCKER_BUILDKIT=1
# (default on modern Docker / `docker buildx`).
COPY src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    cargo build --release --locked && \
    cp target/release/im-server /im-server

# ============================================================
# Stage 3 — Runtime (minimal)
# ============================================================
FROM debian:bookworm-slim AS runtime

# Runtime dynamic deps:
#   libssl3      — linked by the `openssl` crate (web-push JWT/JWS)
#   libsqlite3-0 — linked by libsqlite3-sys when not bundled (harmless if bundled)
#   ca-certificates — TLS verification for outbound HTTPS (web push, AI bots)
#   curl         — used by HEALTHCHECK
#   tzdata       — correct local timestamps in logs
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl tzdata \
        libssl3 libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/* \
    && ln -sf /usr/share/zoneinfo/UTC /etc/localtime

# Non-root runtime user.
RUN groupadd --system imserver \
 && useradd  --system --gid imserver --home-dir /app --shell /usr/sbin/nologin imserver

WORKDIR /app
COPY --from=backend /im-server /app/im-server

# data_dir is resolved as <binary_dir>/data, i.e. /app/data (SQLite DB + uploads).
# certs/ holds TLS material when TLS_MODE != none (relative to WORKDIR).
RUN mkdir -p /app/data /app/certs \
 && chown -R imserver:imserver /app

USER imserver

EXPOSE 3000/tcp 3443/tcp

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:3000/api/health || exit 1

ENTRYPOINT ["/app/im-server"]

# VAST — Real-time IM Server

A full-featured instant messaging server with a modern React frontend. Built with **Axum** (Rust) on the backend and **React + Vite** (Bun) on the frontend.

## Features

- **Real-time messaging** via WebSocket with presence indicators and typing notifications
- **Channel-based conversations** — create public/private channels, archive/unarchive
- **Thread support** — reply to messages in nested threads
- **Direct Messages** — one-on-one conversations
- **Reactions** — emoji reactions on any message
- **File uploads** with attachment support
- **Search** — full-text message search across channels
- **Join requests & invitations** — access control for private channels
- **JWT authentication** — register, login, token-based auth
- **TLS support** — self-signed and Let's Encrypt modes

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs) 1.93+ (`rustc`, `cargo`)
- [Bun](https://bun.sh) 1.2+ (`bun`)

### Setup

```bash
# Clone the repository
git clone <repo-url> && cd vast

# Copy environment configuration
cp .env.example .env
# Edit .env — especially JWT_SECRET (generate: openssl rand -base64 48)

# Start in development mode (backend + frontend concurrently)
make dev
```

The backend starts on **http://localhost:3000** and the frontend dev server on **http://localhost:5173** (proxying API and WebSocket to the backend).

### Production Build

```bash
# Full release build
./scripts/build.sh

# Binary is at: target/release/im-server
# Frontend is embedded into the binary via rust-embed
```

### Run the server

```bash
# Copy .env next to the binary and run:
./target/release/im-server

# Or via TLS:
TLS_MODE=self-signed ./target/release/im-server
```

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Client                               │
│  ┌─────────┐  ┌────────────────┐  ┌──────────────────────┐ │
│  │  React  │  │  WebSocket     │  │  REST API calls      │ │
│  │  (SPA)  │  │  (real-time)   │  │  (auth, files, ...)  │ │
│  └────┬────┘  └───────┬────────┘  └──────────┬───────────┘ │
└───────┼───────────────┼──────────────────────┼─────────────┘
        │  Vite dev     │  ws://localhost:3000 │  http://localhost:3000
        │  proxy ───────┘                      │
        ▼                                       ▼
┌─────────────────────────────────────────────────────────────┐
│                      IM Server (Axum)                        │
│  ┌─────────┐  ┌──────────┐  ┌────────┐  ┌───────────────┐  │
│  │  Auth   │  │ Messages │  │Search  │  │  File Upload  │  │
│  │  (JWT)  │  │ + Thread │  │        │  │               │  │
│  ├─────────┤  ├──────────┤  ├────────┤  ├───────────────┤  │
│  │Channels │  │  DM      │  │Reaction│  │  Invitations  │  │
│  │         │  │          │  │        │  │  + Requests   │  │
│  ├─────────┴──┴──────────┴──┴────────┴──┴───────────────┤  │
│  │              WebSocket Manager (presence, typing)      │  │
│  ├───────────────────────────────────────────────────────┤  │
│  │              SQLite (via sqlx + migrations)            │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Stack

| Layer        | Technology                                |
|-------------|-------------------------------------------|
| **Backend** | Rust, Axum 0.8, Tokio, sqlx, jsonwebtoken |
| **Database** | SQLite (single file, zero-config)        |
| **Frontend** | React 19, Vite 7, Tailwind CSS 4         |
| **Real-time** | WebSocket (axum built-in)               |
| **Auth**     | JWT (access tokens), Argon2 password hash |

### Project Layout

```
vast/
├── src/                  # Rust backend
│   ├── api/              #  REST route handlers
│   │   ├── auth.rs       #  Register / Login / Password reset
│   │   ├── channels.rs   #  Channel CRUD + archive
│   │   ├── channel_members.rs #  Member management
│   │   ├── messages.rs   #  Message CRUD + threads
│   │   ├── dm.rs         #  Direct messages
│   │   ├── files.rs      #  File upload/download (auth-guarded)
│   │   ├── reactions.rs  #  Emoji reactions
│   │   ├── search.rs     #  Full-text search
│   │   ├── requests.rs   #  Join requests
│   │   ├── invitations.rs#  Invitations
│   │   └── presence.rs   #  Presence status
│   ├── auth/             #  Auth module
│   │   ├── mod.rs        #  JWT + password utilities
│   │   └── middleware.rs #  JWT auth middleware
│   ├── ws/               #  WebSocket handler
│   │   ├── mod.rs        #  Connection manager + hub
│   │   └── protocol.rs   #  WS message protocol
│   ├── db/               #  Database module
│   │   └── mod.rs        #  Pool + migrations + queries
│   ├── embed.rs          #  Frontend static file embedding
│   ├── error.rs          #  Unified error types
│   ├── lib.rs            #  App state + router setup
│   └── main.rs           #  Server entrypoint
├── frontend/             # React SPA
│   ├── src/
│   │   ├── api/          #  API client (channels, dm, reactions, ...)
│   │   ├── components/   #  UI components (MessageList, ChannelSidebar, ...)
│   │   ├── pages/        #  Route pages (Login, Register, Search, DM, ...)
│   │   ├── hooks/        #  Custom hooks (useWebSocket, useCursorSync, ...)
│   │   ├── stores/       #  Zustand stores (auth, channel, message, ...)
│   │   ├── types/        #  TypeScript type definitions
│   │   └── test/         #  Test setup
│   ├── e2e/              #  Playwright E2E tests
│   │   ├── auth.spec.ts          #  Login / Register flows
│   │   ├── channels.spec.ts      #  Channel CRUD
│   │   ├── chat.spec.ts          #  Messaging
│   │   ├── dm.spec.ts            #  Direct messages
│   │   ├── threads.spec.ts       #  Thread replies
│   │   ├── permissions.spec.ts   #  Channel membership
│   │   ├── reactions.spec.ts     #  Emoji reactions
│   │   ├── search.spec.ts        #  Full-text search
│   │   └── helpers.ts            #  Shared E2E utilities
│   └── vitest.config.ts #  Unit test config
├── tests/                #  Rust integration tests
│   └── integration/      #  175 tests across 4 suites
├── scripts/              # Utility scripts
│   ├── build.sh          #  One-click build
│   ├── bench.sh          #  Benchmark suite
│   └── gen-self-signed-cert.sh
├── deploy/               # Production deployment
│   ├── install.sh        #  Install as systemd service
│   ├── im-server.service #  systemd unit file
│   └── nginx.conf        #  Nginx reverse proxy config
├── certs/                # TLS certificates
└── data/                 # Runtime data (SQLite DB, uploads)
```

## API Overview

All API endpoints are prefixed with `/api`. Authentication via `Authorization: Bearer <token>` header (except auth endpoints).

### Authentication

| Method | Path                  | Description                  |
|--------|-----------------------|------------------------------|
| POST   | `/api/auth/register`  | Register new user            |
| POST   | `/api/auth/login`     | Login, returns JWT token     |

### Channels

| Method | Path                            | Description              |
|--------|---------------------------------|--------------------------|
| GET    | `/api/channels`                 | List all channels        |
| POST   | `/api/channels`                 | Create a channel         |
| GET    | `/api/channels/{id}`            | Get channel details      |
| PATCH  | `/api/channels/{id}`            | Update channel           |
| POST   | `/api/channels/{id}/archive`    | Archive a channel        |
| POST   | `/api/channels/{id}/unarchive`  | Unarchive a channel      |

### Messages & Threads

| Method | Path                                                   | Description                |
|--------|--------------------------------------------------------|----------------------------|
| GET    | `/api/channels/{channel_id}/messages`                  | List messages (paginated)  |
| POST   | `/api/channels/{channel_id}/messages`                  | Send a message             |
| DELETE | `/api/messages/{message_id}`                           | Delete a message           |
| GET    | `/api/channels/{channel_id}/messages/{msg_id}/thread`  | Get thread replies         |

### Reactions

| Method | Path                                               | Description       |
|--------|----------------------------------------------------|-------------------|
| GET    | `/api/messages/{message_id}/reactions`             | Get reactions     |
| POST   | `/api/messages/{message_id}/reactions`             | Add reaction      |
| DELETE | `/api/messages/{message_id}/reactions/{emoji}`     | Remove reaction   |

### Direct Messages

| Method | Path       | Description          |
|--------|------------|----------------------|
| GET    | `/api/dm/` | List DM conversations|
| POST   | `/api/dm/` | Create/open a DM     |

### Files

| Method | Path                  | Description         |
|--------|-----------------------|---------------------|
| POST   | `/api/files/upload`   | Upload a file       |
| GET    | `/api/files/{file_id}`| Download a file     |

### Search

| Method | Path              | Description              |
|--------|-------------------|--------------------------|
| GET    | `/api/search`     | Full-text search messages|

### Join Requests & Invitations

| Method | Path                                    | Description              |
|--------|-----------------------------------------|--------------------------|
| POST   | `/api/channels/{id}/join-request`       | Request to join channel  |
| GET    | `/api/requests`                         | List join requests       |
| PUT    | `/api/requests/{id}/approve`            | Approve a request        |
| PUT    | `/api/requests/{id}/reject`             | Reject a request         |
| POST   | `/api/channels/{id}/invitations`        | Create an invitation     |
| GET    | `/api/invitations`                      | List invitations         |
| PUT    | `/api/invitations/{id}/accept`          | Accept an invitation     |
| PUT    | `/api/invitations/{id}/reject`          | Reject an invitation     |

### WebSocket

Connect to `/ws?token=<jwt_token>` for real-time events:
- New messages (broadcast)
- Typing indicators
- Presence updates (online/offline)
- Message reactions

### Health

| Method | Path          | Description            |
|--------|---------------|------------------------|
| GET    | `/api/health` | Server health check    |
| GET    | `/`           | Root health check      |

## Deployment

### Manual Deployment

```bash
# 1. Build the binary
./scripts/build.sh

# 2. Run the install script (as root)
sudo ./deploy/install.sh target/release/im-server

# 3. Edit the environment file
sudo nano /opt/im-server/.env

# 4. Start the service
sudo systemctl start im-server
sudo systemctl status im-server
```

### Nginx Reverse Proxy

The repo includes `deploy/nginx.conf` — a production-grade reverse proxy configuration with:

- TLS termination
- HTTP → HTTPS redirection
- WebSocket support (long-lived connections)
- Security headers
- Request size limits (50 MB)
- Rate limiting

Place it at `/etc/nginx/sites-available/im-server`, adjust `server_name` and certificate paths, then enable:

```bash
sudo ln -s /etc/nginx/sites-available/im-server /etc/nginx/sites-enabled/
sudo nginx -t && sudo systemctl reload nginx
```

### Environment Variables

| Variable       | Default                    | Description                            |
|---------------|----------------------------|----------------------------------------|
| `DATABASE_URL`| `sqlite:vast.db`           | SQLite database path                   |
| `JWT_SECRET`  | `dev-secret-change-me`     | JWT signing secret (REQUIRED in prod)  |
| `INVITE_CODE` | `IM2024`                   | Registration invite code               |
| `SERVER_PORT` | `3000`                     | HTTP listen port                       |
| `UPLOAD_MAX_SIZE` | `52428800`            | Max upload size in bytes (50 MiB)      |
| `TLS_MODE`    | `none`                     | `none`, `self-signed`, or `lets-encrypt`|

### systemd Service

The service runs as `im-server` user with strict hardening:
- `NoNewPrivileges=true`, `PrivateTmp=true`, `ProtectSystem=strict`
- Read/write access only to `/opt/im-server` and `/var/log/im-server`
- File descriptor limit: 65536

## Development

```bash
# Run tests (backend + frontend unit)
make test

# Run E2E tests (requires servers running on :3000 + :5173)
make test-e2e

# Run Rust integration tests only (175 tests)
make test-backend

# Run lints
make clippy

# Clean build artifacts
make clean

# Parallel dev servers (backend + hot-reload frontend)
make dev
```

Benchmarks are available via `scripts/bench.sh` — tests insert throughput, concurrent read latency, and WebSocket memory usage.

## License

MIT

# VAST

### A real-time IM server you can own.

VAST is a self-hosted instant messaging platform that ships as a **single binary** — Rust on the backend, React SPA embedded at compile time, and SQLite for zero-config persistence. No containers, no microservices, no cloud dependency. Just one process, one database file, and your own hardware.

---

## Why I Built This

I wanted a team chat tool that didn't outsource its database, its auth, or its uptime to someone else's cloud. Something I could drop onto a $5 VPS and immediately have channels, threads, reactions, file sharing, and full-text search — without configuring Redis, without running a separate database server, without wrestling with Docker Compose.

I also wanted an AI bot that actually lives in the channel, not in a separate sidebar. When you `@mention` a bot in VAST, it sees the same channel context you do — message history, participants, thread structure — and replies inline, in real time.

So I built it. Rust + React + SQLite + WebSockets. One binary, one database file, zero bullshit.

---

## What It Can Do

### Messaging

- **Channels** — public and private, with archive/unarchive + ZIP download for history export
- **Threads** — nested replies that don't clutter the main channel view
- **Direct Messages** — one-on-one conversations, private by default
- **Reactions** — emoji reactions on any message (Slack-style, not Discord-style — pick any emoji, not just a fixed set)
- **Typing indicators** and **presence** — see who's online and who's typing, live

### Files

- **Upload** with multipart support, up to 50 MiB by default (configurable)
- **Indexed listing** with keyset pagination, grid/list views, and infinite scroll
- **Soft delete** — files are marked deleted but recoverable; clients see 410 Gone for deleted files
- **Access control** — files are scoped to the channel they were uploaded to

### Search

- **Full-text search** across all channel messages
- Indexed via SQLite FTS5, so it's fast even with tens of thousands of messages
- Searches within the user's accessible channels only

### AI Bots

- **Channel-resident Hermes Agent bots** — add them as virtual channel members
- **@mention activation** — bots only respond when explicitly called, by name or display name
- **Full channel context** — message history, participants, and thread structure sent to the bot on each @mention
- **OpenAI-compatible API** — works with any OpenAI-compatible endpoint (local LLMs, cloud APIs, anything)
- **Admin-managed** — create, configure, test connectivity, add/remove from channels from the admin console

### Access Control

- **JWT authentication** — access + refresh token pair (15 min / 7 days), Argon2id password hashing
- **Token epoch revocation** — disable a user and every JWT they hold is instantly invalid
- **Invite codes** — admin-managed, with usage limits and enable/disable toggle
- **Join requests** — for private channels; owners approve or reject
- **Invitations** — channel owners can invite specific users directly

### Admin Console

A full admin panel isolated from the main app — separate JWT domain, separate login, so a compromised user token can't touch admin endpoints.

- **Dashboard** — user count, channel count, message count at a glance
- **User management** — disable, force-logout, reset password, delete
- **Invite code management** — create, toggle, reset usage count, delete
- **Audit logging** — every admin action logged with timestamp, admin username, action type, and target
- **Bot management** — CRUD, test connectivity, assign to channels

### Web Push Notifications

Browser push notifications via service worker. Subscribe from the app, get notified when someone @mentions you or sends a DM while you're away.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        Client                               │
│  ┌─────────┐  ┌────────────────┐  ┌──────────────────────┐ │
│  │  React  │  │  WebSocket     │  │  REST API calls      │ │
│  │  (SPA)  │  │  (real-time)   │  │  (auth, files, ...)  │ │
│  └────┬────┘  └───────┬────────┘  └──────────┬───────────┘ │
└───────┼───────────────┼──────────────────────┼─────────────┘
        │               │                      │
        ▼               ▼                      ▼
┌─────────────────────────────────────────────────────────────┐
│                      IM Server (Axum)                        │
│  ┌───────────┐ ┌───────────┐ ┌───────────┐ ┌─────────────┐ │
│  │   Auth    │ │ Channels  │ │  Search   │ │   Files     │ │
│  │   (JWT)   │ │ + Threads │ │  (FTS5)   │ │  (50 MiB)  │ │
│  ├───────────┤ ├───────────┤ ├───────────┤ ├─────────────┤ │
│  │  Bots     │ │    DM     │ │ Reactions │ │  Web Push   │ │
│  │ (Hermes)  │ │           │ │           │ │             │ │
│  ├───────────┤ ├───────────┤ ├───────────┤ ├─────────────┤ │
│  │  Admin    │ │           │ │           │ │             │ │
│  │(JWT+audit)│ │           │ │           │ │             │ │
│  ├───────────┴─┴───────────┴─┴───────────┴─┴─────────────┤ │
│  │         WebSocket Hub (broadcast + presence)            │ │
│  ├────────────────────────────────────────────────────────┤ │
│  │         SQLite (WAL mode, compile-time migrations)      │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

### Why These Choices

**Rust + Axum** — I wanted a language that compiles to a single binary and a framework that's fast without being bloated. Axum's extractor-based middleware and built-in WebSocket support make the code clean and the runtime lean. Memory usage at idle is around 10-15 MB.

**React + Vite + Tailwind** — The frontend is embedded into the Rust binary via `rust-embed`. Vite builds it, Cargo bundles it. No separate frontend deployment, no CDN, no build steps on the server.

**SQLite** — The database is a single file. Back it up with `cp`. Migrations are embedded at compile time, so the server auto-creates and auto-migrates on first run. WAL mode gives concurrent read performance good enough for a team-sized IM server.

**WebSocket** — Axum's native WebSocket support means no extra dependency for real-time. Messages, typing, presence, reactions — all go over the same persistent connection. The broadcast hub uses Tokio channels internally.

---

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs) 1.93+
- [Bun](https://bun.sh) 1.2+

### Development

```bash
git clone <repo-url> && cd vast
cp .env.example .env

# Generate a real JWT secret in production:
# openssl rand -base64 48

make dev
```

Backend starts on **http://localhost:3000**, frontend dev server on **http://localhost:5173** (with API/WS proxy to the backend).

Admin console: **http://localhost:5173/admin/login** — `admin` / `admin123` in dev mode.

### Production Build

```bash
./scripts/build.sh

# Binary: target/release/im-server
# The frontend is embedded — no separate static file serving needed.
```

### Run

```bash
cp .env target/release/
./target/release/im-server

# With TLS:
TLS_MODE=self-signed ./target/release/im-server
```

---

## Tech Stack

| Layer            | Technology                                       |
|------------------|--------------------------------------------------|
| Language         | Rust 2024, TypeScript 6.0                        |
| Web framework    | Axum 0.8 (ws + multipart)                        |
| Async runtime    | Tokio 1.52                                       |
| Database         | SQLite via sqlx 0.8 (WAL mode, FTS5)             |
| Auth             | jsonwebtoken 10.4, Argon2 0.5                    |
| Real-time        | WebSocket (Axum built-in), Tokio broadcast       |
| TLS              | rustls via axum-server                            |
| Frontend         | React 19, Vite 7, Tailwind CSS 4                  |
| State management | Zustand 5 (client), TanStack React Query 5 (server) |
| Routing          | React Router 7                                   |
| HTTP client      | reqwest 0.12 (bot API calls)                     |
| Push             | web-push 0.11                                    |
| Testing          | cargo test, vitest, Playwright (E2E)             |

---

## Project Layout

```
vast/
├── src/                      # Rust backend
│   ├── main.rs               # Entrypoint, TLS setup, graceful shutdown
│   ├── lib.rs                # AppState, router, health check
│   ├── embed.rs              # Frontend SPA embedding (rust-embed)
│   ├── error.rs              # Unified error → JSON responses
│   ├── auth/
│   │   ├── mod.rs            # JWT creation/validation, Argon2 hashing
│   │   ├── middleware.rs     # AuthenticatedUser extractor
│   │   └── admin.rs          # Admin JWT domain (separate from user)
│   ├── db/mod.rs             # Pool init, WAL, compile-time migrations
│   ├── ws/
│   │   ├── mod.rs            # Connection pool, broadcast, heartbeat
│   │   └── protocol.rs       # ClientEvent / ServerEvent types
│   ├── bot/
│   │   ├── mod.rs
│   │   └── hermes.rs         # OpenAI-compatible HTTP client
│   ├── push/
│   │   ├── mod.rs            # VAPID key management
│   │   └── sender.rs         # Web Push sender
│   ├── api/
│   │   ├── mod.rs            # Sub-router
│   │   ├── auth.rs           # Register / Login
│   │   ├── channels.rs       # Channel CRUD + archive
│   │   ├── channel_members.rs
│   │   ├── messages.rs       # Messages + threads
│   │   ├── dm.rs             # Direct messages
│   │   ├── files.rs          # Upload, download, listing, soft delete
│   │   ├── reactions.rs      # Emoji reactions
│   │   ├── search.rs         # Full-text search (FTS5)
│   │   ├── requests.rs       # Join requests
│   │   ├── invitations.rs
│   │   ├── presence.rs
│   │   ├── push.rs           # Web Push subscriptions
│   │   ├── trains.rs         # Collaborative train feature
│   │   ├── votes.rs          # Polls/voting
│   │   └── admin/
│   │       ├── mod.rs        # Dashboard, users, invite codes, audit
│   │       └── bots.rs       # Bot CRUD + test endpoint
│   └── db/migrations/        # 9 compile-time SQL migrations
│       ├── 001_initial_schema
│       ├── 002_add_session_active
│       ├── 003_add_trains
│       ├── 004_add_votes
│       ├── 005_token_epoch
│       ├── 006_admin_audit_logs
│       ├── 007_bots
│       ├── 008_file_index
│       └── 009_push_tables
├── frontend/                 # React SPA
│   └── src/
│       ├── main.tsx          # Entrypoint + service worker
│       ├── App.tsx           # Router (user + admin routes)
│       ├── api/              # API client modules per feature
│       ├── components/       # UI components
│       ├── pages/            # Route pages
│       │   └── admin/        # Admin pages (Dashboard, Users, Bots, etc.)
│       ├── hooks/            # useWebSocket, useUnreadTracker, useCursorSync
│       ├── stores/           # Zustand stores (auth, channel, message, unread)
│       └── types/            # TypeScript interfaces
├── tests/integration/        # 28 Rust integration tests
├── deploy/
│   ├── install.sh            # systemd service installer
│   ├── im-server.service     # Hardened systemd unit
│   └── nginx.conf            # Production reverse proxy
├── scripts/
│   ├── build.sh              # One-click build
│   ├── bench.sh              # Benchmark suite
│   ├── dev-server.sh         # Dev mode launcher
│   ├── e2e-test.sh
│   ├── gen-self-signed-cert.sh
│   └── clean-db.sh
├── certs/                    # TLS certificates
└── data/                     # Runtime data (DB, uploads)
```

---

## API Reference

All endpoints prefixed with `/api`. Authentication via `Authorization: Bearer <jwt_token>`.

### Auth

| Method | Path                  | Description       |
|--------|-----------------------|-------------------|
| POST   | `/api/auth/register`  | Register new user |
| POST   | `/api/auth/login`     | Login, get JWT    |

### Channels

| Method | Path                            | Description         |
|--------|---------------------------------|---------------------|
| GET    | `/api/channels`                 | List channels       |
| POST   | `/api/channels`                 | Create a channel    |
| GET    | `/api/channels/{id}`            | Get channel details |
| PATCH  | `/api/channels/{id}`            | Update channel      |
| POST   | `/api/channels/{id}/archive`    | Archive channel     |
| POST   | `/api/channels/{id}/unarchive`  | Unarchive channel   |

### Messages & Threads

| Method | Path                                                    | Description               |
|--------|---------------------------------------------------------|---------------------------|
| GET    | `/api/channels/{channel_id}/messages`                   | List messages (paginated) |
| POST   | `/api/channels/{channel_id}/messages`                   | Send a message            |
| DELETE | `/api/messages/{message_id}`                            | Delete a message          |
| GET    | `/api/channels/{channel_id}/messages/{msg_id}/thread`   | Get thread replies        |

### Reactions

| Method | Path                                             | Description     |
|--------|--------------------------------------------------|-----------------|
| GET    | `/api/messages/{message_id}/reactions`            | Get reactions   |
| POST   | `/api/messages/{message_id}/reactions`            | Add reaction    |
| DELETE | `/api/messages/{message_id}/reactions/{emoji}`    | Remove reaction |

### Direct Messages

| Method | Path       | Description          |
|--------|------------|----------------------|
| GET    | `/api/dm/` | List DM conversations|
| POST   | `/api/dm/` | Create/open a DM     |

### Files

| Method | Path                  | Description     |
|--------|-----------------------|-----------------|
| POST   | `/api/files/upload`   | Upload a file   |
| GET    | `/api/files/{id}`     | Download a file |

### Search

| Method | Path            | Description              |
|--------|-----------------|--------------------------|
| GET    | `/api/search`   | Full-text message search |

### Join Requests & Invitations

| Method | Path                                    | Description           |
|--------|-----------------------------------------|-----------------------|
| POST   | `/api/channels/{id}/join-request`       | Request to join       |
| GET    | `/api/requests`                         | List join requests    |
| PUT    | `/api/requests/{id}/approve`            | Approve               |
| PUT    | `/api/requests/{id}/reject`             | Reject                |
| POST   | `/api/channels/{id}/invitations`        | Create invitation     |
| GET    | `/api/invitations`                      | List invitations      |
| PUT    | `/api/invitations/{id}/accept`          | Accept                |
| PUT    | `/api/invitations/{id}/reject`          | Reject                |

### Admin Console

All admin endpoints require a separate admin JWT (`/api/admin/login`).

| Method | Path                                    | Description              |
|--------|-----------------------------------------|--------------------------|
| POST   | `/api/admin/login`                      | Admin login              |
| POST   | `/api/admin/logout`                     | Admin logout             |
| POST   | `/api/admin/refresh`                    | Refresh admin token      |
| GET    | `/api/admin/me`                         | Get admin info           |
| GET    | `/api/admin/dashboard`                  | Dashboard stats          |
| GET    | `/api/admin/users`                      | List users               |
| GET    | `/api/admin/users/{id}`                 | Get user details         |
| PATCH  | `/api/admin/users/{id}`                 | Update (disable/enable)  |
| POST   | `/api/admin/users/{id}/reset-password`  | Reset user password      |
| DELETE | `/api/admin/users/{id}`                 | Delete user              |
| GET    | `/api/admin/invite-codes`               | List invite codes        |
| POST   | `/api/admin/invite-codes`               | Create invite code       |
| PATCH  | `/api/admin/invite-codes/{code}`        | Update invite code       |
| DELETE | `/api/admin/invite-codes/{code}`        | Delete invite code       |
| GET    | `/api/admin/audit-logs`                 | Audit log (filterable)   |

### Bots

| Method | Path                          | Description                  |
|--------|-------------------------------|------------------------------|
| GET    | `/api/bots`                   | List active bots (public)    |
| POST   | `/api/admin/bots`             | Create bot (admin)           |
| GET    | `/api/admin/bots`             | List all bots (admin)        |
| PATCH  | `/api/admin/bots/:id`         | Update bot (admin)           |
| DELETE | `/api/admin/bots/:id`         | Delete bot (admin)           |
| POST   | `/api/admin/bots/:id/test`    | Test bot connectivity (admin)|
| POST   | `/api/channels/:id/bots`      | Add bot to channel (owner)   |

### WebSocket

Connect to `/ws?token=<jwt_token>`. Events streamed over the connection:

- **New messages** (broadcast to channel)
- **Typing indicators**
- **Presence updates** (online/offline)
- **Message reactions**
- **Message/status changes**

### Health

| Method | Path          | Description         |
|--------|---------------|---------------------|
| GET    | `/api/health` | Server health check |
| GET    | `/`           | Root health check   |

---

## Deployment

### systemd (Recommended)

```bash
./scripts/build.sh
sudo ./deploy/install.sh target/release/im-server
sudo nano /opt/im-server/.env   # Set JWT_SECRET and ADMIN_PASSWORD
sudo systemctl start im-server
```

The systemd unit runs as a dedicated `im-server` user with hardening:

- `NoNewPrivileges=true`, `PrivateTmp=true`, `ProtectSystem=strict`
- Write access limited to `/opt/im-server` and `/var/log/im-server`
- File descriptor limit: 65536

### Nginx Reverse Proxy

The included `deploy/nginx.conf` provides:

- TLS termination
- HTTP → HTTPS redirect
- WebSocket upgrade support
- Security headers (HSTS, XSS, frame options)
- 50 MiB request body limit
- Rate limiting

```bash
sudo cp deploy/nginx.conf /etc/nginx/sites-available/im-server
sudo ln -s /etc/nginx/sites-available/im-server /etc/nginx/sites-enabled/
sudo nginx -t && sudo systemctl reload nginx
```

### Environment Variables

| Variable          | Default                  | Description                         |
|-------------------|--------------------------|-------------------------------------|
| `DATABASE_URL`    | `sqlite:vast.db`         | SQLite path                         |
| `JWT_SECRET`      | `dev-secret-change-me`   | **Change this in production**       |
| `INVITE_CODE`     | `IM2024`                 | Registration invite code            |
| `SERVER_PORT`     | `3000`                   | HTTP listen port                    |
| `UPLOAD_MAX_SIZE` | `52428800`               | Max upload (bytes), default 50 MiB  |
| `TLS_MODE`        | `none`                   | `none`, `self-signed`, `lets-encrypt`|
| `ADMIN_USERNAME`  | `admin`                  | Admin console username              |
| `ADMIN_PASSWORD`  | (empty = disabled)       | Admin console password              |

---

## Development

```bash
# Run all tests
make test

# Frontend unit tests (~180 tests)
cd frontend && bun test

# Backend tests (259 unit + 28 integration)
make test-backend

# E2E tests (requires dev servers on :3000 + :5173)
make test-e2e

# Lint
make clippy

# Clean
make clean
```

Benchmarks: `scripts/bench.sh` — insert throughput, concurrent read latency, WebSocket memory usage.

---

## License

MIT

---

*Built because someone had to.*

# VAST

### A real-time IM server you can own.

VAST is a self-hosted instant messaging platform that ships as a **single binary** — Rust on the backend, React SPA embedded at compile time, and SQLite for zero-config persistence. No containers, no microservices, no cloud dependency. Just one process, one database file, and your own hardware.

---

## Why I Built This

I wanted a team chat tool that didn't outsource its database, its auth, or its uptime to someone else's cloud. Something I could drop onto a $5 VPS and immediately have channels, threads, reactions, file sharing, and full-text search — without configuring Redis, without running a separate database server, without wrestling with Docker Compose.

I also wanted an AI bot that actually lives in the channel, not in a separate sidebar. When you `@mention` a bot in VAST, it sees the same channel context you do — message history, participants, thread structure — and replies inline, in real time.

So I built it. Rust + React + SQLite + WebSockets. One binary, one database file, zero bullshit.

---

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs) 1.93+
- [Bun](https://bun.sh) 1.2+

### Development

```bash
git clone https://github.com/ipconfiger/vast && cd vast
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

### Docker

```bash
cp .env.docker.example .env.docker
# Edit .env.docker — at minimum set JWT_SECRET
docker compose up -d
```

---

## Configuration

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

### Configuring AI Bots

#### HTTP Mode (Direct)

For bots with a public API endpoint（OpenAI、Groq、self-hosted LLM with public URL）:

1. Open the admin panel → Bots → **Create Bot**
2. Set **Connection Mode** to `HTTP`
3. Fill in the **API URL**（e.g. `https://api.openai.com`）and **API Key**
4. Assign the bot to a channel
5. Users can now `@botname` to trigger replies

#### Connector Mode (WebSocket — No Public IP Needed)

For local LLMs behind NAT/firewall（Ollama、LM Studio、vLLM、Hermes Agent）:

1. Open the admin panel → Bots → **Create Bot**
2. Set **Connection Mode** to `Connector`
3. **Copy the Connection Key** shown after creation（only shown once）
4. Assign the bot to a channel
5. Choose one of the two connector options:

**Option A: Hermes Agent Plugin**（recommended for Hermes users）

```bash
cp tools/hermes-vast-plugin.py hermes-agent/gateway/platforms/vast.py
```

Then add to your Hermes Agent `config.yaml`:

```yaml
platforms:
  - type: vast
    connection_key: "your-connection-key"
    ws_url: "ws://your-vast-server:3000/ws/bot"
```

Launch with `hermes gateway` — Hermes manages the WebSocket lifecycle, and your bot inherits Hermes' skill system, memory, and user model.

**Option B: Standalone Connector**（works with any OpenAI-compatible API）

```bash
cd tools
pip install websockets httpx pyyaml
# Edit bot-connector.yaml — paste your connection_key and configure the LLM endpoint
python3 bot-connector.py --config bot-connector.yaml
```

```yaml
# bot-connector.yaml — example with local Ollama
vast:
  url: "ws://your-vast-server:3000/ws/bot"
bots:
  - connection_key: "your-uuid-from-admin-panel"
    llm:
      url: "http://localhost:11434/v1"   # Ollama
      model: "qwen2.5:7b"
      api_key: ""
    system_prompt: "你是一个友好的中文助手"
```

### Deployment

#### systemd (Recommended)

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

#### Nginx Reverse Proxy

The included `deploy/nginx.conf` provides TLS termination, WebSocket upgrade, security headers, and rate limiting.

```bash
sudo cp deploy/nginx.conf /etc/nginx/sites-available/im-server
sudo ln -s /etc/nginx/sites-available/im-server /etc/nginx/sites-enabled/
sudo nginx -t && sudo systemctl reload nginx
```

---

## What It Can Do

### Messaging

- **Channels** — public and private, with archive/unarchive + ZIP download for history export
- **Threads** — nested replies that don't clutter the main channel view
- **Direct Messages** — one-on-one conversations, private by default
- **Reactions** — emoji reactions on any message (pick any emoji, not just a fixed set)
- **Message quoting** — reply with inline quote preview for context
- **Typing indicators** and **presence** — see who's online and who's typing, live

### Files

- **Upload** with multipart support, up to 50 MiB by default (configurable)
- **Indexed listing** with keyset pagination, grid/list views, and infinite scroll
- **Soft delete** — files are marked deleted but recoverable; clients see 410 Gone
- **Access control** — files are scoped to the channel they were uploaded to

### Search

- **Full-text search** across all channel messages
- Indexed via SQLite FTS5, so it's fast even with tens of thousands of messages
- Searches within the user's accessible channels only

### AI Bots

- **Channel-resident bots** — add them as virtual channel members that reply inline
- **@mention activation** — bots only respond when explicitly called, by name or display name
- **Full channel context** — message history, participants, and thread structure sent to the bot on each @mention
- **Two connection modes**:
  - **HTTP** — VAST calls your bot's OpenAI-compatible endpoint directly. Requires a public URL（ideal for cloud APIs like OpenAI, Groq）.
  - **Connector（WebSocket）** — your bot connects **out** to VAST. No public IP needed. Two connector options:
    - **Standalone Connector** — a ~200-line Python script that bridges VAST to any OpenAI-compatible API（Ollama, vLLM, LM Studio）.  One process can manage multiple bots.
    - **Hermes Agent Plugin** — a native gateway plugin for [Hermes Agent](https://github.com/nousresearch/hermes-agent).  Zero extra process; leverages Hermes' skill system, memory, and user model.
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
│  │ + WS /bot │ │           │ │           │ │             │ │
│  ├───────────┤ ├───────────┤ ├───────────┤ ├─────────────┤ │
│  │  Admin    │ │           │ │           │ │             │ │
│  │(JWT+audit)│ │           │ │           │ │             │ │
│  ├───────────┴─┴───────────┴─┴───────────┴─┴─────────────┤ │
│  │         WebSocket Hub (broadcast + presence)            │ │
│  ├────────────────────────────────────────────────────────┤ │
│  │         SQLite (WAL mode, compile-time migrations)      │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│              Bot Connector (optional)                        │
│  ┌─────────────────┐                                        │
│  │  Connector /    │  ── HTTP ──►  LLM API                  │
│  │  Hermes Plugin  │              (Ollama / vLLM / OpenAI) │
│  └─────────────────┘                                        │
└─────────────────────────────────────────────────────────────┘
```

### Why These Choices

**Rust + Axum** — I wanted a language that compiles to a single binary and a framework that's fast without being bloated. Axum's extractor-based middleware and built-in WebSocket support make the code clean and the runtime lean. Memory usage at idle is around 10-15 MB.

**React + Vite + Tailwind** — The frontend is embedded into the Rust binary via `rust-embed`. Vite builds it, Cargo bundles it. No separate frontend deployment, no CDN, no build steps on the server.

**SQLite** — The database is a single file. Back it up with `cp`. Migrations are embedded at compile time, so the server auto-creates and auto-migrates on first run. WAL mode gives concurrent read performance good enough for a team-sized IM server.

**WebSocket** — Axum's native WebSocket support means no extra dependency for real-time. Messages, typing, presence, reactions — all go over the same persistent connection. The broadcast hub uses Tokio channels internally.

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
│   ├── auth/                 # JWT creation/validation + admin domain
│   ├── db/                   # Pool init, WAL, compile-time migrations
│   ├── ws/                   # Connection pool, broadcast, heartbeat, /ws/bot
│   ├── bot/                  # OpenAI-compatible HTTP client
│   ├── push/                 # Web Push (VAPID)
│   └── api/                  # REST endpoints (18 modules)
│       └── admin/            # Admin console endpoints
├── frontend/                 # React SPA
│   └── src/
│       ├── components/       # UI components (MessageBubble, MessageInput, ...)
│       ├── pages/            # Route pages + admin/
│       ├── hooks/            # useWebSocket, useCursorSync
│       └── stores/           # Zustand stores
├── tools/                    # Bot connectors
│   ├── bot-connector.py      # Standalone WebSocket connector
│   ├── bot-connector.yaml    # Example configuration
│   ├── bot-connector.service # systemd unit
│   └── hermes-vast-plugin.py # Hermes Agent gateway plugin
├── db/migrations/            # 12 compile-time SQL migrations
├── tests/integration/        # 28 Rust integration tests
├── deploy/                   # systemd + nginx deployment files
└── scripts/                  # Build, bench, dev scripts
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
| GET    | `/api/admin/dashboard`                  | Dashboard stats          |
| GET    | `/api/admin/users`                      | List users               |
| PATCH  | `/api/admin/users/{id}`                 | Update (disable/enable)  |
| DELETE | `/api/admin/users/{id}`                 | Delete user              |
| GET    | `/api/admin/invite-codes`               | List invite codes        |
| POST   | `/api/admin/invite-codes`               | Create invite code       |
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
| POST   | `/api/admin/bots/:id/test`    | Test connectivity (admin)    |
| POST   | `/api/channels/:id/bots`      | Add bot to channel (owner)   |

### WebSocket (Bot Connector)

Connect bot connectors at `/ws/bot?key=<connection_key>`.

| Direction | Event | Description |
|-----------|-------|-------------|
| VAST → Connector | `BotMention` | User @mentioned the bot, with channel context |
| Connector → VAST | `BotReply` | Connector sends back the LLM response text |

### WebSocket (User)

Connect at `/ws?token=<jwt_token>`. Events: New messages, typing, presence, reactions, message changes.

### Health

| Method | Path          | Description         |
|--------|---------------|---------------------|
| GET    | `/api/health` | Server health check |
| GET    | `/`           | Root health check   |

---

## Development

```bash
# Run all tests
make test

# Frontend unit tests
cd frontend && bun test

# Backend tests
make test-backend

# E2E tests
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

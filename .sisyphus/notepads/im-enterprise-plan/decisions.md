# Architectural Decisions — im-enterprise-plan

## Database
- SQLite + WAL mode
- Single writer queue, pooled readers
- FTS5 external content for search
- Litestream optional backup

## Auth
- JWT: access 15min, refresh 7d
- Argon2id: m=65536, t=3, p=1
- WS auth: JWT via query param before upgrade

## WebSocket
- DashMap connection pool
- tokio::broadcast per channel
- Heartbeat 15s, timeout 60s
- BroadcastPolicy::DropConnection

## Frontend
- React 19.2.7, Vite 8.1.2, TailwindCSS 4.3.1
- Zustand for state (NOT Redux)
- TanStack Query for REST caching
- Monaco Editor lazy-loaded for code snippets

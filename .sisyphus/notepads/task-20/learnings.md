# T20 Learnings

- Routes are registered in TWO places: `api/mod.rs` (used by tests via `api::routes()`) and `main.rs` (used by the server via `api_routes()`). Both must be kept in sync.
- The test setup pattern uses `sqlite::memory:` DB, a helper `setup()` function returning (Router, pool, tokens, channel_id), and a `request()` helper making authenticated JSON calls.
- Channel ownership is checked by comparing `channel.owner_id` with `user.0` from AuthenticatedUser.
- For role-based access (owner/admin), use `auth::middleware::require_role()`.
- The `ConnectionPool::notify_channel()` broadcasts `ServerEvent` to all WebSocket subscribers of a channel.
- Pre-existing modules not declared in `api/mod.rs` (like `dm`) must be added before they can be used in `api/mod.rs` routes.

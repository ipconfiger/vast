# Learnings — IM Enterprise

## T34: TLS/HTTPS

### Dependencies
- axum-server 0.8 with `tls-rustls` feature → provides `axum_server::bind_rustls()` and `RustlsConfig`
- rustls 0.23 → TLS library
- rustls-pemfile 2.2 → PEM certificate parsing

### Architecture
- `TlsMode` enum: `None` (HTTP only), `SelfSigned` (certs/cert.pem + certs/key.pem), `LetsEncrypt` (certs/fullchain.pem + certs/privkey.pem)
- `TLS_MODE` env var: reads at startup via `AppConfig::from_env()`
- TLS mode: spawns HTTPS on 3443 + HTTP redirect on 3000 via `tokio::spawn` + `axum_server::bind_rustls`
- HTTP→HTTPS redirect: `axum::http::Uri` → `Redirect::temporary`
- Graceful shutdown: `tokio::signal::ctrl_c()` → `.abort()` on both server tasks

### Fixes applied
- `db/mod.rs`: `log_pragma_values` used `&sqlx::SqlitePool` but sqlx 0.9 `fetch_one` requires `Executor<'static>` — changed to `impl Executor<'static> + Copy` pattern
- 10 test files needed `tls_mode: crate::TlsMode::None` added to `AppConfig` initializers (2 already had it)

### Verification
- `cargo test`: 144 passed, 0 failed
- Smoke test: `TLS_MODE=self-signed cargo run` → `curl -k https://localhost:3443/api/health` → `{"db":"connected","status":"ok"}`
- HTTP redirect: `curl http://localhost:3000/api/health` → 307 → `https://localhost:3443/api/health`

## F4: Scope Fidelity Check — 2026-07-01

### Key Findings
- **35/38 tasks COMPLIANT** (92%)
- **0 Must NOT violations** across all tasks
- **No git history** — all files untracked, cross-task pollution detectable only structurally
- **5 unaccounted files**: src/lib.rs, migration 002, duplicate api/mod.rs routes, 4 root-level evidence files, certs/

### Issues Found
1. **T36 (MEDIUM)**: Thread page (`<div>Thread View</div>`), DM page (`<div>Direct Message</div>`), Search page (`<div>Search Page</div>`) are stubs
2. **T14 (MINOR)**: `around_cursor` param not implemented (only after_cursor)
3. **T37 (MINOR)**: No ws_flow integration test, no Playwright frontend tests

### Patterns
- Backend: Full API coverage with TDD — all 11 api/ modules have inline tests
- Frontend: Components exist but 3 key view pages are stubs
- Architecture: solid — Axum + WS + SQLite + embedded SPA

### Decisions
- VERDICT: APPROVE WITH ADVISORIES

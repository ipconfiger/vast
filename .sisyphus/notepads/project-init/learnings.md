# Learnings

## T11: Graceful shutdown + health endpoint + CORS

- `TimeoutLayer::with_status_code(StatusCode, Duration)` — note argument order: StatusCode first, Duration second. The old `TimeoutLayer::new(Duration)` is deprecated.
- Health endpoint uses `State<Arc<AppState>>` to access `pool.acquire()` for DB connectivity check — returns `{"status":"ok","db":"connected"}` on success, `{"status":"degraded","db":"error"}` on failure.
- `CorsLayer::permissive()` allows all origins — suitable for dev mode only.
- Graceful shutdown uses `tokio::signal::ctrl_c()` wrapped in an async block, passed to `axum::serve(...).with_graceful_shutdown(...)`.
- SPA fallback via `.fallback(embed::serve_frontend)` — must be placed after all API routes.
- Middleware layer ordering: layers are applied outer-to-inner (last added = outermost). CORS and timeout layers should be applied before `.with_state()`.

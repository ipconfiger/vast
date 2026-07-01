# Tracing Notepad

## Task 9 - Structured Logging

- Tracing init MUST be the first thing in main() — before dotenvy and any other code
- Use `EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))` for env-configurable log level
- Always add `tracing_log::LogTracer::init().ok()` to bridge `log` crate macros to tracing
- ws/mod.rs already had comprehensive instrumentation: `#[instrument(skip(...))]` on all handlers + register/unregister/broadcast
- Key subscriber config: `.with_target(true)`, `.with_thread_ids(true)`, `.with_line_number(true)`
- Pre-existing build errors: embed module (missing frontend/dist), ws/mod.rs (axum 0.8 Message API changes), auth/mod.rs (rand_core feature) — none from tracing changes

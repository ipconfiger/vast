# Channel CRUD — Learnings

## Patterns
- Pool access: Use `&state.pool` (not `&*state.pool`) when `state` is `Arc<AppState>`
- Auth extractor: Import `AuthenticatedUser` from `crate::auth::middleware::AuthenticatedUser`
- Route registration: Channel CRUD routes go in `main.rs`'s `api_routes()` function
- Test JWT secret: Must be `"test-secret"` to avoid polluting env for auth tests (which use `OnceLock`)

## SQLite with sqlx
- Use `sqlx::query_as::<_, T>()` with `FromRow` derive (not `query_as!` — requires compile-time DB)
- SQLite boolean columns (0/1) map to Rust `bool`
- Timestamps stored as `i64` (unix epoch seconds)

## Error handling
- `AppError::BadRequest` maps to 400 status, error code `"BAD_REQUEST"`
- `AppError::ValidationError` maps to 422 status, error code `"VALIDATION_ERROR"`

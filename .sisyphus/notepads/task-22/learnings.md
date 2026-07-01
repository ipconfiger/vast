# T22 — File Upload / Download

## Approach
- Used `axum::extract::Multipart` (requires `multipart` feature on axum)
- MIME whitelist as static array with image/* prefix wildcard
- JSON sidecar (`{uuid}.meta.json`) for metadata instead of DB table
- `RequestBodyLimitLayer` (tower-http 0.7) for size limit — note: the type is `RequestBodyLimitLayer`, NOT `ContentLengthLimitLayer` (that name doesn't exist in tower-http 0.7)

## Files changed
- `Cargo.toml` — added `"multipart"` to axum features, added `mime = "0.3"`
- `src/api/files.rs` — new module with handlers + tests
- `src/api/mod.rs` — added `pub mod files;`
- `src/main.rs` — added routes with `RequestBodyLimitLayer(50*1024*1024)`

## Key decisions
- JSON sidecar for metadata (simpler than DB migration)
- Extension from original filename with mime_guess fallback
- `HeaderMap` + `Vec<u8>` return type for download (works with axum's IntoResponse)
- Multipart field name "file" validated

## Testing
- 5 unit tests cover: upload PNG → 201, disallowed MIME → 415, wrong field → 400, download → correct MIME, missing → 404
- Integration curl tests verified: upload, download integrity, 413 via Content-Length header
- tests require `sqlx::SqlitePool::connect(":memory:")` + running migrations — must be async, no nested Runtime

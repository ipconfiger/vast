# Issues — im-enterprise-plan

## Wave 1 File Conflicts
- `src/main.rs` shared by T1 (create), T6 (router), T10 (fallback), T11 (shutdown/CORS)
  → Solution: Sequence T1 → T6 → T11 (T10 writes embed.rs only)
- `frontend/vite.config.ts` by T2 (create) and T3 (tailwind config)
  → Solution: T3 after T2

## T23 Pre-existing Compile Issues
- `src/api/messages.rs`: `ServerEvent` used in production code (line 273) but import was gated behind `#[cfg(test)]` — moved to module-level imports.
- `src/api/dm.rs`: Missing `use axum::Router;` in the function body — added to file-level imports.
- `src/main.rs`: `ContentLengthLimitLayer` was renamed to `RequestBodyLimitLayer` in tower-http 0.7 — fixed the import.
- `src/api/mod.rs`: `pub mod files;` was duplicated — removed duplicate.
- `api::messages::tests::test_delete_message_*`: 5 tests fail with routing/status code mismatches — pre-existing, unrelated to T23.
- `api::files` module exists but routes reference `api::files::MAX_UPLOAD_SIZE` — this works after adding `pub mod files;` to mod.rs.

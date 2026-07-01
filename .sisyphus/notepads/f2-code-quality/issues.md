# F2 Code Quality Review — Issues

## Clippy Failures (8 total, must fix)

### Unused Imports (4 issues)
1. `src/api/mod.rs:13` — unused import `patch` in routing
2. `src/lib.rs:10` — unused imports `http::Uri`, `patch`, `response::Redirect`
3. `src/api/dm.rs:198` — unused import `get` in inner routing block
4. `src/api/auth.rs:7` — unused import `Serialize`

### Dead Code (2 issues)
5. `src/api/search.rs:31-41` — `SearchResultRow` fields (id, msg_id, channel_id, sender_id, msg_type, payload) flagged as never read — false positive from sqlx::FromRow derive, but lint still fires
6. `src/api/auth.rs:40` — `username` field in `UserRow` never read (also sqlx::FromRow false positive)

### Clippy Style (2 issues)
7. `src/ws/mod.rs:84` — `ConnectionPool::new()` needs `Default` impl (clippy::new-without-default)
8. `src/ws/mod.rs:108` — use `or_default()` instead of `or_insert_with(DashSet::new)` (clippy::unwrap-or-default)

## Bun Test Failure

`e2e/auth.spec.ts` and `e2e/chat.spec.ts` fail because bun:test tries to load Playwright test files that use `test.describe()` from `@playwright/test`. These are e2e tests meant for `npx playwright test`, not `bun test`. Needs test config to exclude e2e/ directory.

## AI Slop Indicators

**Empty doc comments** (`///` with no text) found in:
- `src/ws/mod.rs` (lines 62, 96, 192, 262, 302)
- `src/api/channels.rs` (lines 69, 117, 139, 181, 252, 294)
- `src/api/files.rs` (lines 79, 182)
- `src/api/requests.rs` (lines 39, 127, 161, 212)
- `src/api/reactions.rs` (lines 99, 131, 158)

These are placeholder doc comments that should either be filled in or removed.

## Frontend Quality: EXCELLENT

Zero issues found:
- No `as any` casts
- No `@ts-ignore` or `@ts-expect-error`
- No empty catch blocks
- No `console.log` in production code
- No commented-out code
- No unused imports (tsc would catch)
- tsconfig strict, no errors

## Rust unwrap/expect Usage

401 total across 17 source files. Heavily concentrated in API handlers (expected — error handling via AppError conversion). Most are in `.unwrap_or()` / `.unwrap_or_default()` form which is safe. No raw `.unwrap()` on user input found — all on infrastructure (env vars, server startup, DB config).

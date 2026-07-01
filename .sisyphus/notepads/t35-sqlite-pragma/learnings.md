# T35 — SQLite PRAGMA Tuning

## Changes Made

### 1. PRAGMA logging at startup (`src/db/mod.rs`)
- Added `log_pragma_values()` which queries and logs all 5 T5 PRAGMAs:
  `journal_mode`, `synchronous`, `cache_size`, `mmap_size`, `busy_timeout`
- Each PRAGMA is a separate static string literal to satisfy sqlx's compile-time query check
- Each query is non-fatal (warn on failure)

### 2. PRAGMA optimize on shutdown (`src/db/mod.rs`)
- Added `shutdown()` which runs `PRAGMA optimize` then calls `pool.close().await`
- Called from `main.rs` after `axum::serve` completes on any TLS variant

### 3. Disk space check (`src/db/mod.rs` + callers)
- Added `check_disk_space()` using `fs2::available_space()` — checks >100MB free
- Installed in `send_message()` before INSERT (checks `data_dir`)
- Installed in `upload_file()` before file writes (checks `data_dir`)

### 4. Benchmark script (`scripts/bench.sh`)
- Test 1: inserts 1000 messages with timing → total time + msg/s
- Test 2: 100 concurrent reads → p50/p95/p99 latency
- Test 3: 50 WS connections → RSS memory delta (requires websocat)
- Configurable via `BENCH_BASE_URL`, `BENCH_WS_URL`, env vars

### 5. Pre-existing fixes
- Fixed `tls_mode` field missing in test `AppConfig` constructors across all test modules

## sqlx Gotchas
- `sqlx::query_scalar::<_, String>(sql)` requires `&'static str` — pass literal strings, not variables
- Using a loop with PRAGMA names from a vec fails at compile time
- Solution: inline each call with a separate literal or use a macro

## Dependencies Added
- `fs2 = "0.4"` — for `available_space()` filesystem check

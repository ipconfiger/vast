# Task 5 - Database Initialization Learnings

## foreign_keys PRAGMA
- `PRAGMA foreign_keys` is a per-connection setting in SQLite, NOT a database-level property.
- sqlx sets it via `SqliteConnectOptions::foreign_keys(true)` on each new connection.
- `sqlite3` CLI does NOT set it by default, so running `sqlite3 data/im.db 'PRAGMA foreign_keys;'` will show `0`.
- This is expected behavior. The app correctly enforces foreign keys when connecting through sqlx.

## Verification command
To verify foreign_keys is active in the app context, use:
```bash
sqlite3 -cmd "PRAGMA foreign_keys = ON;" data/im.db "PRAGMA foreign_keys;"
```
This confirms the PRAGMA works when set at connect time.

## Migration files
- SQLite `.up.sql` files contain `CREATE TABLE IF NOT EXISTS` to be idempotent
- Down migration drops triggers first, then FTS5 virtual table, then tables in reverse dependency order
- sqlx's built-in migration system tracks applied migrations in `_sqlx_migrations` table

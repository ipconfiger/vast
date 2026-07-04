-- Rebuild users without token_epoch.
-- SQLite supports DROP COLUMN (3.35+), but the bundled libsqlite3-sys version
-- is not guaranteed across deployment targets, so the table-rebuild pattern
-- is used for portability.
CREATE TABLE users__down005 (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL DEFAULT '',
    password_hash TEXT NOT NULL,
    avatar_url TEXT DEFAULT '',
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

INSERT INTO users__down005 (id, username, display_name, password_hash, avatar_url, created_at)
SELECT id, username, display_name, password_hash, avatar_url, created_at FROM users;

DROP TABLE users;
ALTER TABLE users__down005 RENAME TO users;

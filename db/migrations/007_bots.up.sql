ALTER TABLE users ADD COLUMN is_bot BOOLEAN NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS bots (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    name TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL DEFAULT '',
    api_url TEXT NOT NULL,
    api_key TEXT NOT NULL DEFAULT '',
    system_prompt TEXT NOT NULL DEFAULT '',
    model TEXT NOT NULL DEFAULT 'hermes',
    is_active BOOLEAN NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL
);
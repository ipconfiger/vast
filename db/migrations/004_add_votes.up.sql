-- Votes (polls)
CREATE TABLE IF NOT EXISTS votes (
    id          TEXT PRIMARY KEY,
    channel_id  TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    creator_id  TEXT NOT NULL REFERENCES users(id),
    title       TEXT NOT NULL,
    options     TEXT NOT NULL DEFAULT '[]',
    created_at  INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_votes_channel ON votes(channel_id, created_at DESC);

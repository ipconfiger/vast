-- Trains (chain/relay messages)
CREATE TABLE IF NOT EXISTS trains (
    id          TEXT PRIMARY KEY,
    channel_id  TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    creator_id  TEXT NOT NULL REFERENCES users(id),
    title       TEXT NOT NULL,
    replies     TEXT NOT NULL DEFAULT '[]',
    created_at  INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_trains_channel ON trains(channel_id, created_at DESC);

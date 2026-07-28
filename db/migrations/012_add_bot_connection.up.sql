ALTER TABLE bots ADD COLUMN connection_mode TEXT NOT NULL DEFAULT 'http'
    CHECK(connection_mode IN ('http', 'connector'));

ALTER TABLE bots ADD COLUMN connection_key TEXT NOT NULL DEFAULT '';

CREATE UNIQUE INDEX IF NOT EXISTS idx_bots_connection_key
    ON bots(connection_key) WHERE connection_key != '';

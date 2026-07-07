-- Files: multi-dimensional indexing system for uploaded files
CREATE TABLE IF NOT EXISTS files (
    id TEXT PRIMARY KEY,
    uploader_id TEXT NOT NULL REFERENCES users(id) ON DELETE SET NULL,
    channel_id TEXT REFERENCES channels(id) ON DELETE SET NULL,
    original_name TEXT NOT NULL,
    storage_path TEXT NOT NULL,
    size BIGINT NOT NULL,
    mime_type TEXT NOT NULL,
    extension TEXT NOT NULL,
    is_deleted BOOLEAN NOT NULL DEFAULT 0,
    deleted_at INTEGER,
    deleted_by TEXT REFERENCES users(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Index 1: Channel-scoped file listing (most recent first)
CREATE INDEX IF NOT EXISTS idx_files_channel_created ON files(channel_id, created_at DESC);

-- Index 2: User-scoped file listing (most recent first)
CREATE INDEX IF NOT EXISTS idx_files_uploader_created ON files(uploader_id, created_at DESC);

-- Index 3: File size queries (large-file filtering, quota checks)
CREATE INDEX IF NOT EXISTS idx_files_size ON files(size);

-- Index 4: MIME type filtering (type-aware queries)
CREATE INDEX IF NOT EXISTS idx_files_mime ON files(mime_type);

-- Index 5: Recency sorting (global feed, cleanup ordering)
CREATE INDEX IF NOT EXISTS idx_files_created ON files(created_at DESC);

-- Index 6: Case-insensitive filename search
CREATE INDEX IF NOT EXISTS idx_files_name ON files(original_name COLLATE NOCASE);

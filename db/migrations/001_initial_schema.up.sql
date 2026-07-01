-- Users
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL DEFAULT '',
    password_hash TEXT NOT NULL,
    avatar_url TEXT DEFAULT '',
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Sessions
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL,
    is_refresh BOOLEAN NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    expires_at INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_token ON sessions(token_hash);

-- Invite codes
CREATE TABLE IF NOT EXISTS invite_codes (
    code TEXT PRIMARY KEY,
    created_by_user_id TEXT REFERENCES users(id),
    max_uses INTEGER NOT NULL DEFAULT 100,
    use_count INTEGER NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);
INSERT OR IGNORE INTO invite_codes (code, max_uses, is_active) VALUES ('IM2024', 1000, 1);

-- Channels
CREATE TABLE IF NOT EXISTS channels (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT DEFAULT '',
    owner_id TEXT REFERENCES users(id),
    is_direct BOOLEAN NOT NULL DEFAULT 0,
    is_group_dm BOOLEAN NOT NULL DEFAULT 0,
    is_archived BOOLEAN NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Channel members
CREATE TABLE IF NOT EXISTS channel_members (
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK(role IN ('owner', 'admin', 'member')),
    joined_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (channel_id, user_id)
);
CREATE INDEX IF NOT EXISTS idx_channel_members_user ON channel_members(user_id);

-- Messages
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    msg_id TEXT NOT NULL UNIQUE,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    sender_id TEXT NOT NULL REFERENCES users(id),
    msg_type TEXT NOT NULL CHECK(msg_type IN ('text', 'file', 'code')),
    payload TEXT NOT NULL DEFAULT '{}',
    thread_parent_id INTEGER REFERENCES messages(id) ON DELETE SET NULL,
    deleted_at INTEGER,
    edited_at INTEGER,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);
CREATE INDEX IF NOT EXISTS idx_messages_channel_time ON messages(channel_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_messages_thread ON messages(thread_parent_id, created_at) WHERE thread_parent_id IS NOT NULL;

-- Reactions (Unicode emoji only)
CREATE TABLE IF NOT EXISTS reactions (
    message_id INTEGER NOT NULL REFERENCES messages(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id),
    emoji TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (message_id, user_id, emoji)
);

-- Join requests
CREATE TABLE IF NOT EXISTS join_requests (
    id TEXT PRIMARY KEY,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id),
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'approved', 'rejected')),
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Invitations
CREATE TABLE IF NOT EXISTS invitations (
    id TEXT PRIMARY KEY,
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    inviter_id TEXT NOT NULL REFERENCES users(id),
    invitee_id TEXT NOT NULL REFERENCES users(id),
    status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'accepted', 'rejected')),
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Read receipts
CREATE TABLE IF NOT EXISTS read_receipts (
    user_id TEXT NOT NULL REFERENCES users(id),
    channel_id TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    last_read_message_id INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (user_id, channel_id)
);

-- FTS5 virtual table for message search
CREATE VIRTUAL TABLE IF NOT EXISTS messages_fts USING fts5(
    payload,
    content='messages',
    content_rowid='id',
    tokenize='porter unicode61'
);

-- Triggers to keep FTS5 in sync
CREATE TRIGGER IF NOT EXISTS messages_fts_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, payload) VALUES (new.id, new.payload);
END;

CREATE TRIGGER IF NOT EXISTS messages_fts_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, payload) VALUES ('delete', old.id, old.payload);
END;

CREATE TRIGGER IF NOT EXISTS messages_fts_au AFTER UPDATE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, payload) VALUES ('delete', old.id, old.payload);
    INSERT INTO messages_fts(rowid, payload) VALUES (new.id, new.payload);
END;

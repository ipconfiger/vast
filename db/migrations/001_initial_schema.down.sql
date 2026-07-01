-- Drop triggers first
DROP TRIGGER IF EXISTS messages_fts_au;
DROP TRIGGER IF EXISTS messages_fts_ad;
DROP TRIGGER IF EXISTS messages_fts_ai;

-- Drop FTS5 virtual table
DROP TABLE IF EXISTS messages_fts;

-- Drop tables in reverse dependency order
DROP TABLE IF EXISTS read_receipts;
DROP TABLE IF EXISTS invitations;
DROP TABLE IF EXISTS join_requests;
DROP TABLE IF EXISTS reactions;
DROP TABLE IF EXISTS messages;
DROP TABLE IF EXISTS channel_members;
DROP TABLE IF EXISTS channels;
DROP TABLE IF EXISTS invite_codes;
DROP TABLE IF EXISTS sessions;
DROP TABLE IF EXISTS users;

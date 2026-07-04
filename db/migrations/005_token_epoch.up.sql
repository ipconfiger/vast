-- Per-user epoch counter for forced logout / global token revocation.
-- Incrementing this value invalidates all previously issued JWTs for the user
-- (tokens embed the epoch they were minted with; the auth layer rejects any
-- token whose epoch is older than the column's current value).
ALTER TABLE users ADD COLUMN token_epoch INTEGER NOT NULL DEFAULT 0;

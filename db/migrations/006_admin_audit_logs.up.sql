-- Append-only audit trail for admin-console actions (user bans, role changes,
-- forced logouts, config edits, etc.). The admin-console module writes here;
-- any user-facing admin API SHOULD insert a row on every state-changing call.
CREATE TABLE IF NOT EXISTS admin_audit_logs (
    id           TEXT PRIMARY KEY,
    action       TEXT NOT NULL,
    target_type  TEXT,
    target_id    TEXT,
    details      TEXT,
    performed_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_performed_at
    ON admin_audit_logs(performed_at DESC);

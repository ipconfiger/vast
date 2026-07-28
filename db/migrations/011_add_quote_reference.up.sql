ALTER TABLE messages ADD COLUMN quoted_message_id INTEGER REFERENCES messages(id) ON DELETE SET NULL;

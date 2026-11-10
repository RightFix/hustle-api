ALTER TABLE users
    ADD COLUMN is_suspended BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN suspended_reason TEXT,
    ADD COLUMN suspended_at TIMESTAMPTZ;

CREATE INDEX users_suspended_idx ON users (is_suspended) WHERE is_suspended;

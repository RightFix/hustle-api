ALTER TABLE users
    ADD COLUMN avatar_file TEXT,
    ADD COLUMN email_verified_at TIMESTAMPTZ,
    ADD COLUMN email_messages_enabled BOOLEAN NOT NULL DEFAULT TRUE;

-- Existing accounts are trusted at the moment this ships; new sign-ups
-- must go through email verification.
UPDATE users SET email_verified_at = now();

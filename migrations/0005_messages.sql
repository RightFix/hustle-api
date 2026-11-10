CREATE TABLE conversations (
    id SERIAL PRIMARY KEY,
    user_a INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    user_b INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (user_a < user_b),
    UNIQUE (user_a, user_b)
);
CREATE INDEX idx_conversations_user_a ON conversations (user_a);
CREATE INDEX idx_conversations_user_b ON conversations (user_b);

CREATE TABLE messages (
    id SERIAL PRIMARY KEY,
    conversation_id INTEGER NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    sender_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    body TEXT NOT NULL,
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_messages_conversation ON messages (conversation_id, id);
CREATE INDEX idx_messages_unread ON messages (conversation_id, sender_id, read_at);

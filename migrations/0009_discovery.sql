ALTER TABLE users
    ADD COLUMN location      TEXT,
    ADD COLUMN availability  TEXT,
    ADD COLUMN expected_rate TEXT,
    ADD COLUMN languages     TEXT;

CREATE TABLE saved_gigs (
    user_id    INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    gig_id     INT NOT NULL REFERENCES gigs(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, gig_id)
);

CREATE INDEX saved_gigs_gig_idx ON saved_gigs (gig_id);

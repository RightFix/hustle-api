-- Round 4: hiring & completion pipeline

-- Extend gig lifecycle: open -> in_progress -> completed (or closed at any pre-completion point)
ALTER TABLE gigs DROP CONSTRAINT IF EXISTS gigs_status_check;
ALTER TABLE gigs
    ADD COLUMN IF NOT EXISTS hired_worker_id INT REFERENCES users(id) ON DELETE SET NULL,
    ADD CONSTRAINT gigs_status_check CHECK (status IN ('open', 'in_progress', 'completed', 'closed'));

-- Track application outcomes
ALTER TABLE gig_applications
    ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'accepted', 'rejected'));

-- Mutual completion confirmation
CREATE TABLE IF NOT EXISTS gig_completions (
    gig_id INT PRIMARY KEY REFERENCES gigs(id) ON DELETE CASCADE,
    worker_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    poster_confirmed_at TIMESTAMPTZ,
    worker_confirmed_at TIMESTAMPTZ
);

-- Post-completion ratings (one per rater per gig)
CREATE TABLE IF NOT EXISTS gig_ratings (
    id SERIAL PRIMARY KEY,
    gig_id INT NOT NULL REFERENCES gigs(id) ON DELETE CASCADE,
    rater_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    ratee_id INT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    rating SMALLINT NOT NULL CHECK (rating BETWEEN 1 AND 5),
    comment TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (gig_id, rater_id)
);

CREATE INDEX IF NOT EXISTS gig_ratings_ratee_idx ON gig_ratings (ratee_id, created_at DESC);
CREATE INDEX IF NOT EXISTS gig_ratings_gig_idx ON gig_ratings (gig_id);

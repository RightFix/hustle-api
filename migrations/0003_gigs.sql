ALTER TABLE users
    ADD COLUMN account_type TEXT NOT NULL DEFAULT 'worker'
        CHECK (account_type IN ('worker', 'hirer'));

CREATE TABLE gigs (
    id SERIAL PRIMARY KEY,
    poster_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    description TEXT NOT NULL,
    collar TEXT NOT NULL CHECK (collar IN ('pink', 'gray', 'blue', 'green')),
    location TEXT,
    pay TEXT,
    status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_gigs_poster ON gigs (poster_id);
CREATE INDEX idx_gigs_status ON gigs (status);

CREATE TABLE gig_applications (
    gig_id INTEGER NOT NULL REFERENCES gigs (id) ON DELETE CASCADE,
    worker_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (gig_id, worker_id)
);
CREATE INDEX idx_gig_applications_worker ON gig_applications (worker_id);

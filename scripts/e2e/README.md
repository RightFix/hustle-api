# E2E smoke scripts

Bash/curl smoke tests against a running backend. These are **not** fully isolated test fixtures; they assume a dev/test database with the common seeded accounts used during development:

- `boss@example.com` / `secret123` (admin)
- `pleb@example.com` / `secret123`
- `h1@example.com` / `hirer1` (hirer)
- `w1@example.com` / `worker1` (worker)

## Usage

```bash
# backend running on :8000, DB reachable for helper queries where needed
export BASE_URL=${BASE_URL:-http://localhost:8000}
export DATABASE_URL=${DATABASE_URL:-postgresql://hustle_user:...@localhost:5432/hustle_test}

bash scripts/e2e/admin_moderation.sh
bash scripts/e2e/hiring_pipeline.sh
```

Scripts exit non-zero on failure. Run against a disposable/dev database; they create/modify data.

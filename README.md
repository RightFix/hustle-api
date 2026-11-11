# Hustle backend (Axum)

Rust API for Hustle: Axum + SQLx (Postgres) + argon2 auth + lettre SMTP + utoipa docs.

## Prereqs

- Rust (stable, via rustup)
- Docker + Docker Compose
- (optional) `cargo install sqlx-cli --no-default-features --features postgres`

## Dev run

```bash
docker compose up -d db mail   # Postgres :5432, Mailpit SMTP :1025 + UI :8025
cp .env.example .env           # adjust if needed (or use local dev values)
cargo run                      # migrations run automatically on boot → :8000
```

- API: `http://localhost:8000/api`, health: `/health`
- Swagger UI (admin-gated): `/docs` (and `/api-docs/openapi.json`)
- Dev mail inbox (Mailpit): `http://localhost:8025`

## Environment variables

| Var | Dev default/example | Notes |
| --- | --- | --- |
| `DATABASE_URL` | `postgresql://hustle:hustle@localhost:5432/hustle` | Postgres connection string |
| `SECRET_KEY` | — | **Required in prod.** Long random secret; keep stable (rotation invalidates sessions) |
| `ACCESS_TOKEN_EXPIRE_MINUTES` | `1440` | Cookie `Max-Age` + JWT `exp` (e.g. `10080` = 7 days in prod) |
| `ENVIRONMENT` | `development` | `production` enables cookie `Secure` flag (HTTPS required) |
| `CORS_ORIGIN` | `http://localhost:5173` | Must exactly match frontend origin (credentials/cookies) |
| `FRONTEND_URL` | `http://localhost:5173` | Used to build email links (verify/reset/messages) |
| `SMTP_HOST` / `SMTP_PORT` | `localhost` / `1025` | Mailpit in dev; real provider in prod |
| `SMTP_USER` / `SMTP_PASS` | empty | Empty = no SMTP auth (Mailpit). Set both for authenticated SMTP |
| `SMTP_FROM` | `noreply@hustle.local` | Sender address |
| `UPLOAD_DIR` | `uploads` | Local media storage (avatars/gallery). Persist and back up in prod |

Auth is cookie-only (`HttpOnly; SameSite=Lax` `access_token`). Password reset tokens are opaque, SHA-256-hashed at rest, single-use, 60-minute expiry.

## Docker / production (VPS)

Build and run the API with the prod compose overlay (Postgres from base compose + API container):

```bash
cp .env.example .env.prod      # fill real secrets, origins, SMTP, DATABASE_URL
docker compose -f docker-compose.yml -f docker-compose.prod.yml --env-file .env.prod up -d --build
```

Notes:
- `docker-compose.prod.yml` builds `backend/Dockerfile` (release binary), runs migrations on boot, and mounts a named volume for `/app/uploads`.
- In prod the overlay disables public Postgres port exposure; use a managed Postgres by pointing `DATABASE_URL` at it (and run only the `api` service if desired).
- Set `ENVIRONMENT=production`, HTTPS `CORS_ORIGIN`/`FRONTEND_URL`, and real SMTP credentials. With `Secure` cookies, the app must be served over HTTPS.
- Put a reverse proxy (TLS termination, request limits, timeouts) in front of the API for internet-facing deployments.

### Backups (important)
- **Postgres:** schedule regular dumps/backups (e.g. `pg_dump`) and test restores.
- **Uploads:** back up the uploads volume/path (`UPLOAD_DIR`, avatars/gallery media) alongside the database; DB rows reference files by name.
- Keep `SECRET_KEY` and SMTP credentials in a secret manager or protected env file (never commit `.env`).

## Admin panel (backend repo)

See `admin-panel/README.md` for the standalone React admin dashboard (cookie auth, proxies `/api,/media` to `:8000`).

## E2E smoke scripts

Persisted API smoke scripts live in `scripts/e2e/` (run against a running backend and a disposable/dev DB with seeded users):

```bash
export BASE_URL=http://localhost:8000
bash scripts/e2e/admin_moderation.sh
bash scripts/e2e/hiring_pipeline.sh
```

See `scripts/e2e/README.md` for prerequisites and assumptions. CI runs unit tests, fmt, and clippy (`.github/workflows/ci.yml`).

## Checks

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

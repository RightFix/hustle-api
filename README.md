# Hustle backend (Axum)

Rust API for Hustle: Axum + SQLx (Postgres) + argon2 auth + lettre SMTP + utoipa docs.

## Prereqs

- Rust (via rustup), Docker, `cargo install sqlx-cli --no-default-features --features postgres`

## Run (dev)

```bash
docker compose up -d db mail   # Postgres :5432, Mailpit SMTP :1025 + UI :8025
cargo run                      # migrations run automatically on boot → :8000
```

- API: `http://localhost:8000/api`, health: `/health`, docs (Swagger UI): `/docs`
- Test mail inbox: `http://localhost:8025`

## Env

| Var | Default (dev) | Notes |
| --- | --- | --- |
| `DATABASE_URL` | `postgresql://hustle:hustle@localhost:5432/hustle` | |
| `SECRET_KEY` | — | keep secret; existing JWTs survive rewrites as long as this is stable |
| `ACCESS_TOKEN_EXPIRE_MINUTES` | `1440` | cookie `Max-Age` + JWT `exp` |
| `ENVIRONMENT` | `development` | `production` turns on the cookie `Secure` flag |
| `CORS_ORIGIN` | `http://localhost:5173` | must match the frontend origin (credentials mode) |
| `FRONTEND_URL` | `http://localhost:5173` | used to build password-reset links |
| `SMTP_HOST` / `SMTP_PORT` | `localhost` / `1025` | Mailpit in dev |
| `SMTP_USER` / `SMTP_PASS` | empty | empty = no SMTP auth (Mailpit); set both for prod |
| `SMTP_FROM` | `noreply@hustle.local` | |

Auth is cookie-only: login sets an `HttpOnly; SameSite=Lax` `access_token` cookie.
Forgot-password tokens are opaque, SHA-256-hashed at rest, single-use, 60-minute expiry.

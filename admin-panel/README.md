# Hustle Admin Panel

Standalone React + Vite dashboard (in the backend repo) for Hustle's admin API.

## Covers all admin endpoints
- `GET /api/admin/stats`
- `GET /api/admin/users` (search `q`, filter `status=active|suspended`, pagination)
- `GET /api/admin/users/{id}`
- `PATCH /api/admin/users/{id}` (suspend/unsuspend + reason, verify/unverify, promote/demote admin)
- `DELETE /api/admin/users/{id}`
- `GET /api/admin/posts` / `DELETE /api/admin/posts/{id}`
- `GET /api/admin/gigs` / `DELETE /api/admin/gigs/{id}`
- `GET /api/admin/reports` (filter `status`, `target_type`, pagination)
- `PATCH /api/admin/reports/{id}` (resolve/dismiss + admin note, optional quick actions: `suspend_user` / `delete_user` for user targets, `delete_target` for post/gig targets)
- `GET /api/admin/audit` (admin action audit log, pagination, filters: `action`, `target_type`, `actor_id`)

Auth uses the existing cookie session: login via `POST /api/auth/login`, session checked with `GET /api/users/me` (must be `is_admin`).

## Run (dev)
1. Start the backend on `:8000` (e.g. `cd backend && cargo run`).
2. From `backend/admin-panel/`:
   ```bash
   npm install
   npm run dev
   ```
3. Open http://localhost:5174 and sign in with an admin account.

Vite proxies `/api` and `/media` to `http://localhost:8000` (see `vite.config.ts`).

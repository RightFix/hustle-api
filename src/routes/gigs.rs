use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde::Deserialize;
use sqlx::FromRow;
use utoipa::IntoParams;

use crate::{
    auth::CurrentUser,
    error::{AppError, AppResult},
    models::{
        ApplicantOut, ApplicantRow, ApplyGigRequest, CreateGigRequest, CreateRatingRequest, GigOut,
        Message, PaginatedGigs, RatingOut, RatingRow, RatingSummary, UpdateGigStatusRequest,
        UserRatingsOut,
    },
    AppState,
};

const COLLARS: [&str; 4] = ["pink", "gray", "blue", "green"];

/// Viewer-aware gig row.
#[derive(Debug, FromRow)]
struct GigViewRow {
    id: i32,
    poster_id: i32,
    title: String,
    description: String,
    collar: String,
    location: Option<String>,
    pay: Option<String>,
    status: String,
    created_at: chrono::DateTime<chrono::Utc>,
    hired_worker_id: Option<i32>,
    applicant_count: i64,
    my_application_status: Option<String>,
    saved: bool,
}

impl GigViewRow {
    fn into_out(self) -> GigOut {
        GigOut {
            id: self.id,
            poster_id: self.poster_id,
            title: self.title,
            description: self.description,
            collar: self.collar,
            location: self.location,
            pay: self.pay,
            status: self.status,
            created_at: self.created_at,
            hired_worker_id: self.hired_worker_id,
            applied: Some(self.my_application_status.is_some()),
            my_application_status: self.my_application_status,
            saved: Some(self.saved),
            applicant_count: self.applicant_count,
        }
    }
}

const GIG_COLUMNS: &str = "g.id, g.poster_id, g.title, g.description, g.collar, \
     g.location, g.pay, g.status, g.created_at, g.hired_worker_id, \
     (SELECT COUNT(*) FROM gig_applications a WHERE a.gig_id = g.id) AS applicant_count, \
     (SELECT a.status FROM gig_applications a WHERE a.gig_id = g.id AND a.worker_id = $1) AS my_application_status, \
     EXISTS (SELECT 1 FROM saved_gigs s WHERE s.gig_id = g.id AND s.user_id = $1) AS saved";

fn validate_collar(collar: &str) -> AppResult<()> {
    if COLLARS.contains(&collar) {
        Ok(())
    } else {
        Err(AppError::Unprocessable("Invalid collar type".into()))
    }
}

async fn fetch_gig(state: &AppState, viewer_id: i32, gig_id: i32) -> AppResult<Option<GigOut>> {
    let row = sqlx::query_as::<_, GigViewRow>(&format!(
        "SELECT {GIG_COLUMNS} FROM gigs g WHERE g.id = $2"
    ))
    .bind(viewer_id)
    .bind(gig_id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(row.map(GigViewRow::into_out))
}

#[derive(Debug, FromRow)]
struct GigMeta {
    poster_id: i32,
    status: String,
    hired_worker_id: Option<i32>,
}

async fn gig_meta(state: &AppState, gig_id: i32) -> AppResult<GigMeta> {
    sqlx::query_as::<_, GigMeta>(
        "SELECT poster_id, status, hired_worker_id FROM gigs WHERE id = $1",
    )
    .bind(gig_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Gig not found".into()))
}

#[utoipa::path(
    post,
    path = "/api/gigs",
    tag = "gigs",
    security(("cookie_auth" = [])),
    request_body = CreateGigRequest,
    responses(
        (status = 201, description = "Gig posted", body = GigOut),
        (status = 403, description = "Only hirer profiles can post gigs", body = Message),
        (status = 422, description = "Invalid collar type", body = Message),
    )
)]
pub async fn create_gig(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentUser(current): CurrentUser,
    Json(payload): Json<CreateGigRequest>,
) -> AppResult<(StatusCode, Json<GigOut>)> {
    if !crate::rate_limit::check_ip(&headers, "gig", 20, 300) {
        return Err(AppError::TooManyRequests(
            "Too many requests, please try again later.".into(),
        ));
    }
    if current.account_type != "hirer" {
        return Err(AppError::Forbidden(
            "Switch to a hirer profile to post gigs".into(),
        ));
    }
    let title = payload.title.trim().to_owned();
    let description = payload.description.trim().to_owned();
    if title.is_empty() {
        return Err(AppError::BadRequest("Title cannot be empty".into()));
    }
    if description.is_empty() {
        return Err(AppError::BadRequest("Description cannot be empty".into()));
    }
    validate_collar(&payload.collar)?;
    let location = payload
        .location
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty());
    let pay = payload
        .pay
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty());

    let id = sqlx::query_scalar::<_, i32>(
        "INSERT INTO gigs (poster_id, title, description, collar, location, pay) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(current.id)
    .bind(&title)
    .bind(&description)
    .bind(&payload.collar)
    .bind(&location)
    .bind(&pay)
    .fetch_one(&state.pool)
    .await?;

    let gig = fetch_gig(&state, current.id, id)
        .await?
        .ok_or(AppError::Internal)?;
    Ok((StatusCode::CREATED, Json(gig)))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct GigListQuery {
    pub collar: Option<String>,
    pub q: Option<String>,
    pub location: Option<String>,
    /// open | in_progress | completed | closed | all (default: open)
    pub status: Option<String>,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

fn default_page() -> u32 {
    1
}
fn default_per_page() -> u32 {
    20
}

#[utoipa::path(
    get,
    path = "/api/gigs",
    tag = "gigs",
    security(("cookie_auth" = [])),
    params(GigListQuery),
    responses((status = 200, description = "Browse gigs, newest first", body = PaginatedGigs))
)]
pub async fn list_gigs(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Query(query): Query<GigListQuery>,
) -> AppResult<Json<PaginatedGigs>> {
    if let Some(collar) = &query.collar {
        validate_collar(collar)?;
    }
    let status_filter = query.status.as_deref().unwrap_or("open").to_owned();
    if !matches!(
        status_filter.as_str(),
        "open" | "in_progress" | "completed" | "closed" | "all"
    ) {
        return Err(AppError::BadRequest("Invalid status filter".into()));
    }
    let pattern = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| format!("%{t}%"));
    let location = query
        .location
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| format!("%{t}%"));
    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);
    let offset = (page - 1) as i64 * per_page as i64;

    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM gigs g WHERE ($1 = 'all' OR g.status = $1) \
         AND ($2::text IS NULL OR g.collar = $2) \
         AND ($3::text IS NULL OR g.title ILIKE $3 OR g.description ILIKE $3) \
         AND ($4::text IS NULL OR g.location ILIKE $4)",
    )
    .bind(&status_filter)
    .bind(&query.collar)
    .bind(&pattern)
    .bind(&location)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query_as::<_, GigViewRow>(&format!(
        "SELECT {GIG_COLUMNS} FROM gigs g \
         WHERE ($2 = 'all' OR g.status = $2) \
         AND ($3::text IS NULL OR g.collar = $3) \
         AND ($4::text IS NULL OR g.title ILIKE $4 OR g.description ILIKE $4) \
         AND ($5::text IS NULL OR g.location ILIKE $5) \
         ORDER BY g.created_at DESC LIMIT $6 OFFSET $7"
    ))
    .bind(current.id)
    .bind(&status_filter)
    .bind(&query.collar)
    .bind(&pattern)
    .bind(&location)
    .bind(per_page as i64)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(PaginatedGigs {
        items: rows.into_iter().map(GigViewRow::into_out).collect(),
        total,
        page,
        per_page,
    }))
}

#[utoipa::path(
    get, path = "/api/gigs/{gig_id}", tag = "gigs", security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    responses((status = 200, description = "Gig detail", body = GigOut))
)]
pub async fn read_gig(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(gig_id): Path<i32>,
) -> AppResult<Json<GigOut>> {
    let gig = fetch_gig(&state, current.id, gig_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Gig not found".into()))?;
    Ok(Json(gig))
}

#[utoipa::path(
    patch, path = "/api/gigs/{gig_id}", tag = "gigs", security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    request_body = UpdateGigStatusRequest,
    responses((status = 200, description = "Gig updated", body = GigOut))
)]
pub async fn update_gig(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(gig_id): Path<i32>,
    Json(payload): Json<UpdateGigStatusRequest>,
) -> AppResult<Json<GigOut>> {
    let meta = gig_meta(&state, gig_id).await?;
    if meta.poster_id != current.id {
        return Err(AppError::Forbidden("Not allowed to edit this gig".into()));
    }
    // Poster may manually close/reopen only when not in the hiring/completion flow.
    if !matches!(payload.status.as_str(), "open" | "closed") {
        return Err(AppError::Unprocessable(
            "Status can only be set to open or closed here; use accept/complete for hiring flow"
                .into(),
        ));
    }
    if matches!(meta.status.as_str(), "in_progress" | "completed") {
        return Err(AppError::BadRequest(
            "Cannot manually change status of an in-progress or completed gig".into(),
        ));
    }
    sqlx::query("UPDATE gigs SET status = $1, hired_worker_id = NULL WHERE id = $2")
        .bind(&payload.status)
        .bind(gig_id)
        .execute(&state.pool)
        .await?;
    if payload.status == "open" {
        // reopening clears prior application outcomes so workers can re-apply/be reconsidered
        sqlx::query("UPDATE gig_applications SET status = 'pending' WHERE gig_id = $1")
            .bind(gig_id)
            .execute(&state.pool)
            .await?;
    }
    let gig = fetch_gig(&state, current.id, gig_id)
        .await?
        .ok_or(AppError::Internal)?;
    Ok(Json(gig))
}

#[utoipa::path(
    delete, path = "/api/gigs/{gig_id}", tag = "gigs", security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    responses((status = 204, description = "Gig deleted"))
)]
pub async fn delete_gig(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(gig_id): Path<i32>,
) -> AppResult<StatusCode> {
    let meta = gig_meta(&state, gig_id).await?;
    if meta.poster_id != current.id {
        return Err(AppError::Forbidden("Not allowed to delete this gig".into()));
    }
    sqlx::query("DELETE FROM gigs WHERE id = $1")
        .bind(gig_id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post, path = "/api/gigs/{gig_id}/apply", tag = "gigs", security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    request_body = ApplyGigRequest,
    responses(
        (status = 201, description = "Application sent", body = Message),
        (status = 409, description = "Already applied", body = Message),
    )
)]
pub async fn apply_gig(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentUser(current): CurrentUser,
    Path(gig_id): Path<i32>,
    Json(payload): Json<ApplyGigRequest>,
) -> AppResult<(StatusCode, Json<Message>)> {
    if !crate::rate_limit::check_ip(&headers, "interact", 40, 60) {
        return Err(AppError::TooManyRequests(
            "Too many requests, please try again later.".into(),
        ));
    }
    if current.account_type != "worker" {
        return Err(AppError::Forbidden(
            "Switch to a worker profile to apply for gigs".into(),
        ));
    }
    let meta = gig_meta(&state, gig_id).await?;
    if meta.status != "open" {
        return Err(AppError::BadRequest(
            "This gig is not accepting applications".into(),
        ));
    }
    if meta.poster_id == current.id {
        return Err(AppError::BadRequest(
            "You cannot apply to your own gig".into(),
        ));
    }
    if crate::routes::blocks::is_blocked_either(&state.pool, current.id, meta.poster_id).await? {
        return Err(AppError::Forbidden("Cannot interact with this user".into()));
    }
    let message = payload
        .message
        .map(|m| m.trim().to_owned())
        .filter(|m| !m.is_empty());
    if let Some(m) = &message {
        if m.len() > 500 {
            return Err(AppError::Unprocessable(
                "Message must be 500 characters or fewer".into(),
            ));
        }
    }

    // Re-apply allowed if previously rejected (reopens application as pending).
    let result = sqlx::query(
        "INSERT INTO gig_applications (gig_id, worker_id, message, status) VALUES ($1, $2, $3, 'pending') \
         ON CONFLICT (gig_id, worker_id) DO UPDATE \
           SET message = EXCLUDED.message, status = 'pending', created_at = now() \
         WHERE gig_applications.status = 'rejected'",
    )
    .bind(gig_id)
    .bind(current.id)
    .bind(&message)
    .execute(&state.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::Conflict("Already applied to this gig".into()));
    }

    crate::notify::notify(
        &state,
        meta.poster_id,
        current.id,
        "gig_apply",
        Some("gig"),
        Some(gig_id),
    )
    .await;
    Ok((StatusCode::CREATED, Json(Message::new("Application sent."))))
}

#[utoipa::path(
    delete, path = "/api/gigs/{gig_id}/apply", tag = "gigs", security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    responses((status = 204, description = "Application withdrawn"))
)]
pub async fn withdraw_gig(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(gig_id): Path<i32>,
) -> AppResult<StatusCode> {
    sqlx::query(
        "DELETE FROM gig_applications WHERE gig_id = $1 AND worker_id = $2 AND status = 'pending'",
    )
    .bind(gig_id)
    .bind(current.id)
    .execute(&state.pool)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

// --- save/unsave/saved (unchanged behavior) ---

#[utoipa::path(
    post, path = "/api/gigs/{gig_id}/save", tag = "gigs", security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    responses((status = 204, description = "Gig saved"))
)]
pub async fn save_gig(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(gig_id): Path<i32>,
) -> AppResult<StatusCode> {
    let exists = sqlx::query_scalar::<_, i32>("SELECT id FROM gigs WHERE id = $1")
        .bind(gig_id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("Gig not found".into()));
    }
    sqlx::query("INSERT INTO saved_gigs (user_id, gig_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(current.id)
        .bind(gig_id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete, path = "/api/gigs/{gig_id}/save", tag = "gigs", security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    responses((status = 204, description = "Gig removed from saved list"))
)]
pub async fn unsave_gig(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(gig_id): Path<i32>,
) -> AppResult<StatusCode> {
    let exists = sqlx::query_scalar::<_, i32>("SELECT id FROM gigs WHERE id = $1")
        .bind(gig_id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("Gig not found".into()));
    }
    sqlx::query("DELETE FROM saved_gigs WHERE user_id = $1 AND gig_id = $2")
        .bind(current.id)
        .bind(gig_id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct SavedGigsQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

#[utoipa::path(
    get, path = "/api/gigs/saved", tag = "gigs", security(("cookie_auth" = [])),
    params(SavedGigsQuery),
    responses((status = 200, description = "Saved gigs, newest save first", body = PaginatedGigs))
)]
pub async fn saved_gigs(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Query(query): Query<SavedGigsQuery>,
) -> AppResult<Json<PaginatedGigs>> {
    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);
    let offset = (page - 1) as i64 * per_page as i64;
    let total = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM saved_gigs WHERE user_id = $1")
        .bind(current.id)
        .fetch_one(&state.pool)
        .await?;
    let rows = sqlx::query_as::<_, GigViewRow>(&format!(
        "SELECT {GIG_COLUMNS} FROM gigs g \
         INNER JOIN saved_gigs sg ON sg.gig_id = g.id AND sg.user_id = $1 \
         ORDER BY sg.created_at DESC LIMIT $2 OFFSET $3"
    ))
    .bind(current.id)
    .bind(per_page as i64)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(PaginatedGigs {
        items: rows.into_iter().map(GigViewRow::into_out).collect(),
        total,
        page,
        per_page,
    }))
}

#[utoipa::path(
    get, path = "/api/gigs/{gig_id}/applicants", tag = "gigs", security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    responses((status = 200, description = "Applicants for this gig", body = Vec<ApplicantOut>))
)]
pub async fn list_applicants(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(gig_id): Path<i32>,
) -> AppResult<Json<Vec<ApplicantOut>>> {
    let meta = gig_meta(&state, gig_id).await?;
    if meta.poster_id != current.id {
        return Err(AppError::Forbidden("Not allowed to view applicants".into()));
    }
    let rows = sqlx::query_as::<_, ApplicantRow>(
        "SELECT u.id, u.username, u.bio, u.avatar_file, a.message, a.created_at AS applied_at, a.status \
         FROM gig_applications a JOIN users u ON u.id = a.worker_id \
         WHERE a.gig_id = $1 \
           AND NOT EXISTS ( \
             SELECT 1 FROM user_blocks b \
             WHERE (b.blocker_id = $2 AND b.blocked_id = u.id) \
                OR (b.blocker_id = u.id AND b.blocked_id = $2) \
           ) \
         ORDER BY CASE a.status WHEN 'accepted' THEN 0 WHEN 'pending' THEN 1 ELSE 2 END, a.created_at DESC",
    )
    .bind(gig_id)
    .bind(current.id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| ApplicantOut {
                id: r.id,
                username: r.username,
                bio: r.bio,
                avatar_url: r.avatar_file.map(|f| format!("/media/avatars/{f}")),
                message: r.message,
                applied_at: r.applied_at,
                status: r.status,
            })
            .collect(),
    ))
}

// --- Round 4: hiring & completion ---

#[utoipa::path(
    post, path = "/api/gigs/{gig_id}/applicants/{worker_id}/accept", tag = "gigs", security(("cookie_auth" = [])),
    params(
        ("gig_id" = i32, Path, description = "Gig id"),
        ("worker_id" = i32, Path, description = "Worker to hire"),
    ),
    responses((status = 200, description = "Applicant accepted, gig in progress", body = GigOut))
)]
pub async fn accept_applicant(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentUser(current): CurrentUser,
    Path((gig_id, worker_id)): Path<(i32, i32)>,
) -> AppResult<Json<GigOut>> {
    if !crate::rate_limit::check_ip(&headers, "interact", 40, 60) {
        return Err(AppError::TooManyRequests(
            "Too many requests, please try again later.".into(),
        ));
    }
    let meta = gig_meta(&state, gig_id).await?;
    if meta.poster_id != current.id {
        return Err(AppError::Forbidden("Not allowed to manage this gig".into()));
    }
    if meta.status != "open" {
        return Err(AppError::BadRequest(
            "Only open gigs can accept an applicant".into(),
        ));
    }
    if crate::routes::blocks::is_blocked_either(&state.pool, current.id, worker_id).await? {
        return Err(AppError::Forbidden("Cannot interact with this user".into()));
    }
    let app = sqlx::query_scalar::<_, String>(
        "SELECT status FROM gig_applications WHERE gig_id = $1 AND worker_id = $2",
    )
    .bind(gig_id)
    .bind(worker_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Application not found".into()))?;
    if app != "pending" {
        return Err(AppError::Conflict("Application is not pending".into()));
    }

    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE gigs SET status = 'in_progress', hired_worker_id = $1 WHERE id = $2")
        .bind(worker_id)
        .bind(gig_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE gig_applications SET status = 'accepted' WHERE gig_id = $1 AND worker_id = $2",
    )
    .bind(gig_id)
    .bind(worker_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE gig_applications SET status = 'rejected' WHERE gig_id = $1 AND worker_id <> $2 AND status = 'pending'")
        .bind(gig_id).bind(worker_id).execute(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO gig_completions (gig_id, worker_id) VALUES ($1, $2) \
         ON CONFLICT (gig_id) DO UPDATE SET worker_id = EXCLUDED.worker_id, poster_confirmed_at = NULL, worker_confirmed_at = NULL",
    )
    .bind(gig_id).bind(worker_id).execute(&mut *tx).await?;
    tx.commit().await?;

    crate::notify::notify(
        &state,
        worker_id,
        current.id,
        "gig_accept",
        Some("gig"),
        Some(gig_id),
    )
    .await;
    // notify rejected applicants
    let rejected: Vec<i32> = sqlx::query_scalar(
        "SELECT worker_id FROM gig_applications WHERE gig_id = $1 AND status = 'rejected'",
    )
    .bind(gig_id)
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();
    for uid in rejected {
        crate::notify::notify(
            &state,
            uid,
            current.id,
            "gig_reject",
            Some("gig"),
            Some(gig_id),
        )
        .await;
    }

    let gig = fetch_gig(&state, current.id, gig_id)
        .await?
        .ok_or(AppError::Internal)?;
    Ok(Json(gig))
}

#[utoipa::path(
    post, path = "/api/gigs/{gig_id}/applicants/{worker_id}/reject", tag = "gigs", security(("cookie_auth" = [])),
    params(
        ("gig_id" = i32, Path, description = "Gig id"),
        ("worker_id" = i32, Path, description = "Worker to reject"),
    ),
    responses((status = 200, description = "Applicant rejected", body = Message))
)]
pub async fn reject_applicant(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentUser(current): CurrentUser,
    Path((gig_id, worker_id)): Path<(i32, i32)>,
) -> AppResult<Json<Message>> {
    if !crate::rate_limit::check_ip(&headers, "interact", 40, 60) {
        return Err(AppError::TooManyRequests(
            "Too many requests, please try again later.".into(),
        ));
    }
    let meta = gig_meta(&state, gig_id).await?;
    if meta.poster_id != current.id {
        return Err(AppError::Forbidden("Not allowed to manage this gig".into()));
    }
    let res = sqlx::query(
        "UPDATE gig_applications SET status = 'rejected' \
         WHERE gig_id = $1 AND worker_id = $2 AND status = 'pending'",
    )
    .bind(gig_id)
    .bind(worker_id)
    .execute(&state.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::Conflict(
            "No pending application to reject".into(),
        ));
    }
    crate::notify::notify(
        &state,
        worker_id,
        current.id,
        "gig_reject",
        Some("gig"),
        Some(gig_id),
    )
    .await;
    Ok(Json(Message::new("Applicant rejected.")))
}

#[utoipa::path(
    post, path = "/api/gigs/{gig_id}/complete", tag = "gigs", security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    responses((status = 200, description = "Completion confirmed (gig completes when both confirm)", body = GigOut))
)]
pub async fn complete_gig(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentUser(current): CurrentUser,
    Path(gig_id): Path<i32>,
) -> AppResult<Json<GigOut>> {
    if !crate::rate_limit::check_ip(&headers, "interact", 40, 60) {
        return Err(AppError::TooManyRequests(
            "Too many requests, please try again later.".into(),
        ));
    }
    let meta = gig_meta(&state, gig_id).await?;
    if meta.status != "in_progress" {
        return Err(AppError::BadRequest(
            "Only in-progress gigs can be completed".into(),
        ));
    }
    let hired = meta
        .hired_worker_id
        .ok_or_else(|| AppError::BadRequest("No hired worker for this gig".into()))?;
    let (is_poster, is_worker) = (current.id == meta.poster_id, current.id == hired);
    if !is_poster && !is_worker {
        return Err(AppError::Forbidden(
            "Only the poster or hired worker can confirm completion".into(),
        ));
    }

    // ensure row exists
    sqlx::query(
        "INSERT INTO gig_completions (gig_id, worker_id) VALUES ($1, $2) ON CONFLICT (gig_id) DO NOTHING",
    )
    .bind(gig_id).bind(hired).execute(&state.pool).await?;

    if is_poster {
        sqlx::query("UPDATE gig_completions SET poster_confirmed_at = COALESCE(poster_confirmed_at, now()) WHERE gig_id = $1")
            .bind(gig_id).execute(&state.pool).await?;
    } else {
        sqlx::query("UPDATE gig_completions SET worker_confirmed_at = COALESCE(worker_confirmed_at, now()) WHERE gig_id = $1")
            .bind(gig_id).execute(&state.pool).await?;
    }

    let both: bool = sqlx::query_scalar(
        "SELECT poster_confirmed_at IS NOT NULL AND worker_confirmed_at IS NOT NULL FROM gig_completions WHERE gig_id = $1",
    )
    .bind(gig_id).fetch_one(&state.pool).await.unwrap_or(false);

    let other = if is_poster { hired } else { meta.poster_id };
    if both {
        sqlx::query("UPDATE gigs SET status = 'completed' WHERE id = $1")
            .bind(gig_id)
            .execute(&state.pool)
            .await?;
        // notify both parties completion finalized (other already knows; notify rater prompts context)
        crate::notify::notify(
            &state,
            other,
            current.id,
            "gig_complete",
            Some("gig"),
            Some(gig_id),
        )
        .await;
        crate::notify::notify(
            &state,
            current.id,
            other,
            "gig_complete",
            Some("gig"),
            Some(gig_id),
        )
        .await;
    } else {
        crate::notify::notify(
            &state,
            other,
            current.id,
            "gig_complete",
            Some("gig"),
            Some(gig_id),
        )
        .await;
    }

    let gig = fetch_gig(&state, current.id, gig_id)
        .await?
        .ok_or(AppError::Internal)?;
    Ok(Json(gig))
}

#[utoipa::path(
    post, path = "/api/gigs/{gig_id}/rating", tag = "gigs", security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    request_body = CreateRatingRequest,
    responses(
        (status = 201, description = "Rating submitted", body = RatingOut),
        (status = 409, description = "Already rated", body = Message),
    )
)]
pub async fn rate_gig(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentUser(current): CurrentUser,
    Path(gig_id): Path<i32>,
    Json(payload): Json<CreateRatingRequest>,
) -> AppResult<(StatusCode, Json<RatingOut>)> {
    if !crate::rate_limit::check_ip(&headers, "interact", 40, 60) {
        return Err(AppError::TooManyRequests(
            "Too many requests, please try again later.".into(),
        ));
    }
    if !(1..=5).contains(&payload.rating) {
        return Err(AppError::Unprocessable(
            "Rating must be between 1 and 5".into(),
        ));
    }
    let comment = payload
        .comment
        .map(|c| c.trim().to_owned())
        .filter(|c| !c.is_empty())
        .map(|c| c.chars().take(1000).collect::<String>());
    let meta = gig_meta(&state, gig_id).await?;
    if meta.status != "completed" {
        return Err(AppError::BadRequest(
            "Ratings are only allowed after the gig is completed".into(),
        ));
    }
    let hired = meta
        .hired_worker_id
        .ok_or_else(|| AppError::BadRequest("No hired worker for this gig".into()))?;
    let ratee = if current.id == meta.poster_id {
        hired
    } else if current.id == hired {
        meta.poster_id
    } else {
        return Err(AppError::Forbidden(
            "Only the poster or hired worker can rate this gig".into(),
        ));
    };

    let row = sqlx::query_as::<_, RatingRow>(
        "WITH ins AS ( \
           INSERT INTO gig_ratings (gig_id, rater_id, ratee_id, rating, comment) \
           VALUES ($1, $2, $3, $4, $5) RETURNING * \
         ) SELECT ins.id, ins.gig_id, ins.rater_id, u.username AS rater_username, u.avatar_file AS rater_avatar_file, \
                  ins.ratee_id, ins.rating, ins.comment, ins.created_at \
           FROM ins JOIN users u ON u.id = ins.rater_id",
    )
    .bind(gig_id)
    .bind(current.id)
    .bind(ratee)
    .bind(payload.rating)
    .bind(&comment)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let Some(db) = e.as_database_error() {
            if db.code().as_deref() == Some("23505") {
                return AppError::Conflict("You have already rated this gig".into());
            }
        }
        AppError::from(e)
    })?;

    crate::notify::notify(
        &state,
        ratee,
        current.id,
        "gig_rating",
        Some("gig"),
        Some(gig_id),
    )
    .await;
    Ok((StatusCode::CREATED, Json(RatingOut::from(row))))
}

#[utoipa::path(
    get, path = "/api/users/{user_id}/ratings", tag = "users", security(("cookie_auth" = [])),
    params(("user_id" = i32, Path, description = "User id")),
    responses((status = 200, description = "Ratings received by user", body = UserRatingsOut))
)]
pub async fn user_ratings(
    State(state): State<AppState>,
    _current: CurrentUser,
    Path(user_id): Path<i32>,
) -> AppResult<Json<UserRatingsOut>> {
    let exists = sqlx::query_scalar::<_, i32>("SELECT id FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("User not found".into()));
    }

    let summary = sqlx::query_as::<_, (Option<f64>, i64)>(
        "SELECT AVG(rating)::float8, COUNT(*) FROM gig_ratings WHERE ratee_id = $1",
    )
    .bind(user_id)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query_as::<_, RatingRow>(
        "SELECT r.id, r.gig_id, r.rater_id, u.username AS rater_username, u.avatar_file AS rater_avatar_file, \
                r.ratee_id, r.rating, r.comment, r.created_at \
         FROM gig_ratings r JOIN users u ON u.id = r.rater_id \
         WHERE r.ratee_id = $1 ORDER BY r.created_at DESC LIMIT 50",
    )
    .bind(user_id).fetch_all(&state.pool).await?;

    Ok(Json(UserRatingsOut {
        summary: RatingSummary {
            average: summary.0,
            count: summary.1,
        },
        items: rows.into_iter().map(RatingOut::from).collect(),
    }))
}

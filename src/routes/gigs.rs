use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use sqlx::FromRow;
use utoipa::IntoParams;

use crate::{
    auth::CurrentUser,
    error::{AppError, AppResult},
    models::{
        ApplicantOut, ApplicantRow, ApplyGigRequest, CreateGigRequest, GigOut,
        PaginatedGigs, UpdateGigStatusRequest,
    },
    AppState,
};

const COLLARS: [&str; 4] = ["pink", "gray", "blue", "green"];

/// Viewer-aware gig row: includes applicant count + whether the viewer applied.
/// Bind order in every query using GIG_SELECT: $1 = viewer id, $2 = gig id.
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
    applicant_count: i64,
    applied: bool,
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
            applied: Some(self.applied),
            saved: Some(self.saved),
            applicant_count: self.applicant_count,
        }
    }
}

const GIG_COLUMNS: &str = "g.id, g.poster_id, g.title, g.description, g.collar, \
     g.location, g.pay, g.status, g.created_at, \
     (SELECT COUNT(*) FROM gig_applications a WHERE a.gig_id = g.id) AS applicant_count, \
     EXISTS (SELECT 1 FROM gig_applications a WHERE a.gig_id = g.id AND a.worker_id = $1) AS applied, \
     EXISTS (SELECT 1 FROM saved_gigs s WHERE s.gig_id = g.id AND s.user_id = $1) AS saved";

fn validate_collar(collar: &str) -> AppResult<()> {
    if COLLARS.contains(&collar) {
        Ok(())
    } else {
        Err(AppError::Unprocessable("Invalid collar type".into()))
    }
}

async fn fetch_gig(
    state: &AppState,
    viewer_id: i32,
    gig_id: i32,
) -> AppResult<Option<GigOut>> {
    let row = sqlx::query_as::<_, GigViewRow>(&format!(
        "SELECT {GIG_COLUMNS} FROM gigs g WHERE g.id = $2"
    ))
    .bind(viewer_id)
    .bind(gig_id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(row.map(GigViewRow::into_out))
}

async fn poster_of(state: &AppState, gig_id: i32) -> AppResult<i32> {
    sqlx::query_scalar::<_, i32>("SELECT poster_id FROM gigs WHERE id = $1")
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
        (status = 403, description = "Only hirer profiles can post gigs", body = crate::models::Message),
        (status = 422, description = "Invalid collar type", body = crate::models::Message),
    )
)]
pub async fn create_gig(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Json(payload): Json<CreateGigRequest>,
) -> AppResult<(StatusCode, Json<GigOut>)> {
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
    /// pink | gray | blue | green
    pub collar: Option<String>,
    pub q: Option<String>,
    /// Case-insensitive substring match on location.
    pub location: Option<String>,
    /// open (default) | closed | all
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
    responses(
        (status = 200, description = "Browse gigs, newest first", body = PaginatedGigs),
        (status = 401, description = "Not authenticated", body = crate::models::Message),
    )
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
    if !matches!(status_filter.as_str(), "open" | "closed" | "all") {
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
    get,
    path = "/api/gigs/{gig_id}",
    tag = "gigs",
    security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    responses(
        (status = 200, description = "Gig detail", body = GigOut),
        (status = 404, description = "Gig not found", body = crate::models::Message),
    )
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
    patch,
    path = "/api/gigs/{gig_id}",
    tag = "gigs",
    security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    request_body = UpdateGigStatusRequest,
    responses(
        (status = 200, description = "Gig updated", body = GigOut),
        (status = 403, description = "Not the poster", body = crate::models::Message),
        (status = 404, description = "Gig not found", body = crate::models::Message),
    )
)]
pub async fn update_gig(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(gig_id): Path<i32>,
    Json(payload): Json<UpdateGigStatusRequest>,
) -> AppResult<Json<GigOut>> {
    if poster_of(&state, gig_id).await? != current.id {
        return Err(AppError::Forbidden("Not allowed to edit this gig".into()));
    }
    if !matches!(payload.status.as_str(), "open" | "closed") {
        return Err(AppError::Unprocessable("Invalid status".into()));
    }
    sqlx::query("UPDATE gigs SET status = $1 WHERE id = $2")
        .bind(&payload.status)
        .bind(gig_id)
        .execute(&state.pool)
        .await?;
    let gig = fetch_gig(&state, current.id, gig_id)
        .await?
        .ok_or(AppError::Internal)?;
    Ok(Json(gig))
}

#[utoipa::path(
    delete,
    path = "/api/gigs/{gig_id}",
    tag = "gigs",
    security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    responses(
        (status = 204, description = "Gig deleted"),
        (status = 403, description = "Not the poster", body = crate::models::Message),
        (status = 404, description = "Gig not found", body = crate::models::Message),
    )
)]
pub async fn delete_gig(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(gig_id): Path<i32>,
) -> AppResult<StatusCode> {
    if poster_of(&state, gig_id).await? != current.id {
        return Err(AppError::Forbidden("Not allowed to delete this gig".into()));
    }
    sqlx::query("DELETE FROM gigs WHERE id = $1")
        .bind(gig_id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/gigs/{gig_id}/apply",
    tag = "gigs",
    security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    request_body = ApplyGigRequest,
    responses(
        (status = 201, description = "Application sent", body = crate::models::Message),
        (status = 400, description = "Gig is closed", body = crate::models::Message),
        (status = 403, description = "Only worker profiles can apply", body = crate::models::Message),
        (status = 404, description = "Gig not found", body = crate::models::Message),
        (status = 409, description = "Already applied", body = crate::models::Message),
    )
)]
pub async fn apply_gig(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(gig_id): Path<i32>,
    Json(payload): Json<ApplyGigRequest>,
) -> AppResult<(StatusCode, Json<crate::models::Message>)> {
    if current.account_type != "worker" {
        return Err(AppError::Forbidden(
            "Switch to a worker profile to apply for gigs".into(),
        ));
    }
    let (status, poster_id) = sqlx::query_as::<_, (String, i32)>(
        "SELECT status, poster_id FROM gigs WHERE id = $1",
    )
    .bind(gig_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Gig not found".into()))?;
    if status != "open" {
        return Err(AppError::BadRequest("This gig is closed".into()));
    }
    if crate::routes::blocks::is_blocked_either(&state.pool, current.id, poster_id).await? {
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
    let inserted = sqlx::query(
        "INSERT INTO gig_applications (gig_id, worker_id, message) VALUES ($1, $2, $3) \
         ON CONFLICT DO NOTHING",
    )
    .bind(gig_id)
    .bind(current.id)
    .bind(&message)
    .execute(&state.pool)
    .await?;
    if inserted.rows_affected() == 0 {
        return Err(AppError::Conflict("Already applied to this gig".into()));
    }
    crate::notify::notify(
        &state,
        poster_id,
        current.id,
        "gig_apply",
        Some("gig"),
        Some(gig_id),
    )
    .await;
    Ok((
        StatusCode::CREATED,
        Json(crate::models::Message::new("Application sent.")),
    ))
}

#[utoipa::path(
    delete,
    path = "/api/gigs/{gig_id}/apply",
    tag = "gigs",
    security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    responses((status = 204, description = "Application withdrawn")),
)]
pub async fn withdraw_gig(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(gig_id): Path<i32>,
) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM gig_applications WHERE gig_id = $1 AND worker_id = $2")
        .bind(gig_id)
        .bind(current.id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/gigs/{gig_id}/save",
    tag = "gigs",
    security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    responses(
        (status = 204, description = "Gig saved"),
        (status = 404, description = "Gig not found", body = crate::models::Message),
    )
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
    sqlx::query(
        "INSERT INTO saved_gigs (user_id, gig_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(current.id)
    .bind(gig_id)
    .execute(&state.pool)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/gigs/{gig_id}/save",
    tag = "gigs",
    security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    responses(
        (status = 204, description = "Gig removed from saved list"),
        (status = 404, description = "Gig not found", body = crate::models::Message),
    )
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
    /// 1-based page number.
    #[serde(default = "default_page")]
    pub page: u32,
    /// Items per page (max 100).
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

#[utoipa::path(
    get,
    path = "/api/gigs/saved",
    tag = "gigs",
    security(("cookie_auth" = [])),
    params(SavedGigsQuery),
    responses(
        (status = 200, description = "Saved gigs, newest save first", body = PaginatedGigs),
        (status = 401, description = "Not authenticated", body = crate::models::Message),
    )
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
         ORDER BY sg.created_at DESC \
         LIMIT $2 OFFSET $3"
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
    get,
    path = "/api/gigs/{gig_id}/applicants",
    tag = "gigs",
    security(("cookie_auth" = [])),
    params(("gig_id" = i32, Path, description = "Gig id")),
    responses(
        (status = 200, description = "Applicants for this gig", body = Vec<ApplicantOut>),
        (status = 403, description = "Not the poster", body = crate::models::Message),
        (status = 404, description = "Gig not found", body = crate::models::Message),
    )
)]
pub async fn list_applicants(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(gig_id): Path<i32>,
) -> AppResult<Json<Vec<ApplicantOut>>> {
    if poster_of(&state, gig_id).await? != current.id {
        return Err(AppError::Forbidden("Not allowed to view applicants".into()));
    }
    let rows = sqlx::query_as::<_, ApplicantRow>(
        "SELECT u.id, u.username, u.bio, u.avatar_file, a.message, a.created_at AS applied_at \
         FROM gig_applications a JOIN users u ON u.id = a.worker_id \
         WHERE a.gig_id = $1 \
           AND NOT EXISTS ( \
             SELECT 1 FROM user_blocks b \
             WHERE (b.blocker_id = $2 AND b.blocked_id = u.id) \
                OR (b.blocker_id = u.id AND b.blocked_id = $2) \
           ) \
         ORDER BY a.created_at DESC",
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
            })
            .collect(),
    ))
}

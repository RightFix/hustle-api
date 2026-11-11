use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{
    auth::CurrentUser,
    error::{AppError, AppResult},
    models::{
        FollowInfo, PaginatedUsers, SkillOut, SkillRow, UpdateAccountRequest, UpdateSkillsRequest,
        UserOut, UserRow,
    },
    AppState,
};

#[utoipa::path(
    get,
    path = "/api/users/me",
    tag = "users",
    security(("cookie_auth" = [])),
    responses(
        (status = 200, description = "Current user", body = UserOut),
        (status = 401, description = "Not authenticated", body = crate::models::Message),
    )
)]
pub async fn me(CurrentUser(user): CurrentUser) -> Json<UserOut> {
    Json(UserOut::from(user))
}

#[utoipa::path(
    patch,
    path = "/api/users/me",
    tag = "users",
    security(("cookie_auth" = [])),
    request_body = UpdateAccountRequest,
    responses(
        (status = 200, description = "Updated user", body = UserOut),
        (status = 401, description = "Not authenticated", body = crate::models::Message),
        (status = 422, description = "Invalid account type", body = crate::models::Message),
    )
)]
pub async fn update_me(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Json(payload): Json<UpdateAccountRequest>,
) -> AppResult<Json<UserOut>> {
    if let Some(account_type) = payload.account_type {
        match account_type.as_str() {
            "worker" | "hirer" => {
                sqlx::query("UPDATE users SET account_type = $1 WHERE id = $2")
                    .bind(&account_type)
                    .bind(current.id)
                    .execute(&state.pool)
                    .await?;
            }
            _ => return Err(AppError::Unprocessable("Invalid account type".into())),
        }
    }
    if let Some(bio) = payload.bio {
        sqlx::query("UPDATE users SET bio = $1 WHERE id = $2")
            .bind(bio)
            .bind(current.id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(location) = payload.location {
        let location = trim_or_clear(location, 80, "location")?;
        sqlx::query("UPDATE users SET location = $1 WHERE id = $2")
            .bind(location)
            .bind(current.id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(availability) = payload.availability {
        let availability = trim_or_clear(availability, 20, "availability")?;
        if let Some(v) = availability.as_deref() {
            if !matches!(v, "now" | "open" | "unavailable") {
                return Err(AppError::Unprocessable(
                    "availability must be one of: now, open, unavailable".into(),
                ));
            }
        }
        sqlx::query("UPDATE users SET availability = $1 WHERE id = $2")
            .bind(availability)
            .bind(current.id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(expected_rate) = payload.expected_rate {
        let expected_rate = trim_or_clear(expected_rate, 60, "expected_rate")?;
        sqlx::query("UPDATE users SET expected_rate = $1 WHERE id = $2")
            .bind(expected_rate)
            .bind(current.id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(languages) = payload.languages {
        let languages = trim_or_clear(languages, 120, "languages")?;
        sqlx::query("UPDATE users SET languages = $1 WHERE id = $2")
            .bind(languages)
            .bind(current.id)
            .execute(&state.pool)
            .await?;
    }
    if let Some(enabled) = payload.email_messages_enabled {
        sqlx::query("UPDATE users SET email_messages_enabled = $1 WHERE id = $2")
            .bind(enabled)
            .bind(current.id)
            .execute(&state.pool)
            .await?;
    }
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT id, email, username, hashed_password, bio, is_admin, account_type, location, availability, expected_rate, languages, avatar_file, email_verified_at, email_messages_enabled, is_suspended, suspended_reason, suspended_at, created_at FROM users WHERE id = $1",
    )
    .bind(current.id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(UserOut::from(user)))
}

/// Trimmed string with a length cap; empty means the caller wants to clear the column.
fn trim_or_clear(value: String, max: usize, field: &str) -> AppResult<Option<String>> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > max {
        return Err(AppError::Unprocessable(format!(
            "{field} must be at most {max} characters"
        )));
    }
    Ok(Some(trimmed.to_owned()))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct PeopleQuery {
    /// Substring match against username and bio.
    pub q: Option<String>,
    /// Substring match against the user's skills.
    pub skill: Option<String>,
    /// 1-based page number.
    #[serde(default = "default_people_page")]
    pub page: u32,
    /// Items per page (max 100).
    #[serde(default = "default_people_per_page")]
    pub per_page: u32,
}

fn default_people_page() -> u32 {
    1
}

fn default_people_per_page() -> u32 {
    20
}

#[utoipa::path(
    get,
    path = "/api/users",
    tag = "users",
    security(("cookie_auth" = [])),
    params(PeopleQuery),
    responses(
        (status = 200, description = "Search people, newest first", body = PaginatedUsers),
        (status = 401, description = "Not authenticated", body = crate::models::Message),
    )
)]
pub async fn search_users(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Query(query): Query<PeopleQuery>,
) -> AppResult<Json<PaginatedUsers>> {
    let pattern = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| format!("%{t}%"));
    let skill = query
        .skill
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| format!("%{t}%"));
    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);
    let offset = (page - 1) as i64 * per_page as i64;

    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM users u \
         WHERE u.id <> $1 \
           AND ($2::text IS NULL OR u.username ILIKE $2 OR u.bio ILIKE $2) \
           AND ($3::text IS NULL OR EXISTS ( \
                 SELECT 1 FROM user_skills s \
                 WHERE s.user_id = u.id AND s.skill ILIKE $3))",
    )
    .bind(current.id)
    .bind(&pattern)
    .bind(&skill)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query_as::<_, UserRow>(
        "SELECT id, email, username, hashed_password, bio, is_admin, account_type, \
                location, availability, expected_rate, languages, avatar_file, email_verified_at, email_messages_enabled, is_suspended, suspended_reason, suspended_at, created_at \
         FROM users u \
         WHERE u.id <> $1 \
           AND ($2::text IS NULL OR u.username ILIKE $2 OR u.bio ILIKE $2) \
           AND ($3::text IS NULL OR EXISTS ( \
                 SELECT 1 FROM user_skills s \
                 WHERE s.user_id = u.id AND s.skill ILIKE $3)) \
         ORDER BY u.id DESC \
         LIMIT $4 OFFSET $5",
    )
    .bind(current.id)
    .bind(&pattern)
    .bind(&skill)
    .bind(per_page as i64)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(PaginatedUsers {
        items: rows.into_iter().map(UserOut::from).collect(),
        total,
        page,
        per_page,
    }))
}

#[utoipa::path(
    get,
    path = "/api/users/{user_id}",
    tag = "users",
    security(("cookie_auth" = [])),
    params(("user_id" = i32, Path, description = "User id")),
    responses(
        (status = 200, description = "User", body = UserOut),
        (status = 404, description = "User not found", body = crate::models::Message),
    )
)]
pub async fn read_user(
    State(state): State<AppState>,
    Path(user_id): Path<i32>,
) -> AppResult<Json<UserOut>> {
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT id, email, username, hashed_password, bio, is_admin, account_type, location, availability, expected_rate, languages, avatar_file, email_verified_at, email_messages_enabled, is_suspended, suspended_reason, suspended_at, created_at FROM users WHERE id = $1",
    )
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found".into()))?;
    Ok(Json(UserOut::from(user)))
}

#[utoipa::path(
    post,
    path = "/api/users/{user_id}/follow",
    tag = "users",
    security(("cookie_auth" = [])),
    params(("user_id" = i32, Path, description = "User to follow")),
    responses(
        (status = 204, description = "Followed"),
        (status = 400, description = "Cannot follow yourself", body = crate::models::Message),
        (status = 404, description = "User not found", body = crate::models::Message),
    )
)]
pub async fn follow_user(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(user_id): Path<i32>,
) -> AppResult<StatusCode> {
    if user_id == current.id {
        return Err(AppError::BadRequest("Cannot follow yourself".into()));
    }
    let target = sqlx::query_scalar::<_, i32>("SELECT id FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?;
    if target.is_none() {
        return Err(AppError::NotFound("User not found".into()));
    }
    if crate::routes::blocks::is_blocked_either(&state.pool, current.id, user_id).await? {
        return Err(AppError::Forbidden("Cannot follow this user".into()));
    }
    sqlx::query(
        "INSERT INTO follows (follower_id, following_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(current.id)
    .bind(user_id)
    .execute(&state.pool)
    .await?;
    crate::notify::notify(&state, user_id, current.id, "follow", None, None).await;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/users/{user_id}/follow",
    tag = "users",
    security(("cookie_auth" = [])),
    params(("user_id" = i32, Path, description = "User to unfollow")),
    responses((status = 204, description = "Unfollowed")),
)]
pub async fn unfollow_user(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(user_id): Path<i32>,
) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM follows WHERE follower_id = $1 AND following_id = $2")
        .bind(current.id)
        .bind(user_id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/users/{user_id}/follow-info",
    tag = "users",
    security(("cookie_auth" = [])),
    params(("user_id" = i32, Path, description = "User id")),
    responses(
        (status = 200, description = "Follow stats", body = FollowInfo),
        (status = 404, description = "User not found", body = crate::models::Message),
    )
)]
pub async fn follow_info(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(user_id): Path<i32>,
) -> AppResult<Json<FollowInfo>> {
    let target = sqlx::query_scalar::<_, i32>("SELECT id FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?;
    if target.is_none() {
        return Err(AppError::NotFound("User not found".into()));
    }
    let followers_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM follows WHERE following_id = $1")
            .bind(user_id)
            .fetch_one(&state.pool)
            .await?;
    let following_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM follows WHERE follower_id = $1")
            .bind(user_id)
            .fetch_one(&state.pool)
            .await?;
    let is_following = sqlx::query_scalar::<_, i32>(
        "SELECT follower_id FROM follows WHERE follower_id = $1 AND following_id = $2",
    )
    .bind(current.id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?
    .is_some();
    let is_blocked_by_me = sqlx::query_scalar::<_, i64>(
        "SELECT 1::bigint FROM user_blocks WHERE blocker_id = $1 AND blocked_id = $2",
    )
    .bind(current.id)
    .bind(user_id)
    .fetch_optional(&state.pool)
    .await?
    .is_some();
    let is_blocked_by_them = sqlx::query_scalar::<_, i64>(
        "SELECT 1::bigint FROM user_blocks WHERE blocker_id = $1 AND blocked_id = $2",
    )
    .bind(user_id)
    .bind(current.id)
    .fetch_optional(&state.pool)
    .await?
    .is_some();
    Ok(Json(FollowInfo {
        followers_count,
        following_count,
        is_following,
        is_blocked_by_me,
        is_blocked_by_them,
    }))
}

fn skill_outs(rows: Vec<SkillRow>) -> Vec<SkillOut> {
    rows.into_iter()
        .map(|r| SkillOut {
            id: r.id,
            skill: r.skill,
        })
        .collect()
}

#[utoipa::path(
    get,
    path = "/api/users/{user_id}/skills",
    tag = "users",
    security(("cookie_auth" = [])),
    params(("user_id" = i32, Path, description = "User id")),
    responses(
        (status = 200, description = "User's skills", body = Vec<SkillOut>),
        (status = 404, description = "User not found", body = crate::models::Message),
    )
)]
pub async fn read_skills(
    State(state): State<AppState>,
    Path(user_id): Path<i32>,
) -> AppResult<Json<Vec<SkillOut>>> {
    let exists = sqlx::query_scalar::<_, i32>("SELECT id FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("User not found".into()));
    }
    let rows = sqlx::query_as::<_, SkillRow>(
        "SELECT id, skill FROM user_skills WHERE user_id = $1 ORDER BY id ASC",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(skill_outs(rows)))
}

#[utoipa::path(
    put,
    path = "/api/users/me/skills",
    tag = "users",
    security(("cookie_auth" = [])),
    request_body = UpdateSkillsRequest,
    responses(
        (status = 200, description = "Skills replaced", body = Vec<SkillOut>),
        (status = 401, description = "Not authenticated", body = crate::models::Message),
        (status = 422, description = "Invalid skill list", body = crate::models::Message),
    )
)]
pub async fn update_skills(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Json(payload): Json<UpdateSkillsRequest>,
) -> AppResult<Json<Vec<SkillOut>>> {
    let mut skills: Vec<String> = Vec::new();
    for raw in payload.skills.iter().map(|s| s.trim().to_owned()) {
        if raw.len() < 2 || raw.len() > 40 {
            return Err(AppError::Unprocessable(
                "Each skill must be 2-40 characters".into(),
            ));
        }
        if !skills.iter().any(|s| s.eq_ignore_ascii_case(&raw)) {
            skills.push(raw);
        }
    }
    if skills.is_empty() {
        return Err(AppError::Unprocessable("Add at least one skill".into()));
    }
    if skills.len() > 20 {
        return Err(AppError::Unprocessable("Maximum 20 skills".into()));
    }

    let mut tx = state.pool.begin().await?;
    sqlx::query("DELETE FROM user_skills WHERE user_id = $1")
        .bind(current.id)
        .execute(&mut *tx)
        .await?;
    for skill in &skills {
        sqlx::query("INSERT INTO user_skills (user_id, skill) VALUES ($1, $2)")
            .bind(current.id)
            .bind(skill)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;

    let rows = sqlx::query_as::<_, SkillRow>(
        "SELECT id, skill FROM user_skills WHERE user_id = $1 ORDER BY id ASC",
    )
    .bind(current.id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(skill_outs(rows)))
}

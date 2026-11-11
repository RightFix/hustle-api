use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::{
    auth::CurrentUser,
    error::{AppError, AppResult},
    models::{BlockedUserOut, Message},
    AppState,
};

/// Returns true if either user has blocked the other (interaction should be denied).
pub async fn is_blocked_either(pool: &sqlx::PgPool, a: i32, b: i32) -> Result<bool, sqlx::Error> {
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT 1::bigint FROM user_blocks \
         WHERE (blocker_id = $1 AND blocked_id = $2) OR (blocker_id = $2 AND blocked_id = $1) \
         LIMIT 1",
    )
    .bind(a)
    .bind(b)
    .fetch_optional(pool)
    .await?;
    Ok(exists.is_some())
}

#[utoipa::path(
    post,
    path = "/api/users/{user_id}/block",
    tag = "users",
    security(("cookie_auth" = [])),
    params(("user_id" = i32, Path, description = "User to block")),
    responses(
        (status = 204, description = "User blocked"),
        (status = 400, description = "Cannot block yourself", body = Message),
        (status = 404, description = "User not found", body = Message),
    )
)]
pub async fn block_user(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(user_id): Path<i32>,
) -> AppResult<StatusCode> {
    if user_id == current.id {
        return Err(AppError::BadRequest("You cannot block yourself".into()));
    }
    let exists = sqlx::query_scalar::<_, i32>("SELECT id FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("User not found".into()));
    }

    sqlx::query(
        "INSERT INTO user_blocks (blocker_id, blocked_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(current.id)
    .bind(user_id)
    .execute(&state.pool)
    .await?;

    // Remove follow relationship in both directions when blocking.
    sqlx::query(
        "DELETE FROM follows WHERE (follower_id = $1 AND following_id = $2) \
                                  OR (follower_id = $2 AND following_id = $1)",
    )
    .bind(current.id)
    .bind(user_id)
    .execute(&state.pool)
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/users/{user_id}/block",
    tag = "users",
    security(("cookie_auth" = [])),
    params(("user_id" = i32, Path, description = "User to unblock")),
    responses((status = 204, description = "User unblocked"))
)]
pub async fn unblock_user(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(user_id): Path<i32>,
) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM user_blocks WHERE blocker_id = $1 AND blocked_id = $2")
        .bind(current.id)
        .bind(user_id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/users/me/blocks",
    tag = "users",
    security(("cookie_auth" = [])),
    responses((status = 200, description = "Users I have blocked", body = Vec<BlockedUserOut>))
)]
pub async fn list_my_blocks(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
) -> AppResult<Json<Vec<BlockedUserOut>>> {
    let rows = sqlx::query_as::<_, (i32, String, Option<String>, Option<String>, chrono::DateTime<chrono::Utc>)>(
        "SELECT u.id, u.username, u.bio, u.avatar_file, b.created_at \
         FROM user_blocks b JOIN users u ON u.id = b.blocked_id \
         WHERE b.blocker_id = $1 ORDER BY b.created_at DESC",
    )
    .bind(current.id)
    .fetch_all(&state.pool)
    .await?;

    let out = rows
        .into_iter()
        .map(|(id, username, bio, avatar_file, created_at)| BlockedUserOut {
            id,
            username,
            bio,
            avatar_url: avatar_file.map(|f| format!("/media/avatars/{f}")),
            blocked_at: created_at,
        })
        .collect();
    Ok(Json(out))
}

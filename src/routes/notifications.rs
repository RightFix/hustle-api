use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{
    auth::CurrentUser,
    error::{AppError, AppResult},
    models::{NotificationListRow, NotificationOut, NotificationPage, UnreadCountOut},
    AppState,
};

const SELECT: &str = "SELECT n.id, n.actor_id, n.kind, n.ref_type, n.ref_id, \
     n.created_at, n.read_at, u.username AS actor_username, u.bio AS actor_bio, u.avatar_file AS actor_avatar \
     FROM notifications n JOIN users u ON u.id = n.actor_id";

fn out(row: NotificationListRow) -> NotificationOut {
    NotificationOut {
        id: row.id,
        kind: row.kind,
        actor: crate::models::UserBrief {
            id: row.actor_id,
            username: row.actor_username,
            bio: row.actor_bio,
            avatar_url: row.actor_avatar.map(|f| format!("/media/avatars/{f}")),
        },
        ref_type: row.ref_type,
        ref_id: row.ref_id,
        created_at: row.created_at,
        read_at: row.read_at,
    }
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListQuery {
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
    path = "/api/notifications",
    tag = "notifications",
    security(("cookie_auth" = [])),
    params(ListQuery),
    responses(
        (status = 200, description = "My notifications, newest first", body = NotificationPage),
        (status = 401, description = "Not authenticated", body = crate::models::Message),
    )
)]
pub async fn list_notifications(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Query(query): Query<ListQuery>,
) -> AppResult<Json<NotificationPage>> {
    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);
    let offset = (page - 1) as i64 * per_page as i64;

    let (total, unread_count): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COUNT(*) FILTER (WHERE read_at IS NULL) \
         FROM notifications WHERE user_id = $1",
    )
    .bind(current.id)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query_as::<_, NotificationListRow>(&format!(
        "{SELECT} WHERE n.user_id = $1 ORDER BY n.id DESC LIMIT $2 OFFSET $3"
    ))
    .bind(current.id)
    .bind(per_page as i64)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(NotificationPage {
        items: rows.into_iter().map(out).collect(),
        total,
        page,
        per_page,
        unread_count,
    }))
}

#[utoipa::path(
    get,
    path = "/api/notifications/unread-count",
    tag = "notifications",
    security(("cookie_auth" = [])),
    responses(
        (status = 200, description = "Unread notification count", body = UnreadCountOut),
        (status = 401, description = "Not authenticated", body = crate::models::Message),
    )
)]
pub async fn unread_count(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
) -> AppResult<Json<UnreadCountOut>> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM notifications WHERE user_id = $1 AND read_at IS NULL",
    )
    .bind(current.id)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(UnreadCountOut { count }))
}

#[utoipa::path(
    post,
    path = "/api/notifications/{notification_id}/read",
    tag = "notifications",
    security(("cookie_auth" = [])),
    params(("notification_id" = i32, Path, description = "Notification id")),
    responses(
        (status = 204, description = "Marked read"),
        (status = 404, description = "Not found or not yours", body = crate::models::Message),
    )
)]
pub async fn mark_read(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(notification_id): Path<i32>,
) -> AppResult<axum::http::StatusCode> {
    let result = sqlx::query(
        "UPDATE notifications SET read_at = COALESCE(read_at, now()) \
         WHERE id = $1 AND user_id = $2",
    )
    .bind(notification_id)
    .bind(current.id)
    .execute(&state.pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Notification not found".into()));
    }
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/notifications/read-all",
    tag = "notifications",
    security(("cookie_auth" = [])),
    responses(
        (status = 204, description = "All marked read"),
        (status = 401, description = "Not authenticated", body = crate::models::Message),
    )
)]
pub async fn mark_all_read(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
) -> AppResult<axum::http::StatusCode> {
    sqlx::query("UPDATE notifications SET read_at = now() WHERE user_id = $1 AND read_at IS NULL")
        .bind(current.id)
        .execute(&state.pool)
        .await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

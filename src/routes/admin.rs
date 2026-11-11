use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;
use utoipa::IntoParams;

use crate::{
    auth::AdminUser,
    error::{AppError, AppResult},
    models::{
        AdminAuditOut, AdminAuditRow, AdminGigOut, AdminPostOut, AdminUpdateUser, AdminUserDetail, Message, PaginatedAdminAudit, PaginatedAdminGigs,
        PaginatedAdminPosts, PaginatedUsers, StatsOut, UserOut, UserRow,
    },
    AppState,
};

#[derive(Debug, Deserialize, IntoParams)]
pub struct PageQuery {
    /// 1-based page number.
    #[serde(default = "default_page")]
    pub page: u32,
    /// Items per page (max 100).
    #[serde(default = "default_per_page")]
    pub per_page: u32,
    /// Optional text search (username or email for users; title/description for gigs; content for posts).
    pub q: Option<String>,
    /// Filter users by suspension state: "suspended", "active", or omit for all.
    pub status: Option<String>,
}

fn default_page() -> u32 { 1 }
fn default_per_page() -> u32 { 20 }

fn page_bounds(q: &PageQuery) -> (u32, u32, i64) {
    let page = q.page.max(1);
    let per_page = q.per_page.clamp(1, 100);
    let offset = (page - 1) as i64 * per_page as i64;
    (page, per_page, offset)
}

const USER_SELECT: &str = "SELECT id, email, username, hashed_password, bio, is_admin, account_type, \
    location, availability, expected_rate, languages, avatar_file, email_verified_at, \
    email_messages_enabled, is_suspended, suspended_reason, suspended_at, created_at FROM users";

#[utoipa::path(
    get,
    path = "/api/admin/stats",
    tag = "admin",
    security(("cookie_auth" = [])),
    responses(
        (status = 200, description = "Platform totals", body = StatsOut),
        (status = 401, description = "Not authenticated", body = Message),
        (status = 403, description = "Not an admin", body = Message),
    )
)]
pub async fn stats(State(state): State<AppState>, _admin: AdminUser) -> AppResult<Json<StatsOut>> {
    let user_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users")
        .fetch_one(&state.pool).await?;
    let post_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM posts")
        .fetch_one(&state.pool).await?;
    let gig_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM gigs")
        .fetch_one(&state.pool).await?;
    let conversation_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM conversations")
        .fetch_one(&state.pool).await?;
    let message_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM messages")
        .fetch_one(&state.pool).await?;
    let suspended_user_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM users WHERE is_suspended",
    ).fetch_one(&state.pool).await?;
    let unverified_user_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM users WHERE email_verified_at IS NULL",
    ).fetch_one(&state.pool).await?;

    Ok(Json(StatsOut {
        user_count,
        post_count,
        gig_count,
        conversation_count,
        message_count,
        suspended_user_count,
        unverified_user_count,
    }))
}

#[utoipa::path(
    get,
    path = "/api/admin/users",
    tag = "admin",
    security(("cookie_auth" = [])),
    params(PageQuery),
    responses(
        (status = 200, description = "Paginated user list with optional search/filter", body = PaginatedUsers),
        (status = 401, description = "Not authenticated", body = Message),
        (status = 403, description = "Not an admin", body = Message),
    )
)]
pub async fn list_users(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(query): Query<PageQuery>,
) -> AppResult<Json<PaginatedUsers>> {
    let (page, per_page, offset) = page_bounds(&query);
    let q = query.q.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| format!("%{s}%"));
    let suspended: Option<bool> = match query.status.as_deref() {
        Some("suspended") => Some(true),
        Some("active") => Some(false),
        _ => None,
    };

    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM users \
         WHERE ($1::text IS NULL OR username ILIKE $1 OR email ILIKE $1) \
           AND ($2::bool IS NULL OR is_suspended = $2)",
    )
    .bind(&q)
    .bind(suspended)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query_as::<_, UserRow>(&format!(
        "{USER_SELECT} \
         WHERE ($1::text IS NULL OR username ILIKE $1 OR email ILIKE $1) \
           AND ($2::bool IS NULL OR is_suspended = $2) \
         ORDER BY id DESC LIMIT $3 OFFSET $4"
    ))
    .bind(&q)
    .bind(suspended)
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
    path = "/api/admin/users/{id}",
    tag = "admin",
    security(("cookie_auth" = [])),
    params(("id" = i32, Path, description = "User id")),
    responses(
        (status = 200, description = "User detail with activity counts", body = AdminUserDetail),
        (status = 404, description = "User not found", body = Message),
    )
)]
pub async fn get_user(
    State(state): State<AppState>,
    _admin: AdminUser,
    Path(id): Path<i32>,
) -> AppResult<Json<AdminUserDetail>> {
    let user = sqlx::query_as::<_, UserRow>(&format!("{USER_SELECT} WHERE id = $1"))
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    let post_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM posts WHERE author_id = $1")
        .bind(id).fetch_one(&state.pool).await?;
    let gig_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM gigs WHERE poster_id = $1")
        .bind(id).fetch_one(&state.pool).await?;
    let application_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM gig_applications WHERE worker_id = $1",
    ).bind(id).fetch_one(&state.pool).await?;
    let conversation_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM conversations WHERE user_a = $1 OR user_b = $1",
    ).bind(id).fetch_one(&state.pool).await?;

    Ok(Json(AdminUserDetail {
        user: UserOut::from(user),
        post_count,
        gig_count,
        application_count,
        conversation_count,
    }))
}

#[utoipa::path(
    patch,
    path = "/api/admin/users/{id}",
    tag = "admin",
    security(("cookie_auth" = [])),
    params(("id" = i32, Path, description = "User id")),
    request_body = AdminUpdateUser,
    responses(
        (status = 200, description = "Updated user", body = UserOut),
        (status = 400, description = "Invalid update", body = Message),
        (status = 404, description = "User not found", body = Message),
    )
)]
pub async fn update_user(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i32>,
    Json(payload): Json<AdminUpdateUser>,
) -> AppResult<Json<UserOut>> {
    let user = sqlx::query_as::<_, UserRow>(&format!("{USER_SELECT} WHERE id = $1"))
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    if let Some(make_admin) = payload.is_admin {
        if admin.id == id && !make_admin {
            return Err(AppError::BadRequest("You cannot remove your own admin access".into()));
        }
        sqlx::query("UPDATE users SET is_admin = $1 WHERE id = $2")
            .bind(make_admin).bind(id).execute(&state.pool).await?;
        crate::audit::log(&state, admin.id, if make_admin { "user.promote_admin" } else { "user.demote_admin" }, "user", Some(id), json!({})).await;
    }
    if let Some(verified) = payload.email_verified {
        if verified {
            sqlx::query("UPDATE users SET email_verified_at = COALESCE(email_verified_at, now()) WHERE id = $1")
                .bind(id).execute(&state.pool).await?;
            crate::audit::log(&state, admin.id, "user.verify_email", "user", Some(id), json!({})).await;
        } else {
            sqlx::query("UPDATE users SET email_verified_at = NULL WHERE id = $1")
                .bind(id).execute(&state.pool).await?;
            crate::audit::log(&state, admin.id, "user.unverify_email", "user", Some(id), json!({})).await;
        }
    }
    if let Some(suspend) = payload.is_suspended {
        if suspend {
            if admin.id == id {
                return Err(AppError::BadRequest("You cannot suspend your own account".into()));
            }
            let reason = payload.suspended_reason.as_deref().map(str::trim).filter(|s| !s.is_empty());
            sqlx::query(
                "UPDATE users SET is_suspended = TRUE, suspended_reason = $1, suspended_at = now() WHERE id = $2",
            )
            .bind(reason)
            .bind(id)
            .execute(&state.pool)
            .await?;
            crate::audit::log(&state, admin.id, "user.suspend", "user", Some(id), json!({"reason": reason})).await;
        } else {
            sqlx::query(
                "UPDATE users SET is_suspended = FALSE, suspended_reason = NULL, suspended_at = NULL WHERE id = $1",
            )
            .bind(id)
            .execute(&state.pool)
            .await?;
            crate::audit::log(&state, admin.id, "user.unsuspend", "user", Some(id), json!({})).await;
        }
    } else if let Some(reason) = payload.suspended_reason {
        if user.is_suspended {
            let reason = reason.trim();
            let reason = if reason.is_empty() { None } else { Some(reason.to_owned()) };
            sqlx::query("UPDATE users SET suspended_reason = $1 WHERE id = $2")
                .bind(reason.as_deref()).bind(id).execute(&state.pool).await?;
        }
    }

    let updated = sqlx::query_as::<_, UserRow>(&format!("{USER_SELECT} WHERE id = $1"))
        .bind(id)
        .fetch_one(&state.pool)
        .await?;
    Ok(Json(UserOut::from(updated)))
}

#[utoipa::path(
    delete,
    path = "/api/admin/users/{id}",
    tag = "admin",
    security(("cookie_auth" = [])),
    params(("id" = i32, Path, description = "User id")),
    responses(
        (status = 200, description = "User deleted", body = Message),
        (status = 400, description = "Cannot delete yourself", body = Message),
        (status = 404, description = "User not found", body = Message),
    )
)]
pub async fn delete_user(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i32>,
) -> AppResult<Json<Message>> {
    if admin.id == id {
        return Err(AppError::BadRequest("You cannot delete your own account".into()));
    }
    let res = sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("User not found".into()));
    }
    crate::audit::log(&state, admin.id, "user.delete", "user", Some(id), json!({})).await;
    Ok(Json(Message { detail: "User deleted".into() }))
}

#[utoipa::path(
    get,
    path = "/api/admin/posts",
    tag = "admin",
    security(("cookie_auth" = [])),
    params(PageQuery),
    responses((status = 200, description = "Paginated posts for moderation", body = PaginatedAdminPosts))
)]
pub async fn list_posts(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(query): Query<PageQuery>,
) -> AppResult<Json<PaginatedAdminPosts>> {
    let (page, per_page, offset) = page_bounds(&query);
    let q = query.q.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| format!("%{s}%"));

    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM posts p WHERE ($1::text IS NULL OR p.content ILIKE $1)",
    ).bind(&q).fetch_one(&state.pool).await?;

    let rows = sqlx::query_as::<_, (i32, i32, String, String, chrono::DateTime<chrono::Utc>)>(
        "SELECT p.id, p.author_id, u.username, p.content, p.created_at \
         FROM posts p JOIN users u ON u.id = p.author_id \
         WHERE ($1::text IS NULL OR p.content ILIKE $1) \
         ORDER BY p.id DESC LIMIT $2 OFFSET $3",
    )
    .bind(&q).bind(per_page as i64).bind(offset)
    .fetch_all(&state.pool).await?;

    Ok(Json(PaginatedAdminPosts {
        items: rows.into_iter().map(|(id, author_id, author_username, content, created_at)| {
            AdminPostOut { id, author_id, author_username, content, created_at }
        }).collect(),
        total, page, per_page,
    }))
}

#[utoipa::path(
    delete,
    path = "/api/admin/posts/{id}",
    tag = "admin",
    security(("cookie_auth" = [])),
    params(("id" = i32, Path, description = "Post id")),
    responses(
        (status = 200, description = "Post deleted", body = Message),
        (status = 404, description = "Post not found", body = Message),
    )
)]
pub async fn delete_post(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i32>,
) -> AppResult<Json<Message>> {
    let res = sqlx::query("DELETE FROM posts WHERE id = $1")
        .bind(id).execute(&state.pool).await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Post not found".into()));
    }
    crate::audit::log(&state, admin.id, "post.delete", "post", Some(id), json!({})).await;
    Ok(Json(Message { detail: "Post deleted".into() }))
}

#[utoipa::path(
    get,
    path = "/api/admin/gigs",
    tag = "admin",
    security(("cookie_auth" = [])),
    params(PageQuery),
    responses((status = 200, description = "Paginated gigs for moderation", body = PaginatedAdminGigs))
)]
pub async fn list_gigs(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(query): Query<PageQuery>,
) -> AppResult<Json<PaginatedAdminGigs>> {
    let (page, per_page, offset) = page_bounds(&query);
    let q = query.q.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| format!("%{s}%"));

    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM gigs g WHERE ($1::text IS NULL OR g.title ILIKE $1 OR g.description ILIKE $1)",
    ).bind(&q).fetch_one(&state.pool).await?;

    let rows = sqlx::query_as::<_, (i32, i32, String, String, String, String, String, Option<String>, chrono::DateTime<chrono::Utc>)>(
        "SELECT g.id, g.poster_id, u.username, g.title, g.description, g.collar, g.status, g.location, g.created_at \
         FROM gigs g JOIN users u ON u.id = g.poster_id \
         WHERE ($1::text IS NULL OR g.title ILIKE $1 OR g.description ILIKE $1) \
         ORDER BY g.id DESC LIMIT $2 OFFSET $3",
    )
    .bind(&q).bind(per_page as i64).bind(offset)
    .fetch_all(&state.pool).await?;

    Ok(Json(PaginatedAdminGigs {
        items: rows.into_iter().map(|(id, poster_id, poster_username, title, description, collar, status, location, created_at)| {
            AdminGigOut { id, poster_id, poster_username, title, description, collar, status, location, created_at }
        }).collect(),
        total, page, per_page,
    }))
}

#[utoipa::path(
    delete,
    path = "/api/admin/gigs/{id}",
    tag = "admin",
    security(("cookie_auth" = [])),
    params(("id" = i32, Path, description = "Gig id")),
    responses(
        (status = 200, description = "Gig deleted", body = Message),
        (status = 404, description = "Gig not found", body = Message),
    )
)]
pub async fn delete_gig(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i32>,
) -> AppResult<Json<Message>> {
    let res = sqlx::query("DELETE FROM gigs WHERE id = $1")
        .bind(id).execute(&state.pool).await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound("Gig not found".into()));
    }
    crate::audit::log(&state, admin.id, "gig.delete", "gig", Some(id), json!({})).await;
    Ok(Json(Message { detail: "Gig deleted".into() }))
}


#[derive(Debug, Deserialize, IntoParams)]
pub struct AuditQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
    /// Filter by action name (exact match), e.g. "user.suspend".
    pub action: Option<String>,
    /// Filter by target type, e.g. "user", "post", "gig", "report".
    pub target_type: Option<String>,
    /// Filter by actor user id.
    pub actor_id: Option<i32>,
}

#[utoipa::path(
    get,
    path = "/api/admin/audit",
    tag = "admin",
    security(("cookie_auth" = [])),
    params(AuditQuery),
    responses((status = 200, description = "Paginated admin audit log", body = PaginatedAdminAudit))
)]
pub async fn list_audit(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(q): Query<AuditQuery>,
) -> AppResult<Json<PaginatedAdminAudit>> {
    let page = q.page.max(1);
    let per_page = q.per_page.clamp(1, 100);
    let offset = (page - 1) as i64 * per_page as i64;
    let action = q.action.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let target_type = q.target_type.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM admin_audit_logs a \
         WHERE ($1::text IS NULL OR a.action = $1) \
           AND ($2::text IS NULL OR a.target_type = $2) \
           AND ($3::int IS NULL OR a.actor_id = $3)",
    )
    .bind(action)
    .bind(target_type)
    .bind(q.actor_id)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query_as::<_, AdminAuditRow>(
        "SELECT a.id, a.actor_id, u.username AS actor_username, a.action, a.target_type, \
                a.target_id, a.metadata, a.created_at \
         FROM admin_audit_logs a \
         LEFT JOIN users u ON u.id = a.actor_id \
         WHERE ($1::text IS NULL OR a.action = $1) \
           AND ($2::text IS NULL OR a.target_type = $2) \
           AND ($3::int IS NULL OR a.actor_id = $3) \
         ORDER BY a.created_at DESC, a.id DESC \
         LIMIT $4 OFFSET $5",
    )
    .bind(action)
    .bind(target_type)
    .bind(q.actor_id)
    .bind(per_page as i64)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(PaginatedAdminAudit {
        items: rows.into_iter().map(AdminAuditOut::from).collect(),
        total,
        page,
        per_page,
    }))
}

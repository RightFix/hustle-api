use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::json;
use utoipa::IntoParams;

use crate::{
    auth::{AdminUser, CurrentUser},
    error::{AppError, AppResult},
    models::{
        AdminReportOut, AdminReportRow, CreateReportRequest, Message, PaginatedAdminReports,
        ReportOut, ReportRow, ResolveReportRequest,
    },
    AppState,
};

const MAX_REASON_LEN: usize = 80;
const MAX_DETAILS_LEN: usize = 2000;

fn clean_opt(s: Option<String>, max: usize) -> Option<String> {
    s.map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
        .map(|v| v.chars().take(max).collect())
}

#[utoipa::path(
    post,
    path = "/api/reports",
    tag = "reports",
    security(("cookie_auth" = [])),
    request_body = CreateReportRequest,
    responses(
        (status = 201, description = "Report submitted", body = ReportOut),
        (status = 400, description = "Invalid report", body = Message),
        (status = 404, description = "Target not found", body = Message),
        (status = 409, description = "Duplicate open report", body = Message),
    )
)]
pub async fn create_report(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Json(payload): Json<CreateReportRequest>,
) -> AppResult<(StatusCode, Json<ReportOut>)> {
    let target_type = payload.target_type.trim().to_lowercase();
    if !matches!(target_type.as_str(), "user" | "post" | "gig") {
        return Err(AppError::BadRequest(
            "target_type must be one of: user, post, gig".into(),
        ));
    }
    let reason = payload.reason.trim();
    if reason.is_empty() {
        return Err(AppError::BadRequest("reason is required".into()));
    }
    let reason: String = reason.chars().take(MAX_REASON_LEN).collect();
    let details = clean_opt(payload.details, MAX_DETAILS_LEN);

    // Ensure target exists and prevent self-reporting users.
    match target_type.as_str() {
        "user" => {
            if payload.target_id == current.id {
                return Err(AppError::BadRequest("You cannot report yourself".into()));
            }
            let exists = sqlx::query_scalar::<_, i32>("SELECT id FROM users WHERE id = $1")
                .bind(payload.target_id)
                .fetch_optional(&state.pool)
                .await?;
            if exists.is_none() {
                return Err(AppError::NotFound("User not found".into()));
            }
        }
        "post" => {
            let exists = sqlx::query_scalar::<_, i32>("SELECT id FROM posts WHERE id = $1")
                .bind(payload.target_id)
                .fetch_optional(&state.pool)
                .await?;
            if exists.is_none() {
                return Err(AppError::NotFound("Post not found".into()));
            }
        }
        "gig" => {
            let exists = sqlx::query_scalar::<_, i32>("SELECT id FROM gigs WHERE id = $1")
                .bind(payload.target_id)
                .fetch_optional(&state.pool)
                .await?;
            if exists.is_none() {
                return Err(AppError::NotFound("Gig not found".into()));
            }
        }
        _ => {}
    }

    let row = sqlx::query_as::<_, ReportRow>(
        "INSERT INTO reports (reporter_id, target_type, target_id, reason, details) \
         VALUES ($1, $2, $3, $4, $5) \
         RETURNING id, reporter_id, target_type, target_id, reason, details, status, \
                   admin_note, resolved_by, resolved_at, created_at",
    )
    .bind(current.id)
    .bind(&target_type)
    .bind(payload.target_id)
    .bind(&reason)
    .bind(&details)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        if let Some(db) = e.as_database_error() {
            // unique violation on open duplicate
            if db.code().as_deref() == Some("23505") {
                return AppError::Conflict(
                    "You already have an open report for this target".into(),
                );
            }
        }
        AppError::from(e)
    })?;

    Ok((StatusCode::CREATED, Json(ReportOut::from(row))))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct AdminReportQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
    /// open | resolved | dismissed | all (default: open)
    pub status: Option<String>,
    /// user | post | gig
    pub target_type: Option<String>,
}
fn default_page() -> u32 {
    1
}
fn default_per_page() -> u32 {
    20
}

const ADMIN_REPORT_SELECT: &str = "\
    SELECT r.id, r.reporter_id, ru.username AS reporter_username, \
           r.target_type, r.target_id, \
           CASE r.target_type \
             WHEN 'user' THEN (SELECT '@' || u.username FROM users u WHERE u.id = r.target_id) \
             WHEN 'post' THEN (SELECT LEFT(p.content, 80) FROM posts p WHERE p.id = r.target_id) \
             WHEN 'gig'  THEN (SELECT g.title FROM gigs g WHERE g.id = r.target_id) \
             ELSE NULL END AS target_label, \
           r.reason, r.details, r.status, r.admin_note, \
           r.resolved_by, au.username AS resolved_by_username, r.resolved_at, r.created_at \
    FROM reports r \
    JOIN users ru ON ru.id = r.reporter_id \
    LEFT JOIN users au ON au.id = r.resolved_by";

#[utoipa::path(
    get,
    path = "/api/admin/reports",
    tag = "admin",
    security(("cookie_auth" = [])),
    params(AdminReportQuery),
    responses(
        (status = 200, description = "Paginated reports for moderation", body = PaginatedAdminReports),
    )
)]
pub async fn admin_list_reports(
    State(state): State<AppState>,
    _admin: AdminUser,
    Query(q): Query<AdminReportQuery>,
) -> AppResult<Json<PaginatedAdminReports>> {
    let page = q.page.max(1);
    let per_page = q.per_page.clamp(1, 100);
    let offset = (page - 1) as i64 * per_page as i64;
    let status = q.status.as_deref().unwrap_or("open");
    let status_filter: Option<&str> = if status == "all" { None } else { Some(status) };
    let target_type = q
        .target_type
        .as_deref()
        .filter(|s| matches!(*s, "user" | "post" | "gig"));

    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM reports \
         WHERE ($1::text IS NULL OR status = $1) \
           AND ($2::text IS NULL OR target_type = $2)",
    )
    .bind(status_filter)
    .bind(target_type)
    .fetch_one(&state.pool)
    .await?;

    let rows = sqlx::query_as::<_, AdminReportRow>(&format!(
        "{ADMIN_REPORT_SELECT} \
         WHERE ($1::text IS NULL OR r.status = $1) \
           AND ($2::text IS NULL OR r.target_type = $2) \
         ORDER BY r.created_at DESC LIMIT $3 OFFSET $4"
    ))
    .bind(status_filter)
    .bind(target_type)
    .bind(per_page as i64)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(PaginatedAdminReports {
        items: rows.into_iter().map(AdminReportOut::from).collect(),
        total,
        page,
        per_page,
    }))
}

#[utoipa::path(
    patch,
    path = "/api/admin/reports/{id}",
    tag = "admin",
    security(("cookie_auth" = [])),
    params(("id" = i32, Path, description = "Report id")),
    request_body = ResolveReportRequest,
    responses(
        (status = 200, description = "Report updated", body = AdminReportOut),
        (status = 400, description = "Invalid status", body = Message),
        (status = 404, description = "Report not found", body = Message),
    )
)]
pub async fn admin_resolve_report(
    State(state): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i32>,
    Json(payload): Json<ResolveReportRequest>,
) -> AppResult<Json<AdminReportOut>> {
    let status = payload.status.trim().to_lowercase();
    if !matches!(status.as_str(), "resolved" | "dismissed") {
        return Err(AppError::BadRequest(
            "status must be 'resolved' or 'dismissed'".into(),
        ));
    }
    let note = clean_opt(payload.admin_note, MAX_DETAILS_LEN);

    // Load report to know target for optional moderation actions.
    let existing = sqlx::query_as::<_, ReportRow>(
        "SELECT id, reporter_id, target_type, target_id, reason, details, status, \
                admin_note, resolved_by, resolved_at, created_at \
         FROM reports WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Report not found".into()))?;

    if payload.delete_target && status == "resolved" {
        match existing.target_type.as_str() {
            "post" => {
                let res = sqlx::query("DELETE FROM posts WHERE id = $1")
                    .bind(existing.target_id)
                    .execute(&state.pool)
                    .await?;
                if res.rows_affected() > 0 {
                    crate::audit::log(
                        &state,
                        admin.id,
                        "post.delete",
                        "post",
                        Some(existing.target_id),
                        json!({"from_report": id}),
                    )
                    .await;
                }
            }
            "gig" => {
                let res = sqlx::query("DELETE FROM gigs WHERE id = $1")
                    .bind(existing.target_id)
                    .execute(&state.pool)
                    .await?;
                if res.rows_affected() > 0 {
                    crate::audit::log(
                        &state,
                        admin.id,
                        "gig.delete",
                        "gig",
                        Some(existing.target_id),
                        json!({"from_report": id}),
                    )
                    .await;
                }
            }
            "user" => {
                return Err(AppError::BadRequest(
                    "Cannot delete a user target via delete_target; use suspend_user".into(),
                ));
            }
            _ => {}
        }
    }

    if payload.suspend_user && status == "resolved" && existing.target_type == "user" {
        if existing.target_id == admin.id {
            return Err(AppError::BadRequest(
                "You cannot suspend your own account".into(),
            ));
        }
        let reason = payload
            .suspend_reason
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or(Some("Suspended from report resolution"));
        sqlx::query(
            "UPDATE users SET is_suspended = TRUE, suspended_reason = $1, suspended_at = now() WHERE id = $2",
        )
        .bind(reason)
        .bind(existing.target_id)
        .execute(&state.pool)
        .await?;
        crate::audit::log(
            &state,
            admin.id,
            "user.suspend",
            "user",
            Some(existing.target_id),
            json!({"from_report": id, "reason": reason}),
        )
        .await;
    }

    if payload.delete_user && status == "resolved" && existing.target_type == "user" {
        if existing.target_id == admin.id {
            return Err(AppError::BadRequest(
                "You cannot delete your own account".into(),
            ));
        }
        let res = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(existing.target_id)
            .execute(&state.pool)
            .await?;
        if res.rows_affected() > 0 {
            crate::audit::log(
                &state,
                admin.id,
                "user.delete",
                "user",
                Some(existing.target_id),
                json!({"from_report": id}),
            )
            .await;
        }
    }

    let updated = sqlx::query(
        "UPDATE reports SET status = $1, admin_note = $2, resolved_by = $3, resolved_at = now() \
         WHERE id = $4",
    )
    .bind(&status)
    .bind(&note)
    .bind(admin.id)
    .bind(id)
    .execute(&state.pool)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::NotFound("Report not found".into()));
    }

    let row =
        sqlx::query_as::<_, AdminReportRow>(&format!("{ADMIN_REPORT_SELECT} WHERE r.id = $1"))
            .bind(id)
            .fetch_one(&state.pool)
            .await?;

    crate::audit::log(
        &state,
        admin.id,
        if status == "resolved" {
            "report.resolve"
        } else {
            "report.dismiss"
        },
        "report",
        Some(id),
        json!({
            "note": note,
            "target_type": row.target_type,
            "target_id": row.target_id,
            "suspend_user": payload.suspend_user,
            "delete_target": payload.delete_target,
            "delete_user": payload.delete_user
        }),
    )
    .await;

    Ok(Json(AdminReportOut::from(row)))
}

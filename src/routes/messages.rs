use axum::{
    http::HeaderMap,
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{
    auth::CurrentUser,
    error::{AppError, AppResult},
    models::{
        ConversationListRow, ConversationOut, MessageBrief, MessageOut, MessageRow,
        StartConversationRequest, UserBrief,
    },
    AppState,
};

const CONVERSATION_SELECT: &str = "SELECT c.id, \
     ou.id AS other_id, ou.username AS other_username, ou.bio AS other_bio, ou.avatar_file AS other_avatar, \
     lm.body AS last_body, lm.created_at AS last_created_at, lm.sender_id AS last_sender_id, \
     (SELECT COUNT(*) FROM messages m \
      WHERE m.conversation_id = c.id AND m.sender_id <> $1 AND m.read_at IS NULL) AS unread_count, \
     GREATEST(c.created_at, COALESCE(lm.created_at, c.created_at)) AS updated_at, \
     EXISTS(SELECT 1 FROM user_blocks b WHERE b.blocker_id = $1 AND b.blocked_id = ou.id) AS is_blocked_by_me, \
     EXISTS(SELECT 1 FROM user_blocks b WHERE b.blocker_id = ou.id AND b.blocked_id = $1) AS is_blocked_by_them \
     FROM conversations c \
     JOIN users ou ON ou.id = CASE WHEN c.user_a = $1 THEN c.user_b ELSE c.user_a END \
     LEFT JOIN LATERAL ( \
       SELECT m.body, m.created_at, m.sender_id FROM messages m \
       WHERE m.conversation_id = c.id ORDER BY m.id DESC LIMIT 1 \
     ) lm ON true";

fn conversation_out(row: ConversationListRow) -> ConversationOut {
    ConversationOut {
        id: row.id,
        other_user: UserBrief {
            id: row.other_id,
            username: row.other_username,
            bio: row.other_bio,
            avatar_url: row.other_avatar.map(|f| format!("/media/avatars/{f}")),
        },
        last_message: match (row.last_body, row.last_created_at, row.last_sender_id) {
            (Some(body), Some(created_at), Some(sender_id)) => Some(MessageBrief {
                body,
                sender_id,
                created_at,
            }),
            _ => None,
        },
        unread_count: row.unread_count,
        updated_at: row.updated_at,
        is_blocked_by_me: row.is_blocked_by_me,
        is_blocked_by_them: row.is_blocked_by_them,
    }
}

#[utoipa::path(
    get,
    path = "/api/conversations",
    tag = "messages",
    security(("cookie_auth" = [])),
    responses(
        (status = 200, description = "My conversations, newest activity first", body = Vec<ConversationOut>),
        (status = 401, description = "Not authenticated", body = crate::models::Message),
    )
)]
pub async fn list_conversations(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
) -> AppResult<Json<Vec<ConversationOut>>> {
    let rows = sqlx::query_as::<_, ConversationListRow>(&format!(
        "{CONVERSATION_SELECT} WHERE c.user_a = $1 OR c.user_b = $1 \
         ORDER BY updated_at DESC, c.id DESC"
    ))
    .bind(current.id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows.into_iter().map(conversation_out).collect()))
}

#[utoipa::path(
    post,
    path = "/api/conversations",
    tag = "messages",
    security(("cookie_auth" = [])),
    request_body = StartConversationRequest,
    responses(
        (status = 200, description = "Conversation with that user (created if needed)", body = ConversationOut),
        (status = 400, description = "Cannot message yourself", body = crate::models::Message),
        (status = 404, description = "User not found", body = crate::models::Message),
    )
)]
pub async fn start_conversation(
    State(state): State<AppState>,
    headers: HeaderMap,
    CurrentUser(current): CurrentUser,
    Json(payload): Json<StartConversationRequest>,
) -> AppResult<Json<ConversationOut>> {
    if !crate::rate_limit::check_ip(&headers, "interact", 40, 60) {
        return Err(AppError::TooManyRequests("Too many requests, please try again later.".into()));
    }
    if payload.user_id == current.id {
        return Err(AppError::BadRequest("Cannot message yourself".into()));
    }
    let target = sqlx::query_scalar::<_, i32>("SELECT id FROM users WHERE id = $1")
        .bind(payload.user_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    if crate::routes::blocks::is_blocked_either(&state.pool, current.id, target).await? {
        return Err(AppError::Forbidden("You cannot message this user".into()));
    }

    let (a, b) = if current.id < target {
        (current.id, target)
    } else {
        (target, current.id)
    };
    sqlx::query(
        "INSERT INTO conversations (user_a, user_b) VALUES ($1, $2) \
         ON CONFLICT (user_a, user_b) DO NOTHING",
    )
    .bind(a)
    .bind(b)
    .execute(&state.pool)
    .await?;

    let row = sqlx::query_as::<_, ConversationListRow>(&format!(
        "{CONVERSATION_SELECT} WHERE c.user_a = $1 AND c.user_b = $2"
    ))
    .bind(a)
    .bind(b)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(conversation_out(row)))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct HistoryQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
}

fn default_page() -> u32 {
    1
}

fn default_per_page() -> u32 {
    50
}

#[utoipa::path(
    get,
    path = "/api/conversations/{conversation_id}/messages",
    tag = "messages",
    security(("cookie_auth" = [])),
    params(("conversation_id" = i32, Path, description = "Conversation id"), HistoryQuery),
    responses(
        (status = 200, description = "Message history page, newest first", body = Vec<MessageOut>),
        (status = 403, description = "Not a participant", body = crate::models::Message),
        (status = 404, description = "Conversation not found", body = crate::models::Message),
    )
)]
pub async fn list_messages(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(conversation_id): Path<i32>,
    Query(query): Query<HistoryQuery>,
) -> AppResult<Json<Vec<MessageOut>>> {
    // membership check
    let row: Option<(i32, i32)> =
        sqlx::query_as("SELECT user_a, user_b FROM conversations WHERE id = $1")
            .bind(conversation_id)
            .fetch_optional(&state.pool)
            .await?;
    match row {
        None => return Err(AppError::NotFound("Conversation not found".into())),
        Some((a, b)) if a != current.id && b != current.id => {
            return Err(AppError::Forbidden(
                "Not a participant in this conversation".into(),
            ))
        }
        _ => {}
    }

    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 200);
    let offset = (page - 1) as i64 * per_page as i64;
    let rows = sqlx::query_as::<_, MessageRow>(
        "SELECT id, conversation_id, sender_id, body, read_at, delivered_at, created_at \
         FROM messages WHERE conversation_id = $1 \
         ORDER BY id DESC LIMIT $2 OFFSET $3",
    )
    .bind(conversation_id)
    .bind(per_page as i64)
    .bind(offset)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows.into_iter().map(MessageOut::from).collect()))
}

#[utoipa::path(
    post,
    path = "/api/conversations/{conversation_id}/read",
    tag = "messages",
    security(("cookie_auth" = [])),
    params(("conversation_id" = i32, Path, description = "Conversation id")),
    responses(
        (status = 204, description = "Marked read"),
        (status = 403, description = "Not a participant", body = crate::models::Message),
        (status = 404, description = "Conversation not found", body = crate::models::Message),
    )
)]
pub async fn mark_read(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(conversation_id): Path<i32>,
) -> AppResult<axum::http::StatusCode> {
    let row: Option<(i32, i32)> =
        sqlx::query_as("SELECT user_a, user_b FROM conversations WHERE id = $1")
            .bind(conversation_id)
            .fetch_optional(&state.pool)
            .await?;
    let (a, b) = match row {
        None => return Err(AppError::NotFound("Conversation not found".into())),
        Some(pair) => pair,
    };
    if a != current.id && b != current.id {
        return Err(AppError::Forbidden(
            "Not a participant in this conversation".into(),
        ));
    }
    let result = sqlx::query(
        "UPDATE messages SET read_at = now() \
         WHERE conversation_id = $1 AND sender_id <> $2 AND read_at IS NULL",
    )
    .bind(conversation_id)
    .bind(current.id)
    .execute(&state.pool)
    .await?;
    // let the other side's UI flip to "read" live
    if result.rows_affected() > 0 {
        let peer = if a == current.id { b } else { a };
        state
            .hub
            .send_to(peer, serde_json::json!({"type":"read","conversation_id":conversation_id}));
    }
    Ok(axum::http::StatusCode::NO_CONTENT)
}

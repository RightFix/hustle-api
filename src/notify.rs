use serde_json::json;

use crate::{
    models::{NotificationOut, NotificationRow},
    AppState,
};

/// Record a notification for `user_id` and push it over the websocket hub.
///
/// Deduplicates: an unread notification with the same (user_id, actor_id,
/// kind, ref_id) is not created twice — so re-following or a burst of
/// messages doesn't spam the bell. Never notifies about your own actions.
#[tracing::instrument(skip_all, fields(user_id, actor_id, kind))]
pub async fn notify(
    state: &AppState,
    user_id: i32,
    actor_id: i32,
    kind: &str,
    ref_type: Option<&str>,
    ref_id: Option<i32>,
) {
    if user_id == actor_id {
        return;
    }
    let row = sqlx::query_as::<_, NotificationRow>(
        "INSERT INTO notifications (user_id, actor_id, kind, ref_type, ref_id) \
         SELECT $1, $2, $3, $4, $5 \
         WHERE NOT EXISTS ( \
           SELECT 1 FROM notifications \
           WHERE user_id = $1 AND actor_id = $2 AND kind = $3 \
             AND ref_id IS NOT DISTINCT FROM $5 AND read_at IS NULL \
         ) \
         RETURNING id, actor_id, kind, ref_type, ref_id, created_at, read_at",
    )
    .bind(user_id)
    .bind(actor_id)
    .bind(kind)
    .bind(ref_type)
    .bind(ref_id)
    .fetch_optional(&state.pool)
    .await;

    let notification = match row {
        Ok(Some(row)) => match fetch_out(state, row).await {
            Ok(out) => out,
            Err(e) => {
                tracing::error!(error = %e, "notification row missing actor");
                return;
            }
        },
        Ok(None) => return, // deduped away
        Err(e) => {
            tracing::error!(error = %e, "failed to insert notification");
            return;
        }
    };

    state.hub.send_to(
        user_id,
        json!({"type": "notification", "notification": notification}),
    );
}

/// Load a notification row joined with its actor's profile.
pub async fn fetch_out(
    state: &AppState,
    row: NotificationRow,
) -> Result<NotificationOut, sqlx::Error> {
    let actor = sqlx::query_as::<_, (i32, String, Option<String>, Option<String>)>(
        "SELECT id, username, bio, avatar_file FROM users WHERE id = $1",
    )
    .bind(row.actor_id)
    .fetch_one(&state.pool)
    .await?;
    Ok(NotificationOut {
        id: row.id,
        kind: row.kind,
        actor: crate::models::UserBrief {
            id: actor.0,
            username: actor.1,
            bio: actor.2,
            avatar_url: actor.3.map(|f| format!("/media/avatars/{f}")),
        },
        ref_type: row.ref_type,
        ref_id: row.ref_id,
        created_at: row.created_at,
        read_at: row.read_at,
    })
}

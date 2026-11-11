use serde_json::Value;

use crate::AppState;

/// Best-effort admin audit log. Failures are logged but do not break the request.
pub async fn log(
    state: &AppState,
    actor_id: i32,
    action: &str,
    target_type: &str,
    target_id: Option<i32>,
    metadata: Value,
) {
    let res = sqlx::query(
        "INSERT INTO admin_audit_logs (actor_id, action, target_type, target_id, metadata) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(actor_id)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(metadata)
    .execute(&state.pool)
    .await;
    if let Err(e) = res {
        tracing::warn!(error = %e, action, target_type, "failed to write admin audit log");
    }
}

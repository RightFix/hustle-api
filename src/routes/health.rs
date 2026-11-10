use axum::Json;
use serde_json::{json, Value};

#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    responses((status = 200, description = "Service is up")),
)]
pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

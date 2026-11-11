use std::path::{Path, PathBuf};

use axum::{
    extract::{Multipart, State},
    Json,
};

use crate::{
    auth::CurrentUser,
    error::{AppError, AppResult},
    models::{UserOut, UserRow},
    AppState,
};

const MAX_AVATAR_BYTES: usize = 10 * 1024 * 1024;
const MAX_AVATAR_DIMENSION: u32 = 512;

fn avatar_dir(state: &AppState) -> PathBuf {
    Path::new(&state.config.upload_dir).join("avatars")
}

#[utoipa::path(
    post,
    path = "/api/users/me/avatar",
    tag = "users",
    security(("cookie_auth" = [])),
    request_body(
        content_type = "multipart/form-data",
        description = "Multipart: 'file' containing a JPEG, PNG or WebP image"
    ),
    responses(
        (status = 200, description = "Avatar uploaded (shrunk to max 512px)", body = UserOut),
        (status = 400, description = "Missing or invalid file field", body = crate::models::Message),
        (status = 413, description = "File too large", body = crate::models::Message),
        (status = 422, description = "Unsupported image", body = crate::models::Message),
    )
)]
pub async fn upload_avatar(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    mut multipart: Multipart,
) -> AppResult<Json<UserOut>> {
    let mut file_bytes: Option<Vec<u8>> = None;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Invalid multipart body: {e}")))?
    {
        if field.name() != Some("file") || field.file_name().is_none() {
            continue;
        }
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| AppError::BadRequest(format!("Invalid multipart body: {e}")))?
        {
            if buf.len() + chunk.len() > MAX_AVATAR_BYTES + 1 {
                return Err(AppError::PayloadTooLarge(
                    "Images must be 10MB or smaller".into(),
                ));
            }
            buf.extend_from_slice(&chunk);
        }
        if buf.is_empty() {
            return Err(AppError::BadRequest("Empty file".into()));
        }
        file_bytes = Some(buf);
    }

    let Some(raw) = file_bytes else {
        return Err(AppError::BadRequest(
            "Missing 'file' field (multipart/form-data)".into(),
        ));
    };

    let image = crate::images::process_jpeg_image(&raw, MAX_AVATAR_DIMENSION)?;
    let dir = avatar_dir(&state);
    tokio::fs::create_dir_all(&dir).await.map_err(|e| {
        tracing::error!(error = %e, "failed to create avatar dir");
        AppError::Internal
    })?;

    let file_name = format!("{}.jpg", uuid::Uuid::new_v4());
    tokio::fs::write(dir.join(&file_name), &image.bytes)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to write avatar");
            AppError::Internal
        })?;

    let previous =
        sqlx::query_scalar::<_, Option<String>>("SELECT avatar_file FROM users WHERE id = $1")
            .bind(current.id)
            .fetch_one(&state.pool)
            .await?;

    sqlx::query("UPDATE users SET avatar_file = $1 WHERE id = $2")
        .bind(&file_name)
        .bind(current.id)
        .execute(&state.pool)
        .await?;

    // Delete the old avatar only after the new one is persisted.
    if let Some(previous) = previous {
        let _ = tokio::fs::remove_file(dir.join(previous)).await;
    }

    current_user(&state, current.id).await.map(Json)
}

#[utoipa::path(
    delete,
    path = "/api/users/me/avatar",
    tag = "users",
    security(("cookie_auth" = [])),
    responses(
        (status = 200, description = "Avatar removed", body = UserOut),
    )
)]
pub async fn delete_avatar(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
) -> AppResult<Json<UserOut>> {
    let previous =
        sqlx::query_scalar::<_, Option<String>>("SELECT avatar_file FROM users WHERE id = $1")
            .bind(current.id)
            .fetch_one(&state.pool)
            .await?;

    sqlx::query("UPDATE users SET avatar_file = NULL WHERE id = $1")
        .bind(current.id)
        .execute(&state.pool)
        .await?;

    if let Some(previous) = previous {
        let _ = tokio::fs::remove_file(avatar_dir(&state).join(previous)).await;
    }

    current_user(&state, current.id).await.map(Json)
}

async fn current_user(state: &AppState, id: i32) -> AppResult<UserOut> {
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT id, email, username, hashed_password, bio, is_admin, account_type, \
                location, availability, expected_rate, languages, avatar_file, \
                email_verified_at, email_messages_enabled, created_at \
         FROM users WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.pool)
    .await?;
    Ok(UserOut::from(user))
}

use std::path::{Path, PathBuf};

use axum::{
    extract::{Multipart, Path as AxumPath, State},
    Json,
};

use crate::{
    auth::CurrentUser,
    error::{AppError, AppResult},
    models::{GalleryItemOut, GalleryItemRow},
    AppState,
};

const MAX_ITEMS_PER_USER: i64 = 30;
const MAX_VIDEO_BYTES: usize = 50 * 1024 * 1024;
const MAX_IMAGE_DIMENSION: u32 = 1600;
const MAX_VIDEO_SECONDS: u32 = 60;
const MAX_CAPTION_LEN: usize = 200;

struct Processed {
    bytes: Vec<u8>,
    kind: &'static str,
    ext: &'static str,
    mime: &'static str,
    width: Option<i32>,
    height: Option<i32>,
    duration_seconds: Option<i32>,
}

fn gallery_dir(state: &AppState) -> PathBuf {
    Path::new(&state.config.upload_dir).join("gallery")
}

// ----- image pipeline -----

fn process_image(raw: &[u8]) -> AppResult<Processed> {
    let image = crate::images::process_jpeg_image(raw, MAX_IMAGE_DIMENSION)?;
    Ok(Processed {
        bytes: image.bytes,
        kind: "image",
        ext: "jpg",
        mime: "image/jpeg",
        width: Some(image.width as i32),
        height: Some(image.height as i32),
        duration_seconds: None,
    })
}

// ----- video pipeline -----

fn process_video(raw: &[u8]) -> AppResult<Processed> {
    if raw.len() > MAX_VIDEO_BYTES {
        return Err(AppError::PayloadTooLarge(format!(
            "Videos must be {MAX_VIDEO_BYTES} bytes or smaller"
        )));
    }
    // ISO BMFF (MP4): bytes 4..8 == "ftyp"
    if raw.len() >= 12 && raw[4..8] == *b"ftyp" {
        if let Some(secs) = mp4_duration_seconds(raw) {
            if secs > MAX_VIDEO_SECONDS {
                return Err(AppError::Unprocessable(format!(
                    "Videos must be {MAX_VIDEO_SECONDS} seconds or shorter"
                )));
            }
        }
        return Ok(Processed {
            bytes: raw.to_vec(),
            kind: "video",
            ext: "mp4",
            mime: "video/mp4",
            width: None,
            height: None,
            duration_seconds: secs_i32(mp4_duration_seconds(raw)),
        });
    }
    // EBML (WebM)
    if raw.len() >= 4 && raw[..4] == [0x1A, 0x45, 0xDF, 0xA3] {
        return Ok(Processed {
            bytes: raw.to_vec(),
            kind: "video",
            ext: "webm",
            mime: "video/webm",
            width: None,
            height: None,
            duration_seconds: None,
        });
    }
    Err(AppError::Unprocessable(
        "Only MP4 or WebM videos are supported".into(),
    ))
}

fn secs_i32(secs: Option<u32>) -> Option<i32> {
    secs.map(|s| s.min(i32::MAX as u32) as i32)
}

/// Locate the mvhd box and read timescale/duration. Returns None when the
/// file doesn't contain a parsable mvhd (callers then rely on size limits).
fn mp4_duration_seconds(buf: &[u8]) -> Option<u32> {
    let pos = find_bytes(buf, b"mvhd")?;
    let start = pos + 4; // skip "mvhd" fourcc
    let version = *buf.get(start)?;
    if version == 0 {
        let timescale = be_u32(buf, start + 12)?;
        let duration = be_u32(buf, start + 16)?;
        if timescale == 0 {
            return None;
        }
        Some(duration / timescale)
    } else if version == 1 {
        let timescale = be_u32(buf, start + 20)?;
        let duration = be_u64(buf, start + 24)?;
        if timescale == 0 {
            return None;
        }
        Some((duration / timescale as u64) as u32)
    } else {
        None
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn be_u32(buf: &[u8], at: usize) -> Option<u32> {
    buf.get(at..at + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn be_u64(buf: &[u8], at: usize) -> Option<u64> {
    buf.get(at..at + 8)
        .map(|b| u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
}

// ----- handlers -----

#[utoipa::path(
    post,
    path = "/api/users/me/gallery",
    tag = "gallery",
    security(("cookie_auth" = [])),
    request_body(
        content_type = "multipart/form-data",
        description = "Multipart: 'file' (image or video) and optional 'caption'"
    ),
    responses(
        (status = 201, description = "Uploaded (images are shrunk to max 1600px and re-encoded as JPEG)", body = GalleryItemOut),
        (status = 401, description = "Not authenticated", body = crate::models::Message),
        (status = 409, description = "Gallery limit reached", body = crate::models::Message),
        (status = 413, description = "File too large", body = crate::models::Message),
        (status = 422, description = "Unsupported or invalid media", body = crate::models::Message),
    )
)]
pub async fn upload_gallery_item(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    mut multipart: Multipart,
) -> AppResult<(axum::http::StatusCode, Json<GalleryItemOut>)> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM gallery_items WHERE user_id = $1")
        .bind(current.id)
        .fetch_one(&state.pool)
        .await?;
    if count >= MAX_ITEMS_PER_USER {
        return Err(AppError::Conflict(format!(
            "Gallery limit of {MAX_ITEMS_PER_USER} items reached — delete something first"
        )));
    }

    // Collect fields (field order is client-controlled).
    let mut file_bytes: Option<(Vec<u8>, Option<String>)> = None;
    let mut caption: Option<String> = None;
    while let Some(mut field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Invalid multipart body: {e}")))?
    {
        let name = field.name().map(str::to_owned);
        let has_file = field.file_name().is_some();
        match name.as_deref() {
            Some("file") if has_file => {
                let mut buf: Vec<u8> = Vec::new();
                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Invalid multipart body: {e}")))?
                {
                    if buf.len() + chunk.len() > MAX_VIDEO_BYTES + 1 {
                        return Err(AppError::PayloadTooLarge(format!(
                            "File must be {} bytes or smaller",
                            MAX_VIDEO_BYTES
                        )));
                    }
                    buf.extend_from_slice(&chunk);
                }
                if buf.is_empty() {
                    return Err(AppError::BadRequest("Empty file".into()));
                }
                file_bytes = Some((buf, field.file_name().map(str::to_owned)));
            }
            Some("caption") => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Invalid caption: {e}")))?;
                let text = text.trim().to_owned();
                if !text.is_empty() {
                    caption = Some(text);
                }
            }
            _ => {} // ignore unknown fields (e.g. extra file parts)
        }
    }

    let Some((raw, _original_name)) = file_bytes else {
        return Err(AppError::BadRequest(
            "Missing 'file' field (multipart/form-data)".into(),
        ));
    };
    if let Some(c) = &caption {
        if c.chars().count() > MAX_CAPTION_LEN {
            return Err(AppError::Unprocessable(format!(
                "Caption must be {MAX_CAPTION_LEN} characters or fewer"
            )));
        }
    }

    let is_image = image::guess_format(&raw).is_ok();
    let processed = if is_image {
        process_image(&raw)?
    } else {
        process_video(&raw)?
    };

    // Write to disk first, then record; clean up on DB failure.
    let dir = gallery_dir(&state);
    tokio::fs::create_dir_all(&dir).await.map_err(|e| {
        tracing::error!(error = %e, "failed to create upload dir");
        AppError::Internal
    })?;
    let file_name = format!("{}.{}", uuid::Uuid::new_v4(), processed.ext);
    let path = dir.join(&file_name);
    tokio::fs::write(&path, &processed.bytes)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to write uploaded file");
            AppError::Internal
        })?;

    let row = sqlx::query_as::<_, GalleryItemRow>(
        "INSERT INTO gallery_items \
         (user_id, kind, file_name, mime_type, byte_size, width, height, duration_seconds, caption) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) \
         RETURNING id, user_id, kind, file_name, mime_type, byte_size, width, height, \
                   duration_seconds, caption, created_at",
    )
    .bind(current.id)
    .bind(processed.kind)
    .bind(&file_name)
    .bind(processed.mime)
    .bind(processed.bytes.len() as i32)
    .bind(processed.width)
    .bind(processed.height)
    .bind(processed.duration_seconds)
    .bind(&caption)
    .fetch_one(&state.pool)
    .await;

    let row = match row {
        Ok(row) => row,
        Err(e) => {
            tracing::error!(error = %e, "gallery insert failed");
            let _ = tokio::fs::remove_file(&path).await;
            return Err(AppError::Internal);
        }
    };

    Ok((axum::http::StatusCode::CREATED, Json(row.into_out())))
}

#[utoipa::path(
    get,
    path = "/api/users/{user_id}/gallery",
    tag = "gallery",
    security(("cookie_auth" = [])),
    params(("user_id" = i32, Path, description = "User id")),
    responses(
        (status = 200, description = "Gallery items, newest first", body = Vec<GalleryItemOut>),
        (status = 404, description = "User not found", body = crate::models::Message),
    )
)]
pub async fn list_gallery(
    State(state): State<AppState>,
    CurrentUser(_current): CurrentUser,
    AxumPath(user_id): AxumPath<i32>,
) -> AppResult<Json<Vec<GalleryItemOut>>> {
    let exists = sqlx::query_scalar::<_, i32>("SELECT id FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("User not found".into()));
    }
    let rows = sqlx::query_as::<_, GalleryItemRow>(
        "SELECT id, user_id, kind, file_name, mime_type, byte_size, width, height, \
                duration_seconds, caption, created_at \
         FROM gallery_items WHERE user_id = $1 ORDER BY id DESC",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(
        rows.into_iter().map(GalleryItemRow::into_out).collect(),
    ))
}

#[utoipa::path(
    delete,
    path = "/api/gallery/{item_id}",
    tag = "gallery",
    security(("cookie_auth" = [])),
    params(("item_id" = i32, Path, description = "Gallery item id")),
    responses(
        (status = 204, description = "Deleted (file removed from disk)"),
        (status = 404, description = "Not found or not yours", body = crate::models::Message),
    )
)]
pub async fn delete_gallery_item(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    AxumPath(item_id): AxumPath<i32>,
) -> AppResult<axum::http::StatusCode> {
    let file_name = sqlx::query_scalar::<_, String>(
        "DELETE FROM gallery_items WHERE id = $1 AND user_id = $2 RETURNING file_name",
    )
    .bind(item_id)
    .bind(current.id)
    .fetch_optional(&state.pool)
    .await?;
    let Some(file_name) = file_name else {
        return Err(AppError::NotFound("Gallery item not found".into()));
    };
    // Best-effort file cleanup — an orphaned file is harmless.
    let _ = tokio::fs::remove_file(gallery_dir(&state).join(&file_name)).await;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mvhd_version0_duration() {
        // Minimal mvhd payload: version(1) + flags(3), creation, modification,
        // timescale, duration — all big-endian.
        let mut buf = vec![0u8]; // version 0
        buf.extend_from_slice(&[0, 0, 0]); // flags (3 bytes)
        buf.extend_from_slice(&0u32.to_be_bytes()); // creation
        buf.extend_from_slice(&0u32.to_be_bytes()); // modification
        buf.extend_from_slice(&1000u32.to_be_bytes()); // timescale
        buf.extend_from_slice(&120_000u32.to_be_bytes()); // duration -> 120s
        let mut full = b"xxxxmvhd".to_vec();
        full.extend_from_slice(&buf);
        assert_eq!(mp4_duration_seconds(&full), Some(120));
    }

    #[test]
    fn mvhd_missing_is_none() {
        assert_eq!(mp4_duration_seconds(b"no box here"), None);
    }
}

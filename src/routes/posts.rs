use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use utoipa::IntoParams;

use crate::{
    auth::CurrentUser,
    error::{AppError, AppResult},
    models::{CreatePostRequest, PaginatedPosts, PostOut, PostRow},
    AppState,
};

#[derive(Debug, Deserialize, IntoParams)]
pub struct FeedQuery {
    /// Only include posts from followed users.
    #[serde(default)]
    pub following: bool,
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
    post,
    path = "/api/posts",
    tag = "posts",
    security(("cookie_auth" = [])),
    request_body = CreatePostRequest,
    responses(
        (status = 201, description = "Post created", body = PostOut),
        (status = 400, description = "Empty content or over 280 characters", body = crate::models::Message),
    )
)]
pub async fn create_post(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Json(payload): Json<CreatePostRequest>,
) -> AppResult<(StatusCode, Json<PostOut>)> {
    let content = payload.content.trim().to_owned();
    if content.is_empty() {
        return Err(AppError::BadRequest("Post content cannot be empty".into()));
    }
    if content.len() > 280 {
        return Err(AppError::BadRequest("Post exceeds 280 characters".into()));
    }
    let post = sqlx::query_as::<_, PostRow>(
        "INSERT INTO posts (author_id, content) VALUES ($1, $2) \
         RETURNING id, author_id, content, created_at",
    )
    .bind(current.id)
    .bind(&content)
    .fetch_one(&state.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(PostOut::from(post))))
}

#[utoipa::path(
    get,
    path = "/api/posts/feed",
    tag = "posts",
    security(("cookie_auth" = [])),
    params(FeedQuery),
    responses((status = 200, description = "Feed page, newest first", body = PaginatedPosts)),
)]
pub async fn read_feed(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Query(query): Query<FeedQuery>,
) -> AppResult<Json<PaginatedPosts>> {
    let page = query.page.max(1);
    let per_page = query.per_page.clamp(1, 100);
    let offset = (page - 1) as i64 * per_page as i64;

    let total = if query.following {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM posts p \
             WHERE p.author_id IN (SELECT following_id FROM follows WHERE follower_id = $1)",
        )
        .bind(current.id)
        .fetch_one(&state.pool)
        .await?
    } else {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM posts")
            .fetch_one(&state.pool)
            .await?
    };

    let posts = if query.following {
        sqlx::query_as::<_, PostRow>(
            "SELECT p.id, p.author_id, p.content, p.created_at FROM posts p \
             WHERE p.author_id IN (SELECT following_id FROM follows WHERE follower_id = $1) \
             ORDER BY p.created_at DESC, p.id DESC LIMIT $2 OFFSET $3",
        )
        .bind(current.id)
        .bind(per_page as i64)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as::<_, PostRow>(
            "SELECT id, author_id, content, created_at FROM posts \
             ORDER BY created_at DESC, id DESC LIMIT $1 OFFSET $2",
        )
        .bind(per_page as i64)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?
    };
    Ok(Json(PaginatedPosts {
        items: posts.into_iter().map(PostOut::from).collect(),
        total,
        page,
        per_page,
    }))
}

#[utoipa::path(
    get,
    path = "/api/posts/{post_id}",
    tag = "posts",
    security(("cookie_auth" = [])),
    params(("post_id" = i32, Path, description = "Post id")),
    responses(
        (status = 200, description = "Post", body = PostOut),
        (status = 404, description = "Post not found", body = crate::models::Message),
    )
)]
pub async fn read_post(
    State(state): State<AppState>,
    Path(post_id): Path<i32>,
) -> AppResult<Json<PostOut>> {
    let post = sqlx::query_as::<_, PostRow>(
        "SELECT id, author_id, content, created_at FROM posts WHERE id = $1",
    )
    .bind(post_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Post not found".into()))?;
    Ok(Json(PostOut::from(post)))
}

#[utoipa::path(
    delete,
    path = "/api/posts/{post_id}",
    tag = "posts",
    security(("cookie_auth" = [])),
    params(("post_id" = i32, Path, description = "Post id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 403, description = "Not the author", body = crate::models::Message),
        (status = 404, description = "Post not found", body = crate::models::Message),
    )
)]
pub async fn delete_post(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
    Path(post_id): Path<i32>,
) -> AppResult<StatusCode> {
    let post = sqlx::query_as::<_, PostRow>(
        "SELECT id, author_id, content, created_at FROM posts WHERE id = $1",
    )
    .bind(post_id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Post not found".into()))?;
    if post.author_id != current.id {
        return Err(AppError::Forbidden(
            "Not allowed to delete this post".into(),
        ));
    }
    sqlx::query("DELETE FROM posts WHERE id = $1")
        .bind(post_id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/posts/user/{user_id}",
    tag = "posts",
    security(("cookie_auth" = [])),
    params(("user_id" = i32, Path, description = "Author id")),
    responses(
        (status = 200, description = "Author's posts, newest first", body = Vec<PostOut>),
        (status = 404, description = "User not found", body = crate::models::Message),
    )
)]
pub async fn read_user_posts(
    State(state): State<AppState>,
    Path(user_id): Path<i32>,
) -> AppResult<Json<Vec<PostOut>>> {
    let author = sqlx::query_scalar::<_, i32>("SELECT id FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?;
    if author.is_none() {
        return Err(AppError::NotFound("User not found".into()));
    }
    let posts = sqlx::query_as::<_, PostRow>(
        "SELECT id, author_id, content, created_at FROM posts \
         WHERE author_id = $1 ORDER BY created_at DESC",
    )
    .bind(user_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(posts.into_iter().map(PostOut::from).collect()))
}

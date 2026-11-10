use axum::{extract::{Query, State}, http::StatusCode, Json};
use serde::Deserialize;
use utoipa::IntoParams;
use axum_extra::extract::{
    cookie::{Cookie, SameSite},
    CookieJar,
};
use chrono::{Duration, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use validator::ValidateEmail;

use crate::{
    auth::{self, COOKIE_NAME, CurrentUser},
    email,
    error::{AppError, AppResult},
    models::{
        ForgotPasswordRequest, LoginRequest, Message, RegisterRequest, ResetPasswordRequest,
        Token, UserOut, UserRow,
    },
    AppState,
};

fn auth_cookie(token: String, secure: bool, max_age_minutes: i64) -> Cookie<'static> {
    Cookie::build((COOKIE_NAME, token))
        .http_only(true)
        .same_site(SameSite::Lax)
        .secure(secure)
        .path("/")
        .max_age(time::Duration::minutes(max_age_minutes))
        .build()
}

fn clear_auth_cookie() -> Cookie<'static> {
    Cookie::build(COOKIE_NAME).path("/").build()
}

/// Best-effort outbound email: registration and verification endpoints should
/// survive an SMTP outage, but we do want it logged.
async fn send_verification(state: &AppState, user: &UserRow) {
    match auth::create_verification_token(user.id, &state.config.secret_key) {
        Ok(token) => {
            if let Err(err) = email::send_verification_email(&state.config, &user.email, &token).await {
                tracing::warn!(error = %err, user_id = user.id, "verification email failed");
            }
        }
        Err(err) => tracing::warn!(error = %err, user_id = user.id, "verification token failed"),
    }
}

#[utoipa::path(
    post,
    path = "/api/auth/register",
    tag = "auth",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "User registered", body = UserOut),
        (status = 409, description = "Email or username already registered", body = Message),
        (status = 422, description = "Invalid email address", body = Message),
    )
)]
pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> AppResult<(StatusCode, Json<UserOut>)> {
    if !payload.email.validate_email() {
        return Err(AppError::Unprocessable("Invalid email address".into()));
    }
    let existing = sqlx::query_scalar::<_, i32>(
        "SELECT id FROM users WHERE email = $1 OR username = $2",
    )
    .bind(&payload.email)
    .bind(&payload.username)
    .fetch_optional(&state.pool)
    .await?;
    if existing.is_some() {
        return Err(AppError::Conflict(
            "Email or username already registered".into(),
        ));
    }

    let hashed = auth::hash_password(payload.password).await?;
    let account_type = match payload.account_type.as_deref() {
        Some("hirer") => "hirer",
        _ => "worker",
    };
    let user = sqlx::query_as::<_, UserRow>(
        "INSERT INTO users (email, username, hashed_password, account_type) VALUES ($1, $2, $3, $4) \
         RETURNING id, email, username, hashed_password, bio, is_admin, account_type, location, availability, expected_rate, languages, avatar_file, email_verified_at, email_messages_enabled, is_suspended, suspended_reason, suspended_at, created_at",
    )
    .bind(&payload.email)
    .bind(&payload.username)
    .bind(&hashed)
    .bind(account_type)
    .fetch_one(&state.pool)
    .await?;
    send_verification(&state, &user).await;

    Ok((StatusCode::CREATED, Json(UserOut::from(user))))
}

#[utoipa::path(
    post,
    path = "/api/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Logged in; session cookie set", body = Token),
        (status = 401, description = "Incorrect email or password", body = Message),
    )
)]
pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<LoginRequest>,
) -> AppResult<(CookieJar, Json<Token>)> {
    let user = sqlx::query_as::<_, UserRow>(
        "SELECT id, email, username, hashed_password, bio, is_admin, account_type, location, availability, expected_rate, languages, avatar_file, email_verified_at, email_messages_enabled, is_suspended, suspended_reason, suspended_at, created_at FROM users WHERE email = $1",
    )
    .bind(&payload.email)
    .fetch_optional(&state.pool)
    .await?;
    let valid = match &user {
        Some(u) => {
            auth::verify_password(payload.password, u.hashed_password.clone()).await?
        }
        None => false,
    };
    let user = match (user, valid) {
        (Some(u), true) => u,
        _ => {
            return Err(AppError::Unauthorized(
                "Incorrect email or password".into(),
            ))
        }
    };
    if user.is_suspended {
        let msg = user
            .suspended_reason
            .clone()
            .filter(|r| !r.trim().is_empty())
            .unwrap_or_else(|| "Your account has been suspended".into());
        return Err(AppError::Forbidden(msg));
    }

    let token = auth::create_access_token(
        user.id,
        &state.config.secret_key,
        state.config.access_token_expire_minutes,
    )?;
    let jar = jar.add(auth_cookie(
        token.clone(),
        state.config.is_production(),
        state.config.access_token_expire_minutes,
    ));
    Ok((jar, Json(Token::bearer(token))))
}

#[utoipa::path(
    post,
    path = "/api/auth/logout",
    tag = "auth",
    responses((status = 204, description = "Logged out; session cookie cleared")),
)]
pub async fn logout(jar: CookieJar) -> (CookieJar, StatusCode) {
    (jar.remove(clear_auth_cookie()), StatusCode::NO_CONTENT)
}

const FORGOT_SUCCESS: &str =
    "If an account with that email exists, a reset link has been sent.";

#[utoipa::path(
    post,
    path = "/api/auth/forgot-password",
    tag = "auth",
    request_body = ForgotPasswordRequest,
    responses((status = 200, description = "Always succeeds to avoid email enumeration", body = Message)),
)]
pub async fn forgot_password(
    State(state): State<AppState>,
    Json(payload): Json<ForgotPasswordRequest>,
) -> AppResult<Json<Message>> {
    if payload.email.validate_email() {
        let user = sqlx::query_as::<_, UserRow>(
            "SELECT id, email, username, hashed_password, bio, is_admin, account_type, location, availability, expected_rate, languages, avatar_file, email_verified_at, email_messages_enabled, is_suspended, suspended_reason, suspended_at, created_at FROM users WHERE email = $1",
        )
        .bind(&payload.email)
        .fetch_optional(&state.pool)
        .await?;
        if let Some(user) = user {
            let mut bytes = [0u8; 32];
            rand::thread_rng().fill_bytes(&mut bytes);
            let token = hex::encode(bytes);
            let mut hasher = Sha256::new();
            hasher.update(token.as_bytes());
            let token_hash = hex::encode(hasher.finalize());
            let expires_at = Utc::now() + Duration::minutes(60);
            sqlx::query(
                "INSERT INTO password_resets (user_id, token_hash, expires_at) VALUES ($1, $2, $3)",
            )
            .bind(user.id)
            .bind(&token_hash)
            .bind(expires_at)
            .execute(&state.pool)
            .await?;
            if let Err(err) = email::send_reset_email(&state.config, &user.email, &token).await {
                tracing::error!(error = ?err, "reset email failed (returning success anyway)");
            }
        }
    }
    Ok(Json(Message::new(FORGOT_SUCCESS)))
}

#[utoipa::path(
    post,
    path = "/api/auth/reset-password",
    tag = "auth",
    request_body = ResetPasswordRequest,
    responses(
        (status = 200, description = "Password has been reset", body = Message),
        (status = 400, description = "Invalid or expired reset token", body = Message),
        (status = 422, description = "New password too short", body = Message),
    )
)]
pub async fn reset_password(
    State(state): State<AppState>,
    Json(payload): Json<ResetPasswordRequest>,
) -> AppResult<Json<Message>> {
    if payload.new_password.len() < 8 {
        return Err(AppError::Unprocessable(
            "Password must be at least 8 characters long".into(),
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(payload.token.as_bytes());
    let token_hash = hex::encode(hasher.finalize());

    let reset = sqlx::query_as::<_, crate::models::PasswordResetRow>(
        "SELECT id, user_id, token_hash, expires_at, used_at FROM password_resets \
         WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now()",
    )
    .bind(&token_hash)
    .fetch_optional(&state.pool)
    .await?;
    let reset = match reset {
        Some(r) => r,
        None => {
            return Err(AppError::BadRequest(
                "Invalid or expired reset token".into(),
            ))
        }
    };

    let hashed = auth::hash_password(payload.new_password).await?;
    sqlx::query("UPDATE users SET hashed_password = $1 WHERE id = $2")
        .bind(&hashed)
        .bind(reset.user_id)
        .execute(&state.pool)
        .await?;
    sqlx::query("UPDATE password_resets SET used_at = now() WHERE id = $1")
        .bind(reset.id)
        .execute(&state.pool)
        .await?;
    Ok(Json(Message::new("Password has been reset.")))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct VerifyEmailQuery {
    /// Single-purpose token from the verification email.
    pub token: String,
}

#[utoipa::path(
    get,
    path = "/api/auth/verify-email",
    tag = "auth",
    params(VerifyEmailQuery),
    responses(
        (status = 200, description = "Email verified", body = Message),
        (status = 400, description = "Invalid or expired verification link", body = Message),
    )
)]
pub async fn verify_email(
    State(state): State<AppState>,
    Query(query): Query<VerifyEmailQuery>,
) -> AppResult<Json<Message>> {
    let user_id = auth::decode_verification_token(&query.token, &state.config.secret_key)?;

    let updated = sqlx::query("UPDATE users SET email_verified_at = now() WHERE id = $1 AND email_verified_at IS NULL")
        .bind(user_id)
        .execute(&state.pool)
        .await?;

    if updated.rows_affected() == 0 {
        // Already verified is idempotently accepted so links stay shareable/reloadable.
        let exists = sqlx::query_scalar::<_, i32>("SELECT id FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.pool)
            .await?;
        if exists.is_none() {
            return Err(AppError::BadRequest(
                "Verification link is invalid or expired".into(),
            ));
        }
    }

    Ok(Json(Message::new("Email verified. You're all set.")))
}

#[utoipa::path(
    post,
    path = "/api/auth/resend-verification",
    tag = "auth",
    security(("cookie_auth" = [])),
    responses(
        (status = 200, description = "Verification email sent (or the account is already verified)", body = Message),
        (status = 401, description = "Not authenticated", body = Message),
    )
)]
pub async fn resend_verification(
    State(state): State<AppState>,
    CurrentUser(current): CurrentUser,
) -> AppResult<Json<Message>> {
    if current.email_verified_at.is_some() {
        return Ok(Json(Message::new("Email already verified.")));
    }
    send_verification(&state, &current).await;
    Ok(Json(Message::new("If this account exists, a verification email has been sent.")))
}

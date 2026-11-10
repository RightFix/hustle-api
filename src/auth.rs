use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
};
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, Request},
    middleware::Next,
    response::Response,
};
use axum_extra::extract::CookieJar;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::{
    error::{AppError, AppResult},
    models::UserRow,
};

pub const COOKIE_NAME: &str = "access_token";

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
    /// Present only for single-purpose tokens (e.g. email verification).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    purpose: Option<String>,
}

pub const VERIFY_EMAIL_PURPOSE: &str = "verify_email";

pub async fn hash_password(password: String) -> AppResult<String> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|err| {
                tracing::error!(error = %err, "password hashing failed");
                AppError::Internal
            })
    })
    .await
    .map_err(|err| {
        tracing::error!(error = %err, "hash task panicked");
        AppError::Internal
    })?
}

pub async fn verify_password(password: String, hashed: String) -> AppResult<bool> {
    tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&hashed).map_err(|err| {
            tracing::error!(error = %err, "stored password hash is invalid");
            AppError::Internal
        })?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    })
    .await
    .map_err(|err| {
        tracing::error!(error = %err, "verify task panicked");
        AppError::Internal
    })?
}

pub fn create_access_token(
    user_id: i32,
    secret: &str,
    expire_minutes: i64,
) -> AppResult<String> {
    let exp = (Utc::now() + Duration::minutes(expire_minutes)).timestamp() as usize;
    let claims = Claims {
        sub: user_id.to_string(),
        exp,
        purpose: None,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(AppError::from)
}

fn decode_user_id(token: &str, secret: &str) -> AppResult<i32> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| AppError::InvalidCredentials("Could not validate credentials".into()))?;
    if data.claims.purpose.is_some() {
        return Err(AppError::InvalidCredentials("Could not validate credentials".into()));
    }
    data.claims.sub.parse::<i32>().map_err(|_| {
        AppError::InvalidCredentials("Could not validate credentials".into())
    })
}

pub fn create_verification_token(user_id: i32, secret: &str) -> AppResult<String> {
    let exp = (Utc::now() + Duration::hours(48)).timestamp() as usize;
    let claims = Claims {
        sub: user_id.to_string(),
        exp,
        purpose: Some(VERIFY_EMAIL_PURPOSE.to_owned()),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(AppError::from)
}

/// Decode a single-purpose verification token; unlike session tokens, the
/// caller must receive this exact purpose, or decoding fails.
pub fn decode_verification_token(token: &str, secret: &str) -> AppResult<i32> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|_| AppError::BadRequest("Verification link is invalid or expired".into()))?;
    if data.claims.purpose.as_deref() != Some(VERIFY_EMAIL_PURPOSE) {
        return Err(AppError::BadRequest(
            "Verification link is invalid or expired".into(),
        ));
    }
    data.claims.sub.parse::<i32>().map_err(|_| {
        AppError::BadRequest("Verification link is invalid or expired".into())
    })
}

#[derive(Debug, Clone)]
pub struct CurrentUser(pub UserRow);

/// Validate the session cookie and load the user. Shared by the
/// extractor, the admin guard, and the docs gate.
pub async fn user_from_jar(state: &crate::AppState, jar: &CookieJar) -> AppResult<UserRow> {
    let raw = jar
        .get(COOKIE_NAME)
        .map(|c| c.value_trimmed().to_owned())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| {
            AppError::InvalidCredentials("Could not validate credentials".into())
        })?;
    let user_id = decode_user_id(&raw, &state.config.secret_key)?;
    let user = sqlx::query_as::<_, UserRow>(
            "SELECT id, email, username, hashed_password, bio, is_admin, account_type, location, availability, expected_rate, languages, avatar_file, email_verified_at, email_messages_enabled, is_suspended, suspended_reason, suspended_at, created_at FROM users WHERE id = $1",
        )
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| {
            AppError::InvalidCredentials("Could not validate credentials".into())
        })?;
    if user.is_suspended {
        let msg = user
            .suspended_reason
            .clone()
            .filter(|r| !r.trim().is_empty())
            .unwrap_or_else(|| "Your account has been suspended".into());
        return Err(AppError::Forbidden(msg));
    }
    Ok(user)
}

pub async fn require_admin(state: &crate::AppState, jar: &CookieJar) -> AppResult<UserRow> {
    let user = user_from_jar(state, jar).await?;
    if !user.is_admin {
        return Err(AppError::Forbidden("Admin access required".into()));
    }
    Ok(user)
}

impl FromRequestParts<crate::AppState> for CurrentUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &crate::AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| {
                AppError::InvalidCredentials("Could not validate credentials".into())
            })?;
        Ok(Self(user_from_jar(state, &jar).await?))
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AdminUser(pub UserRow);

impl FromRequestParts<crate::AppState> for AdminUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &crate::AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| {
                AppError::InvalidCredentials("Could not validate credentials".into())
            })?;
        Ok(Self(require_admin(state, &jar).await?))
    }
}

/// Layer for the Swagger UI + OpenAPI JSON: logged-in admins only.
pub async fn admin_only(
    axum::extract::State(state): axum::extract::State<crate::AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, AppError> {
    let (mut parts, body) = request.into_parts();
    let jar = CookieJar::from_request_parts(&mut parts, &state)
        .await
        .map_err(|_| {
            AppError::InvalidCredentials("Could not validate credentials".into())
        })?;
    require_admin(&state, &jar).await?;
    Ok(next.run(Request::from_parts(parts, body)).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_round_trip() {
        let token = create_access_token(42, "test-secret", 60).unwrap();
        assert_eq!(decode_user_id(&token, "test-secret").unwrap(), 42);
        assert!(decode_user_id(&token, "wrong-secret").is_err());
        assert!(decode_user_id("not-a-token", "test-secret").is_err());
    }

    #[tokio::test]
    async fn argon2_round_trip() {
        let hash = hash_password("secret123".to_owned()).await.unwrap();
        assert!(hash.starts_with("$argon2"));
        assert!(verify_password("secret123".to_owned(), hash.clone())
            .await
            .unwrap());
        assert!(!verify_password("wrong".to_owned(), hash).await.unwrap());
    }
}

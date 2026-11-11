use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use axum::{
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};

static LIMITER: OnceLock<Mutex<HashMap<String, Vec<Instant>>>> = OnceLock::new();

fn limiter() -> &'static Mutex<HashMap<String, Vec<Instant>>> {
    LIMITER.get_or_init(|| Mutex::new(HashMap::new()))
}

fn check(key: &str, limit: usize, window: Duration) -> bool {
    let now = Instant::now();
    let mut map = limiter().lock().unwrap_or_else(|e| e.into_inner());
    let entry = map.entry(key.to_owned()).or_default();
    entry.retain(|t| now.duration_since(*t) < window);
    if entry.len() >= limit {
        return false;
    }
    entry.push(now);
    if map.len() > 8000 {
        map.retain(|_, v| !v.is_empty());
    }
    true
}

/// Check a custom key (used by WS and other non-HTTP contexts).
pub fn allow(key: &str, limit: usize, window: Duration) -> bool {
    check(key, limit, window)
}

fn client_ip(req: &Request) -> String {
    if let Some(v) = req.headers().get("x-forwarded-for") {
        if let Ok(s) = v.to_str() {
            if let Some(first) = s.split(',').next() {
                let ip = first.trim();
                if !ip.is_empty() {
                    return ip.to_owned();
                }
            }
        }
    }
    "unknown".to_owned()
}

fn deny() -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        axum::Json(serde_json::json!({"detail": "Too many requests, please try again later."})),
    )
        .into_response()
}

pub async fn auth_strict(req: Request, next: Next) -> Response {
    let ip = client_ip(&req);
    if !check(&format!("auth:{ip}"), 10, Duration::from_secs(60)) {
        return deny();
    }
    next.run(req).await
}

pub async fn write_actions(req: Request, next: Next) -> Response {
    let ip = client_ip(&req);
    if !check(&format!("write:{ip}"), 60, Duration::from_secs(60)) {
        return deny();
    }
    next.run(req).await
}

pub async fn report_limit(req: Request, next: Next) -> Response {
    let ip = client_ip(&req);
    if !check(&format!("report:{ip}"), 20, Duration::from_secs(600)) {
        return deny();
    }
    next.run(req).await
}

/// Higher-friction social/write interactions (follow, start conversation, apply).
pub async fn interact_limit(req: Request, next: Next) -> Response {
    let ip = client_ip(&req);
    if !check(&format!("interact:{ip}"), 40, Duration::from_secs(60)) {
        return deny();
    }
    next.run(req).await
}


/// Helper for in-handler checks (returns Err-style bool: true if allowed).
pub fn check_ip(headers: &axum::http::HeaderMap, bucket: &str, limit: usize, window_secs: u64) -> bool {
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_owned());
    check(&format!("{bucket}:{ip}"), limit, Duration::from_secs(window_secs))
}

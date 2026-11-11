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
    if map.len() > 5000 {
        map.retain(|_, v| !v.is_empty());
    }
    true
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

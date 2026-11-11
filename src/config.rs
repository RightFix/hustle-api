use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub project_name: String,
    pub environment: String,
    pub database_url: String,
    pub secret_key: String,
    pub access_token_expire_minutes: i64,
    pub cors_origin: String,
    pub frontend_url: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_user: String,
    pub smtp_pass: String,
    pub smtp_from: String,
    pub upload_dir: String,
}

fn get(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_owned())
}

impl Config {
    pub fn from_env() -> Self {
        let _ = dotenvy::dotenv();
        Self {
            project_name: get("PROJECT_NAME", "Hustle"),
            environment: get("ENVIRONMENT", "development"),
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgresql://hustle:hustle@localhost:5432/hustle".to_owned()),
            secret_key: get("SECRET_KEY", "change-me-in-production"),
            access_token_expire_minutes: env::var("ACCESS_TOKEN_EXPIRE_MINUTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1440),
            cors_origin: get("CORS_ORIGIN", "http://localhost:5173"),
            frontend_url: get("FRONTEND_URL", "http://localhost:5173"),
            smtp_host: get("SMTP_HOST", "localhost"),
            smtp_port: env::var("SMTP_PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1025),
            smtp_user: get("SMTP_USER", ""),
            smtp_pass: get("SMTP_PASS", ""),
            smtp_from: get("SMTP_FROM", "noreply@hustle.local"),
            upload_dir: get("UPLOAD_DIR", "uploads"),
        }
    }

    pub fn is_production(&self) -> bool {
        self.environment == "production"
    }
}

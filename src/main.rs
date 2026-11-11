mod audit;
mod auth;
mod config;
mod docs;
mod email;
mod error;
mod images;
mod models;
mod notify;
mod rate_limit;
mod routes;
mod ws;

use std::sync::Arc;

use axum::http::{HeaderValue, Method};
use config::Config;
use docs::ApiDoc;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(Debug, Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub hub: Arc<ws::WsHub>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hustle_backend=debug,tower_http=debug".into()),
        )
        .init();

    let config = Config::from_env();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    tracing::info!("database migrations applied");

    let origin: HeaderValue = config
        .cors_origin
        .parse()
        .expect("CORS_ORIGIN must be a valid origin");
    let cors = CorsLayer::new()
        .allow_origin(origin)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
        ])
        .allow_credentials(true);

    let state = AppState {
        pool,
        config: Arc::new(config),
        hub: Arc::new(ws::WsHub::new()),
    };
    let docs: axum::Router<AppState> = SwaggerUi::new("/docs")
        .url("/api-docs/openapi.json", ApiDoc::openapi())
        .into();
    let docs = docs.route_layer(axum::middleware::from_fn_with_state(
        state.clone(),
        auth::admin_only,
    ));
    let app = routes::router(&state.config.upload_dir)
        .merge(docs)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}

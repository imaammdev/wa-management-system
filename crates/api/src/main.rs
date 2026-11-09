//! mms-api — the HTTP surface.
//!
//! Three families of routes:
//!   /v1/*        public, documented API, authenticated by API key
//!   /api/*       dashboard API, authenticated by session cookie
//!   /webhooks/*  inbound from Meta
//! plus /public/* for the marketing site and /healthz for probes.

mod error;
mod extract;
mod ratelimit;
mod routes;
mod service;
mod state;

use axum::http::{header, HeaderValue, Method};
use axum::routing::get;
use axum::{Json, Router};
use mms_core::{db, Config};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::ratelimit::RateLimiter;
use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,sqlx=warn".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cfg = Config::from_env()?;
    tracing::info!(provider = %cfg.wa_provider, "starting mms-api");

    let pool = db::connect(&cfg.database_url, 20).await?;
    db::migrate(&pool).await?;
    tracing::info!("migrations applied");

    let limiter = Arc::new(RateLimiter::new());

    // Keep the rate-limit map from growing without bound.
    let sweeper = limiter.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(600));
        loop {
            tick.tick().await;
            sweeper.sweep();
        }
    });

    let state = AppState {
        pool,
        meta: Arc::from(mms_provider::build_with(
            "meta_cloud",
            &cfg.meta_api_base,
            &cfg.meta_api_version,
            cfg.meta_conversions_path.clone(),
        )),
        mock: Arc::from(mms_provider::build("mock", "", "")),
        limiter,
        cfg: Arc::new(cfg),
    };

    // The dashboard is served from a different origin in dev, so it needs CORS
    // with credentials for the session cookie to be sent.
    let cors = CorsLayer::new()
        .allow_origin(
            state
                .cfg
                .app_url
                .parse::<HeaderValue>()
                .unwrap_or_else(|_| HeaderValue::from_static("http://localhost:5173")),
        )
        .allow_credentials(true)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            header::HeaderName::from_static("x-org-id"),
            header::HeaderName::from_static("idempotency-key"),
        ]);

    let app = Router::new()
        .route("/healthz", get(healthz))
        .nest("/public", routes::public::router())
        .nest("/webhooks", routes::meta_webhook::router())
        .nest("/v1", routes::v1::router())
        .nest("/api/auth", routes::auth::router())
        .nest("/api/organizations", routes::orgs::router())
        .nest("/api/api-keys", routes::keys::router())
        .nest("/api/whatsapp-accounts", routes::wa_accounts::router())
        .nest("/api/inbox", routes::inbox::router())
        .nest("/api/contacts", routes::contacts::router())
        .nest("/api/crm", routes::crm::router())
        .nest("/api/analytics", routes::analytics::router())
        .nest("/api/ai", routes::ai::router())
        .nest("/api/ads", routes::ads::router())
        .nest("/api/kichat", routes::kichat::router())
        // Public, token-authenticated customer chat room.
        .nest("/kichat", routes::kichat::public_router())
        .nest("/api/settings", routes::settings::router())
        .nest("/api/templates", routes::templates::router())
        .nest("/api/broadcasts", routes::broadcasts::router())
        .nest("/api/webhooks", routes::webhooks::router())
        .nest("/api/billing", routes::billing::router())
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        // Media is uploaded separately; JSON bodies stay small.
        .layer(RequestBodyLimitLayer::new(2 * 1024 * 1024))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(&state.cfg.bind_addr).await?;
    tracing::info!(addr = %state.cfg.bind_addr, "listening");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn healthz(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<serde_json::Value> {
    let db_ok = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .is_ok();
    Json(json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "database": db_ok,
        "provider": state.cfg.wa_provider,
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

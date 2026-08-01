pub mod attachments;
pub mod auth;
pub mod config;
pub mod error;
pub mod rate_limit;
pub mod state;
pub mod sync;

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    routing::{delete, get, post},
};
use serde::Serialize;
use sqlx::PgPool;

use crate::{config::Config, state::AppState};

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    service: &'static str,
}

pub fn app(pool: PgPool, config: &Config) -> Router {
    let state = AppState::new(pool, config);
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/api/v1/auth/register", post(auth::register))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/auth/refresh", post(auth::refresh))
        .route("/api/v1/auth/logout", post(auth::logout))
        .route("/api/v1/devices", get(auth::list_devices))
        .route("/api/v1/devices/{id}", delete(auth::revoke_device))
        .route("/api/v1/sync/push", post(sync::push))
        .route("/api/v1/sync/pull", get(sync::pull))
        .route("/api/v1/sync/ack", post(sync::ack))
        .route("/api/v1/attachments", post(attachments::create))
        .route("/api/v1/attachments/{id}", get(attachments::status))
        .route(
            "/api/v1/attachments/{id}/parts/{part}",
            axum::routing::put(attachments::upload_part).layer(DefaultBodyLimit::max(
                snapline_domain::ATTACHMENT_PART_BYTES as usize + 1,
            )),
        )
        .route(
            "/api/v1/attachments/{id}/complete",
            post(attachments::complete),
        )
        .route(
            "/api/v1/attachments/{id}/content",
            get(attachments::download),
        )
        .with_state(state)
}

async fn live() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "snapline-server",
    })
}

async fn ready() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        service: "snapline-server",
    })
}

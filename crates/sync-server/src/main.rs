mod auth;
mod config;
mod db;
mod routes;
mod sync_service;

use anyhow::Result;
use axum::{
    routing::{get, post},
    Router,
};
use config::Config;
use routes::AppState;
use std::{net::SocketAddr, sync::Arc};

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_env()?;
    let pool = db::connect(&config.database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    let state = Arc::new(AppState { pool, config });

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/auth/register", post(routes::register))
        .route("/auth/login", post(routes::login))
        .route("/sync/push", post(routes::push))
        .route("/sync/pull", get(routes::pull))
        .route("/sync/snapshot", get(routes::snapshot))
        .with_state(state);
    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

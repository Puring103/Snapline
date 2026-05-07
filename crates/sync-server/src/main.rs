/// Snapline 同步服务器入口：初始化配置、数据库连接和 Axum 路由。
mod assets;
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
    eprintln!(
        "snapline.sync_server.public_base_url={}",
        config.public_base_url
    );
    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;
    routes::bootstrap_first_account(&pool, &config).await?;
    let asset_store = assets::LocalFsAssetStore::new(&config.asset_data_dir);
    let state = Arc::new(AppState {
        pool,
        config,
        asset_store,
    });

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/auth/register", post(routes::register))
        .route("/auth/login", post(routes::login))
        .route("/sync/push", post(routes::push))
        .route("/sync/pull", get(routes::pull))
        .route("/sync/snapshot", get(routes::snapshot))
        .route("/sync/assets/upload", post(routes::upload_asset))
        .route(
            "/sync/assets/:asset_id/download",
            get(routes::download_asset),
        )
        .with_state(state);
    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

mod auth;
mod config;
mod db;

use anyhow::Result;
use axum::{routing::get, Router};
use config::Config;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::from_env()?;
    let pool = db::connect(&config.database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let app = Router::new().route("/health", get(|| async { "ok" }));
    let addr: SocketAddr = "0.0.0.0:8080".parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

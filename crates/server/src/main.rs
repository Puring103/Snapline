use anyhow::Result;
use snapline_server::{app, config::Config};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .json()
        .init();

    let config = Config::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(20)
        .connect(&config.database_url)
        .await?;
    sqlx::migrate!().run(&pool).await?;
    let listener = TcpListener::bind(config.bind).await?;
    tracing::info!(address = %config.bind, "snapline server listening");
    axum::serve(listener, app(pool, &config)).await?;
    Ok(())
}

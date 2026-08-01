use anyhow::Result;
use snapline_server::{config::Config, health_router};
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
    let listener = TcpListener::bind(config.bind).await?;
    tracing::info!(address = %config.bind, "snapline server listening");
    axum::serve(listener, health_router()).await?;
    Ok(())
}

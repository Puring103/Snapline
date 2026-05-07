/// PostgreSQL 连接池初始化与 schema 迁移。
use anyhow::Result;
use sqlx::{postgres::PgPoolOptions, PgPool};

pub async fn connect(database_url: &str) -> Result<PgPool> {
    Ok(PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await?)
}

pub async fn migrate(pool: &PgPool) -> Result<()> {
    for sql in [
        include_str!("../migrations/0001_init.sql"),
        include_str!("../migrations/0002_e2ee.sql"),
    ] {
        for statement in sql.split(';').map(str::trim).filter(|s| !s.is_empty()) {
            sqlx::query(statement).execute(pool).await?;
        }
    }
    Ok(())
}

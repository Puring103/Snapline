use std::path::PathBuf;

use sqlx::PgPool;

use crate::{auth::TokenService, config::Config, rate_limit::LoginLimiter};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub tokens: TokenService,
    pub login_limiter: LoginLimiter,
    pub object_dir: PathBuf,
    pub attachment_quota_bytes: u64,
    pub upload_ttl_seconds: i64,
    pub upload_cleanup_interval_seconds: u64,
}

impl AppState {
    pub fn new(pool: PgPool, config: &Config) -> Self {
        Self {
            pool,
            tokens: TokenService::new(
                config.jwt_secret.as_bytes(),
                config.access_token_ttl_seconds,
                config.refresh_token_ttl_seconds,
            ),
            login_limiter: LoginLimiter::default(),
            object_dir: config.object_dir.clone(),
            attachment_quota_bytes: config.attachment_quota_bytes,
            upload_ttl_seconds: config.upload_ttl_seconds,
            upload_cleanup_interval_seconds: config.upload_cleanup_interval_seconds,
        }
    }
}

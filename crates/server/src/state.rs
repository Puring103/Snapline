use sqlx::PgPool;

use crate::{auth::TokenService, config::Config, rate_limit::LoginLimiter};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub tokens: TokenService,
    pub login_limiter: LoginLimiter,
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
        }
    }
}

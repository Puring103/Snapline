use std::{env, net::SocketAddr, str::FromStr};

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub database_url: String,
    pub jwt_secret: String,
    pub access_token_ttl_seconds: i64,
    pub refresh_token_ttl_seconds: i64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bind = env::var("SNAPLINE_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());
        let database_url =
            env::var("SNAPLINE_DATABASE_URL").context("SNAPLINE_DATABASE_URL is required")?;
        let jwt_secret =
            env::var("SNAPLINE_JWT_SECRET").context("SNAPLINE_JWT_SECRET is required")?;
        if jwt_secret.len() < 32 {
            bail!("SNAPLINE_JWT_SECRET must contain at least 32 characters");
        }
        Ok(Self {
            bind: SocketAddr::from_str(&bind).context("invalid SNAPLINE_BIND")?,
            database_url,
            jwt_secret,
            access_token_ttl_seconds: parse_i64("SNAPLINE_ACCESS_TOKEN_TTL_SECONDS", 900)?,
            refresh_token_ttl_seconds: parse_i64("SNAPLINE_REFRESH_TOKEN_TTL_SECONDS", 2_592_000)?,
        })
    }
}

fn parse_i64(name: &str, default: i64) -> Result<i64> {
    match env::var(name) {
        Ok(value) => value.parse().with_context(|| format!("invalid {name}")),
        Err(_) => Ok(default),
    }
}

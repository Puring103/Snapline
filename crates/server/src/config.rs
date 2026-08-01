use std::{env, net::SocketAddr, path::PathBuf, str::FromStr};

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: SocketAddr,
    pub database_url: String,
    pub jwt_secret: String,
    pub access_token_ttl_seconds: i64,
    pub refresh_token_ttl_seconds: i64,
    pub object_dir: PathBuf,
    pub attachment_quota_bytes: u64,
    pub upload_ttl_seconds: i64,
    pub upload_cleanup_interval_seconds: u64,
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
        let attachment_quota_bytes =
            parse_u64("SNAPLINE_ATTACHMENT_QUOTA_BYTES", 10 * 1024 * 1024 * 1024)?;
        let upload_ttl_seconds = parse_i64("SNAPLINE_UPLOAD_TTL_SECONDS", 86_400)?;
        let upload_cleanup_interval_seconds =
            parse_u64("SNAPLINE_UPLOAD_CLEANUP_INTERVAL_SECONDS", 3_600)?;
        if attachment_quota_bytes == 0 {
            bail!("SNAPLINE_ATTACHMENT_QUOTA_BYTES must be greater than zero");
        }
        if upload_ttl_seconds <= 0 {
            bail!("SNAPLINE_UPLOAD_TTL_SECONDS must be greater than zero");
        }
        if upload_cleanup_interval_seconds == 0 {
            bail!("SNAPLINE_UPLOAD_CLEANUP_INTERVAL_SECONDS must be greater than zero");
        }
        Ok(Self {
            bind: SocketAddr::from_str(&bind).context("invalid SNAPLINE_BIND")?,
            database_url,
            jwt_secret,
            access_token_ttl_seconds: parse_i64("SNAPLINE_ACCESS_TOKEN_TTL_SECONDS", 900)?,
            refresh_token_ttl_seconds: parse_i64("SNAPLINE_REFRESH_TOKEN_TTL_SECONDS", 2_592_000)?,
            object_dir: PathBuf::from(
                env::var("SNAPLINE_OBJECT_DIR").unwrap_or_else(|_| "data/objects".into()),
            ),
            attachment_quota_bytes,
            upload_ttl_seconds,
            upload_cleanup_interval_seconds,
        })
    }
}

fn parse_u64(name: &str, default: u64) -> Result<u64> {
    match env::var(name) {
        Ok(value) => value.parse().with_context(|| format!("invalid {name}")),
        Err(_) => Ok(default),
    }
}

fn parse_i64(name: &str, default: i64) -> Result<i64> {
    match env::var(name) {
        Ok(value) => value.parse().with_context(|| format!("invalid {name}")),
        Err(_) => Ok(default),
    }
}

/// 服务器配置：从环境变量中读取所有运行时参数。
use anyhow::{Context, Result};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
    pub asset_data_dir: PathBuf,
    pub public_base_url: String,
    pub allow_registration: bool,
    pub bootstrap_admin_email: Option<String>,
    pub bootstrap_admin_password: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: std::env::var("DATABASE_URL").context("DATABASE_URL is required")?,
            jwt_secret: std::env::var("JWT_SECRET").context("JWT_SECRET is required")?,
            asset_data_dir: std::env::var("ASSET_DATA_DIR")
                .context("ASSET_DATA_DIR is required")?
                .into(),
            public_base_url: std::env::var("PUBLIC_BASE_URL")
                .context("PUBLIC_BASE_URL is required")?,
            allow_registration: std::env::var("ALLOW_REGISTRATION")
                .unwrap_or_else(|_| "true".to_string())
                == "true",
            bootstrap_admin_email: std::env::var("SNAPLINE_BOOTSTRAP_ADMIN_EMAIL").ok(),
            bootstrap_admin_password: std::env::var("SNAPLINE_BOOTSTRAP_ADMIN_PASSWORD").ok(),
        })
    }
}

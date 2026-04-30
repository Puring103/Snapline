pub mod mock;
pub mod processor;
pub mod protocol;

use anyhow::Result;
use async_trait::async_trait;
use protocol::{AssetDownload, AssetUploadRequest};
use protocol::{
    LoginRequest, LoginResponse, PullResponse, PushRequest, PushResponse, SnapshotResponse,
};

#[async_trait]
pub trait SyncApi {
    async fn login(&self, request: LoginRequest) -> Result<LoginResponse>;
    async fn push(&self, token: &str, request: PushRequest) -> Result<PushResponse>;
    async fn pull(&self, token: &str, cursor: i64) -> Result<PullResponse>;
    async fn snapshot(&self, token: &str) -> Result<SnapshotResponse>;
    async fn upload_asset(&self, token: &str, request: AssetUploadRequest) -> Result<()>;
    async fn download_asset(&self, token: &str, asset_id: &str) -> Result<AssetDownload>;
}

pub struct HttpSyncApi {
    base_url: String,
    client: reqwest::Client,
}

impl HttpSyncApi {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl SyncApi for HttpSyncApi {
    async fn login(&self, request: LoginRequest) -> Result<LoginResponse> {
        Ok(self
            .client
            .post(format!("{}/auth/login", self.base_url))
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn push(&self, token: &str, request: PushRequest) -> Result<PushResponse> {
        Ok(self
            .client
            .post(format!("{}/sync/push", self.base_url))
            .bearer_auth(token)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn pull(&self, token: &str, cursor: i64) -> Result<PullResponse> {
        Ok(self
            .client
            .get(format!("{}/sync/pull", self.base_url))
            .bearer_auth(token)
            .query(&[("cursor", cursor)])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn snapshot(&self, token: &str) -> Result<SnapshotResponse> {
        Ok(self
            .client
            .get(format!("{}/sync/snapshot", self.base_url))
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn upload_asset(&self, token: &str, request: AssetUploadRequest) -> Result<()> {
        let form = reqwest::multipart::Form::new()
            .text("metadata", serde_json::to_string(&request.metadata)?)
            .part(
                "file",
                reqwest::multipart::Part::bytes(request.bytes)
                    .file_name(request.metadata.markdown_path.clone())
                    .mime_str(&request.metadata.content_type)?,
            );
        self.client
            .post(format!("{}/sync/assets/upload", self.base_url))
            .bearer_auth(token)
            .multipart(form)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    async fn download_asset(&self, token: &str, asset_id: &str) -> Result<AssetDownload> {
        let response = self
            .client
            .get(format!(
                "{}/sync/assets/{}/download",
                self.base_url, asset_id
            ))
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        let bytes = response.bytes().await?.to_vec();
        Ok(AssetDownload {
            content_type,
            bytes,
        })
    }
}

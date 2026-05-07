/// 同步客户端：定义与服务端通信的抽象接口及 HTTP 实现。
///
/// `SyncApi` trait 支持依赖注入，测试中可用 `MockSyncApi` 替换真实 HTTP 调用。
pub mod mock;
pub mod processor;
pub mod protocol;

use anyhow::Result;
use async_trait::async_trait;
use protocol::{AssetDownload, AssetUploadRequest};
use protocol::{
    LoginRequest, LoginResponse, PullResponse, PushRequest, PushResponse, SnapshotResponse,
};

/// 同步 API 的抽象接口，解耦业务逻辑与具体传输层。
#[async_trait]
pub trait SyncApi {
    /// 登录并获取 access token。
    async fn login(&self, request: LoginRequest) -> Result<LoginResponse>;
    /// 将本地变更批量推送到服务端。
    async fn push(&self, token: &str, request: PushRequest) -> Result<PushResponse>;
    /// 拉取服务端在指定游标之后的变更。
    async fn pull(&self, token: &str, cursor: i64) -> Result<PullResponse>;
    /// 获取账户全量快照（首次登录或重置时使用）。
    async fn snapshot(&self, token: &str) -> Result<SnapshotResponse>;
    /// 上传单个资源文件（multipart form）。
    async fn upload_asset(&self, token: &str, request: AssetUploadRequest) -> Result<()>;
    /// 按 asset_id 下载资源文件字节。
    async fn download_asset(&self, token: &str, asset_id: &str) -> Result<AssetDownload>;
}

/// 基于 `reqwest` 的 HTTP 同步 API 实现。
pub struct HttpSyncApi {
    base_url: String,
    client: reqwest::Client,
}

impl HttpSyncApi {
    /// 创建客户端，`base_url` 末尾斜杠会被自动去除。
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

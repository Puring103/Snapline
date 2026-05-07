use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 资源文件的唯一标识符，基于 UUID v4。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AssetId(pub Uuid);

impl AssetId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(value: &str) -> Result<Self, uuid::Error> {
        Uuid::parse_str(value).map(Self)
    }
}

impl Default for AssetId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AssetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// 保存图片后返回给调用方的路径三元组。
///
/// - `markdown_path`：写入 Markdown 正文的相对路径（如 `assets/notes/<note>/<asset>.png`）
/// - `filesystem_path`：磁盘上的绝对路径，供前端通过 `<img>` 标签直接读取
/// - `asset_url`：`asset://localhost/...` 协议 URL，供 Tauri WebView 访问本地文件
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRef {
    pub markdown_path: String,
    pub filesystem_path: String,
    pub asset_url: String,
}

/// 资源文件的元数据，用于同步和快照。
///
/// `storage_key` 是服务端存储路径（例如 S3 key 或本地文件路径），
/// 客户端通过 `AssetUploadPayload.markdown_path` 引用同一文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetMetadata {
    pub id: AssetId,
    pub note_id: crate::NoteId,
    pub content_type: String,
    pub byte_size: i64,
    pub sha256: String,
    /// 服务端存储键（文件系统路径或对象存储 key）。
    pub storage_key: String,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::AssetId;

    #[test]
    fn default_asset_id_generates_uuid() {
        let id = AssetId::default();

        assert_eq!(id.to_string().len(), 36);
    }
}

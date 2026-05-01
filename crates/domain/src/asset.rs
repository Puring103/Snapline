use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRef {
    pub markdown_path: String,
    pub filesystem_path: String,
    pub asset_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetMetadata {
    pub id: AssetId,
    pub note_id: crate::NoteId,
    pub content_type: String,
    pub byte_size: i64,
    pub sha256: String,
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

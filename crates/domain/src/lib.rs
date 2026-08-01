use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const API_PREFIX: &str = "/api/v1";
pub const MAX_SYNC_PAGE_SIZE: u32 = 500;
pub const DEFAULT_SYNC_PAGE_SIZE: u32 = 100;
pub const MAX_CIPHERTEXT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_ATTACHMENT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const ATTACHMENT_PART_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncOperation {
    Upsert,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EncryptedEnvelope {
    pub object_id: Uuid,
    pub object_type: String,
    pub device_id: Uuid,
    pub base_version: i64,
    pub operation: SyncOperation,
    pub ciphertext: String,
    pub nonce: String,
    pub wrapped_key: String,
    pub client_updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SyncChange {
    pub cursor: i64,
    pub version: i64,
    pub envelope: EncryptedEnvelope,
    pub server_created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiErrorBody {
    pub code: String,
    pub message: String,
    pub request_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_json_uses_stable_snake_case_operations() {
        let envelope = EncryptedEnvelope {
            object_id: Uuid::nil(),
            object_type: "item".into(),
            device_id: Uuid::nil(),
            base_version: 0,
            operation: SyncOperation::Upsert,
            ciphertext: "ciphertext".into(),
            nonce: "nonce".into(),
            wrapped_key: "wrapped".into(),
            client_updated_at: DateTime::UNIX_EPOCH,
        };
        let json = serde_json::to_value(&envelope).expect("serialize envelope");
        assert_eq!(json["operation"], "upsert");
        assert_eq!(
            serde_json::from_value::<EncryptedEnvelope>(json).unwrap(),
            envelope
        );
    }
}

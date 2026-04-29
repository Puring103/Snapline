use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use snapline_domain::{AssetMetadata, Note, NoteId, SyncPayload};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub account_id: String,
    pub access_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushRequest {
    pub device_id: String,
    pub changes: Vec<PushChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushChange {
    pub queue_id: String,
    pub note_id: NoteId,
    pub base_version: i64,
    pub payload: SyncPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PushChangeResult {
    Accepted {
        queue_id: String,
        note_id: NoteId,
        server_version: i64,
        cursor: i64,
    },
    Conflict {
        queue_id: String,
        note_id: NoteId,
        server_note: Note,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResponse {
    pub results: Vec<PushChangeResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullResponse {
    pub cursor: i64,
    pub changes: Vec<RemoteChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteChange {
    pub cursor: i64,
    pub device_id: String,
    pub note: Note,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotResponse {
    pub cursor: i64,
    pub notes: Vec<Note>,
    pub assets: Vec<AssetMetadata>,
}

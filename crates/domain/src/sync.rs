use crate::{AssetId, Note, NoteId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncOpType {
    UpsertNote,
    DeleteNote,
    AssetUpload,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteChangePayload {
    pub title: String,
    pub content_md: String,
    pub pinned: bool,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl NoteChangePayload {
    pub fn from_note(note: &Note) -> Self {
        Self {
            title: note.title.clone(),
            content_md: note.content_md.clone(),
            pinned: note.pinned,
            deleted_at: note.deleted_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetUploadPayload {
    pub asset_id: AssetId,
    pub note_id: NoteId,
    pub content_type: String,
    pub byte_size: i64,
    pub sha256: String,
    pub markdown_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyncPayload {
    Note(NoteChangePayload),
    Asset(AssetUploadPayload),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictCopyRequest {
    pub source_note_id: NoteId,
    pub rejected_note: Note,
    pub server_note: Note,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Note;
    use chrono::{TimeZone, Utc};

    #[test]
    fn note_payload_round_trips_as_json() {
        let mut note = Note::draft(Utc.with_ymd_and_hms(2026, 4, 29, 1, 0, 0).unwrap());
        note.title = "Hello".to_string();
        note.content_md = "# Hello".to_string();
        note.pinned = true;

        let payload = SyncPayload::Note(NoteChangePayload::from_note(&note));
        let json = serde_json::to_string(&payload).unwrap();
        let decoded: SyncPayload = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, payload);
        assert!(!json.contains("upsert_note"));
        assert!(json.contains("Hello"));
    }
}

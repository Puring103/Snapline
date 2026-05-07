use crate::{AssetId, Note, NoteId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 同步操作类型，写入 change_log 和 change_queue 的 `op_type` 列。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncOpType {
    /// 新建或更新笔记。
    UpsertNote,
    /// 软删除笔记。
    DeleteNote,
    /// 上传资源文件。
    AssetUpload,
}

impl SyncOpType {
    /// 返回写入数据库的字符串表示，与 serde 序列化保持一致。
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncOpType::UpsertNote => "upsert_note",
            SyncOpType::DeleteNote => "delete_note",
            SyncOpType::AssetUpload => "asset_upload",
        }
    }

    /// 从数据库字符串解析操作类型。
    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "upsert_note" => Some(SyncOpType::UpsertNote),
            "delete_note" => Some(SyncOpType::DeleteNote),
            "asset_upload" => Some(SyncOpType::AssetUpload),
            _ => None,
        }
    }
}

/// 推送笔记变更时携带的载荷，仅包含可被服务端覆写的字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteChangePayload {
    pub title: String,
    pub content_md: String,
    pub pinned: bool,
    /// 若不为 None，表示本次变更为软删除。
    pub deleted_at: Option<DateTime<Utc>>,
}

impl NoteChangePayload {
    /// 从笔记对象快照出推送载荷。
    pub fn from_note(note: &Note) -> Self {
        Self {
            title: note.title.clone(),
            content_md: note.content_md.clone(),
            pinned: note.pinned,
            deleted_at: note.deleted_at,
        }
    }
}

/// 推送资源文件时携带的元数据载荷。
///
/// `markdown_path` 是客户端 Markdown 正文中引用该资源的相对路径，
/// 服务端用它生成 `storage_key`，两者含义不同，不要混用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetUploadPayload {
    pub asset_id: AssetId,
    pub note_id: NoteId,
    pub content_type: String,
    pub byte_size: i64,
    pub sha256: String,
    /// 客户端 Markdown 正文中的相对引用路径。
    pub markdown_path: String,
}

/// 同步载荷的联合类型，区分笔记变更和资源上传。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyncPayload {
    Note(NoteChangePayload),
    Asset(AssetUploadPayload),
}

/// 冲突副本创建请求，记录被服务端拒绝的本地笔记和服务端当前版本。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConflictCopyRequest {
    pub source_note_id: NoteId,
    /// 被服务端拒绝的本地编辑版本。
    pub rejected_note: Note,
    /// 服务端当前持有的版本（将覆写本地）。
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

    #[test]
    fn op_type_as_str_round_trips() {
        for op in [
            SyncOpType::UpsertNote,
            SyncOpType::DeleteNote,
            SyncOpType::AssetUpload,
        ] {
            assert_eq!(SyncOpType::from_str(op.as_str()), Some(op));
        }
    }
}

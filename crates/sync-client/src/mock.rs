/// 用于测试的内存 SyncApi 实现，模拟服务端的推送/拉取/快照行为。
use crate::protocol::*;
use crate::SyncApi;
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use snapline_domain::{AssetMetadata, AssetUploadPayload, Note, SyncPayload};
use std::sync::Mutex;

/// 线程安全的内存同步后端，各字段均用 `Mutex` 保护。
#[derive(Default)]
pub struct MockSyncApi {
    notes: Mutex<Vec<Note>>,
    /// 记录每条变更对应的设备 ID，用于拉取时填充 `RemoteChange.device_id`。
    change_devices: Mutex<Vec<(i64, snapline_domain::NoteId, String)>>,
    assets: Mutex<Vec<AssetUploadPayload>>,
    asset_bytes: Mutex<Vec<(String, Vec<u8>)>>,
    /// 全局单调递增游标，每接受一条变更时加 1。
    cursor: Mutex<i64>,
}

#[async_trait]
impl SyncApi for MockSyncApi {
    async fn register(&self, request: LoginRequest) -> Result<LoginResponse> {
        self.login(request).await
    }

    async fn login(&self, request: LoginRequest) -> Result<LoginResponse> {
        let account_id = format!("acct_{}", request.email);
        Ok(LoginResponse {
            access_token: format!("token:{account_id}"),
            account_id,
            kek_salt: None,
            encrypted_dek: None,
        })
    }

    async fn push(&self, _token: &str, request: PushRequest) -> Result<PushResponse> {
        let account_id =
            account_id_from_token(_token).unwrap_or_else(|| "mock-account".to_string());
        let mut notes = self.notes.lock().unwrap();
        let mut cursor = self.cursor.lock().unwrap();
        let mut results = Vec::new();
        for change in request.changes {
            if let SyncPayload::Note(payload) = change.payload {
                if let Some(existing) = notes.iter_mut().find(|note| {
                    note.id == change.note_id
                        && note.owner_account_id.as_deref() == Some(&account_id)
                }) {
                    // 版本号不匹配 → 冲突
                    if existing.server_version != change.base_version {
                        results.push(PushChangeResult::Conflict {
                            queue_id: change.queue_id,
                            note_id: change.note_id,
                            server_note: existing.clone(),
                        });
                        continue;
                    }
                    existing.title = payload.title;
                    existing.content_md = payload.content_md;
                    existing.pinned = payload.pinned;
                    existing.deleted_at = payload.deleted_at;
                    existing.server_version += 1;
                    *cursor += 1;
                    self.change_devices.lock().unwrap().push((
                        *cursor,
                        existing.id.clone(),
                        request.device_id.clone(),
                    ));
                    results.push(PushChangeResult::Accepted {
                        queue_id: change.queue_id,
                        note_id: existing.id.clone(),
                        server_version: existing.server_version,
                        cursor: *cursor,
                    });
                } else {
                    // 新笔记：直接插入
                    let mut note = Note::draft(Utc::now());
                    note.id = change.note_id.clone();
                    note.title = payload.title;
                    note.content_md = payload.content_md;
                    note.pinned = payload.pinned;
                    note.deleted_at = payload.deleted_at;
                    note.owner_account_id = Some(account_id.clone());
                    note.server_version = 1;
                    notes.push(note);
                    *cursor += 1;
                    self.change_devices.lock().unwrap().push((
                        *cursor,
                        change.note_id.clone(),
                        request.device_id.clone(),
                    ));
                    results.push(PushChangeResult::Accepted {
                        queue_id: change.queue_id,
                        note_id: change.note_id,
                        server_version: 1,
                        cursor: *cursor,
                    });
                }
            }
        }
        Ok(PushResponse { results })
    }

    async fn pull(&self, _token: &str, _cursor: i64) -> Result<PullResponse> {
        let account_id =
            account_id_from_token(_token).unwrap_or_else(|| "mock-account".to_string());
        let notes = self.notes.lock().unwrap();
        let cursor = *self.cursor.lock().unwrap();
        let change_devices = self.change_devices.lock().unwrap();
        Ok(PullResponse {
            cursor,
            changes: change_devices
                .iter()
                .filter(|(change_cursor, _, _)| *change_cursor > _cursor)
                .filter_map(|(change_cursor, note_id, device_id)| {
                    notes
                        .iter()
                        .find(|note| {
                            note.id == *note_id
                                && note.owner_account_id.as_deref() == Some(&account_id)
                        })
                        .cloned()
                        .map(|note| RemoteChange {
                            cursor: *change_cursor,
                            device_id: device_id.clone(),
                            note,
                            changed_at: Utc::now(),
                        })
                })
                .collect(),
        })
    }

    async fn snapshot(&self, _token: &str) -> Result<SnapshotResponse> {
        let account_id =
            account_id_from_token(_token).unwrap_or_else(|| "mock-account".to_string());
        Ok(SnapshotResponse {
            cursor: *self.cursor.lock().unwrap(),
            notes: self
                .notes
                .lock()
                .unwrap()
                .iter()
                .filter(|note| note.owner_account_id.as_deref() == Some(&account_id))
                .cloned()
                .collect(),
            assets: self
                .assets
                .lock()
                .unwrap()
                .iter()
                .filter(|asset| asset.markdown_path.strip_prefix("assets/notes/").is_some())
                .map(|asset| AssetMetadata {
                    id: asset.asset_id.clone(),
                    note_id: asset.note_id.clone(),
                    content_type: asset.content_type.clone(),
                    byte_size: asset.byte_size,
                    sha256: asset.sha256.clone(),
                    storage_key: asset.markdown_path.clone(),
                    created_at: Utc::now(),
                    deleted_at: None,
                })
                .collect(),
        })
    }

    async fn upload_asset(&self, _token: &str, request: AssetUploadRequest) -> Result<()> {
        self.asset_bytes
            .lock()
            .unwrap()
            .push((request.metadata.asset_id.to_string(), request.bytes));
        self.assets.lock().unwrap().push(request.metadata);
        Ok(())
    }

    async fn download_asset(&self, _token: &str, asset_id: &str) -> Result<AssetDownload> {
        let bytes = self
            .asset_bytes
            .lock()
            .unwrap()
            .iter()
            .find(|(id, _)| id == asset_id)
            .map(|(_, bytes)| bytes.clone())
            .unwrap_or_default();
        Ok(AssetDownload {
            content_type: "image/png".to_string(),
            bytes,
        })
    }
}

impl MockSyncApi {
    pub fn uploaded_asset_bytes(&self, asset_id: &str) -> Option<Vec<u8>> {
        self.asset_bytes
            .lock()
            .unwrap()
            .iter()
            .find(|(id, _)| id == asset_id)
            .map(|(_, bytes)| bytes.clone())
    }

    pub fn uploaded_assets(&self) -> Vec<AssetUploadPayload> {
        self.assets.lock().unwrap().clone()
    }

    pub fn notes(&self) -> Vec<Note> {
        self.notes.lock().unwrap().clone()
    }
}

/// 从 `token:acct_xxx` 格式的 Bearer token 中解析账户 ID。
fn account_id_from_token(token: &str) -> Option<String> {
    token.strip_prefix("token:").map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use snapline_domain::{NoteChangePayload, NoteId};

    #[tokio::test]
    async fn mock_accepts_first_note_push() {
        let api = MockSyncApi::default();
        let note = Note::draft(Utc::now());
        let response = api
            .push(
                "token",
                PushRequest {
                    device_id: "device-a".to_string(),
                    changes: vec![PushChange {
                        queue_id: "q1".to_string(),
                        note_id: note.id.clone(),
                        base_version: 0,
                        payload: SyncPayload::Note(NoteChangePayload::from_note(&note)),
                    }],
                },
            )
            .await
            .unwrap();

        assert!(matches!(
            response.results[0],
            PushChangeResult::Accepted {
                server_version: 1,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn mock_reports_version_conflict() {
        let api = MockSyncApi::default();
        let note = Note::draft(Utc::now());
        let note_id: NoteId = note.id.clone();
        let payload = SyncPayload::Note(NoteChangePayload::from_note(&note));
        api.push(
            "token",
            PushRequest {
                device_id: "device-a".to_string(),
                changes: vec![PushChange {
                    queue_id: "q1".to_string(),
                    note_id: note_id.clone(),
                    base_version: 0,
                    payload: payload.clone(),
                }],
            },
        )
        .await
        .unwrap();

        let conflict = api
            .push(
                "token",
                PushRequest {
                    device_id: "device-b".to_string(),
                    changes: vec![PushChange {
                        queue_id: "q2".to_string(),
                        note_id,
                        base_version: 0,
                        payload,
                    }],
                },
            )
            .await
            .unwrap();

        assert!(matches!(
            conflict.results[0],
            PushChangeResult::Conflict { .. }
        ));
    }
}

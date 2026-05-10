/// 同步处理器：读取本地变更队列，推送到服务端并处理响应（接受/冲突）。
///
/// 这些函数是纯业务逻辑，不持有任何状态，可在后台任务中按需调用。
mod assets;
mod crypto;
mod full;
mod pull;
mod push;
mod snapshot;

pub use assets::upload_pending_assets;
pub use full::run_full_sync_from_path;
pub use pull::pull_remote_changes;
pub use push::push_pending_changes;
pub use snapshot::import_snapshot_and_assets;

use std::path::Path;

/// 单次同步操作的统计报告。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProcessReport {
    /// 被服务端接受的变更数量。
    pub accepted: usize,
    /// 发生冲突（服务端版本更新）的变更数量。
    pub conflicts: usize,
    /// 推送失败的变更数量。
    pub failed: usize,
}

/// 单次完整同步的统计报告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullSyncReport {
    pub uploaded_assets: usize,
    pub pushed: usize,
    pub pulled: usize,
    pub conflicts: usize,
    pub failed: usize,
}

pub struct FullSyncContext<'a> {
    pub token: &'a str,
    pub device_id: &'a str,
    pub data_dir: &'a Path,
    pub dek: Option<&'a [u8; 32]>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockSyncApi;
    use crate::protocol::{PushChange, PushRequest};
    use crate::SyncApi;
    use chrono::Utc;
    use snapline_domain::{Note, NoteChangePayload, SyncOpType, SyncPayload};
    use snapline_storage::NoteRepository;

    #[tokio::test]
    async fn processor_deletes_accepted_queue_items() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut note = Note::draft(Utc::now());
        note.owner_account_id = Some("acct_a".to_string());
        let payload = SyncPayload::Note(NoteChangePayload::from_note(&note));
        repo.apply_remote_note(&note).unwrap();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        repo.enqueue_change(
            Some("acct_a"),
            &note.id,
            SyncOpType::UpsertNote,
            0,
            &payload,
            Utc::now(),
        )
        .unwrap();

        let api = MockSyncApi::default();
        let report = push_pending_changes(&repo, &api, "token:acct_a", "device-a", None)
            .await
            .unwrap();

        assert_eq!(report.accepted, 1);
        assert!(repo
            .list_pending_changes(Some("acct_a"), 10)
            .unwrap()
            .is_empty());
        assert_eq!(repo.get_note(&note.id).unwrap().server_version, 1);
        assert_eq!(repo.get_or_create_sync_state().unwrap().server_cursor, 1);
    }

    #[tokio::test]
    async fn processor_uploads_asset_queue_items_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        let mut note = Note::draft(Utc::now());
        note.owner_account_id = Some("acct_a".to_string());
        let markdown_path = format!("assets/notes/{}/asset.png", note.id);
        let asset_path = dir.path().join(&markdown_path);
        std::fs::create_dir_all(asset_path.parent().unwrap()).unwrap();
        std::fs::write(&asset_path, [137, 80, 78, 71]).unwrap();
        let payload = SyncPayload::Asset(snapline_domain::AssetUploadPayload {
            asset_id: snapline_domain::AssetId::new(),
            note_id: note.id.clone(),
            content_type: "image/png".to_string(),
            byte_size: 4,
            sha256: "sha".to_string(),
            markdown_path,
        });
        repo.enqueue_change(
            Some("acct_a"),
            &note.id,
            SyncOpType::AssetUpload,
            0,
            &payload,
            Utc::now(),
        )
        .unwrap();

        let api = MockSyncApi::default();
        let report = upload_pending_assets(&repo, &api, "token:acct_a", dir.path())
            .await
            .unwrap();

        assert_eq!(report.accepted, 1);
        assert!(repo
            .list_pending_changes(Some("acct_a"), 10)
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn processor_pulls_remote_notes_into_repository() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        let api = MockSyncApi::default();
        let mut note = Note::draft(Utc::now());
        note.title = "Remote".to_string();
        let payload = SyncPayload::Note(NoteChangePayload::from_note(&note));
        api.push(
            "token:acct_a",
            PushRequest {
                device_id: "device-b".to_string(),
                changes: vec![PushChange {
                    queue_id: "q1".to_string(),
                    note_id: note.id.clone(),
                    base_version: 0,
                    payload,
                }],
            },
        )
        .await
        .unwrap();

        let report = pull_remote_changes(&repo, &api, "token:acct_a", "device-a", None)
            .await
            .unwrap();

        assert_eq!(report.accepted, 1);
        assert_eq!(repo.get_note(&note.id).unwrap().title, "Remote");
        assert_eq!(repo.get_or_create_sync_state().unwrap().server_cursor, 1);
    }

    #[tokio::test]
    async fn processor_creates_conflict_copy_for_rejected_local_edit() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let api = MockSyncApi::default();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        let mut server_note = Note::draft(Utc::now());
        server_note.owner_account_id = Some("acct_a".to_string());
        server_note.title = "Server".to_string();
        api.push(
            "token:acct_a",
            PushRequest {
                device_id: "device-a".to_string(),
                changes: vec![PushChange {
                    queue_id: "q1".to_string(),
                    note_id: server_note.id.clone(),
                    base_version: 0,
                    payload: SyncPayload::Note(NoteChangePayload::from_note(&server_note)),
                }],
            },
        )
        .await
        .unwrap();
        let mut local_note = server_note.clone();
        local_note.title = "Local".to_string();
        local_note.content_md = "# Local\n![img](assets/notes/local/image.png)".to_string();
        repo.apply_remote_note(&local_note).unwrap();
        repo.enqueue_change(
            Some("acct_a"),
            &local_note.id,
            SyncOpType::UpsertNote,
            0,
            &SyncPayload::Note(NoteChangePayload::from_note(&local_note)),
            Utc::now(),
        )
        .unwrap();

        let report = push_pending_changes(&repo, &api, "token:acct_a", "device-b", None)
            .await
            .unwrap();

        assert_eq!(report.conflicts, 1);
        let summaries = repo.list_recent_for_owner(10, Some("acct_a")).unwrap();
        let conflict_copy = summaries.iter().find(|note| note.is_conflict_copy).unwrap();
        assert_eq!(conflict_copy.source_note_id.as_ref(), Some(&local_note.id));
        let loaded_copy = repo.get_note(&conflict_copy.id).unwrap();
        assert_eq!(loaded_copy.content_md, local_note.content_md);
        assert_eq!(repo.get_note(&local_note.id).unwrap().title, "Server");
        assert!(repo
            .list_pending_changes(Some("acct_a"), 10)
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn processor_preserves_unsynced_local_edit_when_pull_advances_same_note() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let api = MockSyncApi::default();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        let mut local_note = Note::draft(Utc::now());
        local_note.owner_account_id = Some("acct_a".to_string());
        local_note.title = "Local draft".to_string();
        repo.apply_remote_note(&local_note).unwrap();
        repo.enqueue_change(
            Some("acct_a"),
            &local_note.id,
            SyncOpType::UpsertNote,
            0,
            &SyncPayload::Note(NoteChangePayload::from_note(&local_note)),
            Utc::now(),
        )
        .unwrap();
        let mut remote_note = local_note.clone();
        remote_note.title = "Remote accepted".to_string();
        api.push(
            "token:acct_a",
            PushRequest {
                device_id: "device-b".to_string(),
                changes: vec![PushChange {
                    queue_id: "q1".to_string(),
                    note_id: remote_note.id.clone(),
                    base_version: 0,
                    payload: SyncPayload::Note(NoteChangePayload::from_note(&remote_note)),
                }],
            },
        )
        .await
        .unwrap();

        let report = pull_remote_changes(&repo, &api, "token:acct_a", "device-a", None)
            .await
            .unwrap();

        assert_eq!(report.conflicts, 1);
        assert_eq!(
            repo.get_note(&local_note.id).unwrap().title,
            "Remote accepted"
        );
        let conflict_copy = repo
            .list_recent_for_owner(10, Some("acct_a"))
            .unwrap()
            .into_iter()
            .find(|note| note.is_conflict_copy)
            .unwrap();
        assert_eq!(conflict_copy.source_note_id.as_ref(), Some(&local_note.id));
        let loaded_copy = repo.get_note(&conflict_copy.id).unwrap();
        assert!(loaded_copy.title.contains("Conflict copy"));
        assert_eq!(loaded_copy.content_md, local_note.content_md);
        assert!(repo
            .list_pending_changes(Some("acct_a"), 10)
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn processor_ignores_pulled_changes_from_current_device() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let api = MockSyncApi::default();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        let mut note = Note::draft(Utc::now());
        note.owner_account_id = Some("acct_a".to_string());
        note.title = "Local accepted".to_string();
        repo.apply_remote_note(&note).unwrap();
        repo.enqueue_change(
            Some("acct_a"),
            &note.id,
            SyncOpType::UpsertNote,
            0,
            &SyncPayload::Note(NoteChangePayload::from_note(&note)),
            Utc::now(),
        )
        .unwrap();
        let push_report = push_pending_changes(&repo, &api, "token:acct_a", "device-a", None)
            .await
            .unwrap();
        assert_eq!(push_report.accepted, 1);
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.server_cursor = 0;
        repo.save_sync_state(&state).unwrap();

        let pull_report = pull_remote_changes(&repo, &api, "token:acct_a", "device-a", None)
            .await
            .unwrap();

        assert_eq!(pull_report.accepted, 0);
        assert_eq!(pull_report.conflicts, 0);
        assert_eq!(repo.get_note(&note.id).unwrap().title, "Local accepted");
        assert!(repo
            .list_recent(10)
            .unwrap()
            .iter()
            .all(|note| !note.is_conflict_copy));
    }
}

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
    use crate::protocol::{
        AssetDownload, AssetUploadRequest, LoginRequest, LoginResponse, PullResponse, PushChange,
        PushRequest, PushResponse, SnapshotResponse,
    };
    use crate::SyncApi;
    use anyhow::Result;
    use async_trait::async_trait;
    use chrono::{TimeZone, Utc};
    use snapline_domain::{
        crypto::{decrypt_bytes, encrypt_bytes, encrypt_field, generate_dek},
        AssetId, Note, NoteChangePayload, SyncOpType, SyncPayload,
    };
    use snapline_platform::AppPaths;
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
    async fn processor_encrypts_note_payload_when_dek_is_available() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut note = Note::draft(Utc::now());
        note.owner_account_id = Some("acct_a".to_string());
        note.title = "Private title".to_string();
        note.content_md = "# Private title\nSecret body".to_string();
        repo.apply_remote_note(&note).unwrap();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        repo.enqueue_change(
            Some("acct_a"),
            &note.id,
            SyncOpType::UpsertNote,
            0,
            &SyncPayload::Note(NoteChangePayload::from_note(&note)),
            Utc::now(),
        )
        .unwrap();
        let dek = generate_dek();

        let api = MockSyncApi::default();
        let report = push_pending_changes(&repo, &api, "token:acct_a", "device-a", Some(&dek))
            .await
            .unwrap();

        assert_eq!(report.accepted, 1);
        let stored_on_server = api.notes().pop().unwrap();
        assert_ne!(stored_on_server.title, note.title);
        assert_ne!(stored_on_server.content_md, note.content_md);
        assert_eq!(
            snapline_domain::crypto::decrypt_field(&dek, &stored_on_server.title).unwrap(),
            note.title
        );
        assert_eq!(
            snapline_domain::crypto::decrypt_field(&dek, &stored_on_server.content_md).unwrap(),
            note.content_md
        );
        assert!(repo
            .list_pending_changes(Some("acct_a"), 10)
            .unwrap()
            .is_empty());
        assert_eq!(repo.get_note(&note.id).unwrap().server_version, 1);
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
            asset_id: AssetId::new(),
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
        let report = upload_pending_assets(&repo, &api, "token:acct_a", dir.path(), None)
            .await
            .unwrap();

        assert_eq!(report.accepted, 1);
        assert!(repo
            .list_pending_changes(Some("acct_a"), 10)
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn processor_encrypts_asset_bytes_when_dek_is_available() {
        let dir = tempfile::tempdir().unwrap();
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        let note = Note::draft(Utc::now());
        let asset_id = AssetId::new();
        let markdown_path = format!("assets/notes/{}/{}.png", note.id, asset_id);
        let asset_path = dir.path().join(&markdown_path);
        std::fs::create_dir_all(asset_path.parent().unwrap()).unwrap();
        let plaintext = vec![137, 80, 78, 71, 13, 10, 26, 10];
        std::fs::write(&asset_path, &plaintext).unwrap();
        let payload = SyncPayload::Asset(snapline_domain::AssetUploadPayload {
            asset_id: asset_id.clone(),
            note_id: note.id.clone(),
            content_type: "image/png".to_string(),
            byte_size: plaintext.len() as i64,
            sha256: "local-plaintext-sha".to_string(),
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
        let dek = generate_dek();

        let api = MockSyncApi::default();
        let report = upload_pending_assets(&repo, &api, "token:acct_a", dir.path(), Some(&dek))
            .await
            .unwrap();

        assert_eq!(report.accepted, 1);
        let uploaded = api.uploaded_asset_bytes(&asset_id.to_string()).unwrap();
        assert_ne!(uploaded, plaintext);
        assert_eq!(decrypt_bytes(&dek, &uploaded).unwrap(), plaintext);
        let metadata = api.uploaded_assets().pop().unwrap();
        assert_eq!(metadata.byte_size, uploaded.len() as i64);
        assert_ne!(metadata.sha256, "local-plaintext-sha");
    }

    #[tokio::test]
    async fn processor_keeps_note_queue_when_push_transport_fails() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut note = Note::draft(Utc::now());
        note.owner_account_id = Some("acct_a".to_string());
        repo.apply_remote_note(&note).unwrap();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        repo.enqueue_change(
            Some("acct_a"),
            &note.id,
            SyncOpType::UpsertNote,
            0,
            &SyncPayload::Note(NoteChangePayload::from_note(&note)),
            Utc::now(),
        )
        .unwrap();
        let api = FailingSyncApi::push("network unavailable");

        let result = push_pending_changes(&repo, &api, "token:acct_a", "device-a", None).await;

        assert!(result.is_err());
        let pending = repo.list_pending_changes(Some("acct_a"), 10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].note_id, note.id);
        assert_eq!(repo.get_or_create_sync_state().unwrap().server_cursor, 0);
    }

    #[tokio::test]
    async fn processor_keeps_asset_queue_when_upload_source_file_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        let note = Note::draft(Utc::now());
        let asset_id = AssetId::new();
        repo.enqueue_change(
            Some("acct_a"),
            &note.id,
            SyncOpType::AssetUpload,
            0,
            &SyncPayload::Asset(snapline_domain::AssetUploadPayload {
                asset_id,
                note_id: note.id.clone(),
                content_type: "image/png".to_string(),
                byte_size: 4,
                sha256: "sha".to_string(),
                markdown_path: format!("assets/notes/{}/missing.png", note.id),
            }),
            Utc::now(),
        )
        .unwrap();

        let result = upload_pending_assets(
            &repo,
            &MockSyncApi::default(),
            "token:acct_a",
            dir.path(),
            None,
        )
        .await;

        assert!(result.is_err());
        let pending = repo.list_pending_changes(Some("acct_a"), 10).unwrap();
        assert_eq!(pending.len(), 1);
        assert!(matches!(pending[0].payload, SyncPayload::Asset(_)));
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
    async fn processor_uses_cursor_to_pull_only_new_remote_changes() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        let api = MockSyncApi::default();
        let first = Note::draft(Utc::now());
        let mut second = Note::draft(Utc::now());
        second.title = "Second".to_string();
        for (queue_id, note) in [("q1", first.clone()), ("q2", second.clone())] {
            api.push(
                "token:acct_a",
                PushRequest {
                    device_id: "device-b".to_string(),
                    changes: vec![PushChange {
                        queue_id: queue_id.to_string(),
                        note_id: note.id.clone(),
                        base_version: 0,
                        payload: SyncPayload::Note(NoteChangePayload::from_note(&note)),
                    }],
                },
            )
            .await
            .unwrap();
        }

        let first_pull = pull_remote_changes(&repo, &api, "token:acct_a", "device-a", None)
            .await
            .unwrap();
        let second_pull = pull_remote_changes(&repo, &api, "token:acct_a", "device-a", None)
            .await
            .unwrap();

        assert_eq!(first_pull.accepted, 2);
        assert_eq!(second_pull.accepted, 0);
        assert_eq!(repo.get_or_create_sync_state().unwrap().server_cursor, 2);
        assert_eq!(
            repo.list_recent_for_owner(10, Some("acct_a"))
                .unwrap()
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn processor_keeps_account_scopes_separate_when_pulling() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        let api = MockSyncApi::default();
        let mut acct_a_note = Note::draft(Utc::now());
        acct_a_note.title = "Account A".to_string();
        let mut acct_b_note = Note::draft(Utc::now());
        acct_b_note.title = "Account B".to_string();
        for (token, note) in [
            ("token:acct_a", acct_a_note.clone()),
            ("token:acct_b", acct_b_note),
        ] {
            api.push(
                token,
                PushRequest {
                    device_id: "device-b".to_string(),
                    changes: vec![PushChange {
                        queue_id: note.title.clone(),
                        note_id: note.id.clone(),
                        base_version: 0,
                        payload: SyncPayload::Note(NoteChangePayload::from_note(&note)),
                    }],
                },
            )
            .await
            .unwrap();
        }

        let report = pull_remote_changes(&repo, &api, "token:acct_a", "device-a", None)
            .await
            .unwrap();

        assert_eq!(report.accepted, 1);
        let summaries = repo.list_recent_for_owner(10, Some("acct_a")).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].title, "Account A");
        assert!(!repo
            .note_exists(&acct_b_note_id(&api, "Account B"))
            .unwrap());
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

    #[tokio::test]
    async fn processor_decrypts_snapshot_notes_when_dek_is_available() {
        let dir = tempfile::tempdir().unwrap();
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        let dek = generate_dek();
        let api = MockSyncApi::default();
        let mut note = Note::draft(Utc::now());
        note.owner_account_id = Some("acct_a".to_string());
        let plaintext_title = "Remote encrypted title";
        let plaintext_body = "# Remote encrypted body";
        let payload = SyncPayload::Note(NoteChangePayload {
            title: encrypt_field(&dek, plaintext_title).unwrap(),
            content_md: encrypt_field(&dek, plaintext_body).unwrap(),
            pinned: false,
            deleted_at: None,
        });
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

        import_snapshot_and_assets(&repo, &api, "token:acct_a", dir.path(), Some(&dek))
            .await
            .unwrap();

        let imported = repo.get_note(&note.id).unwrap();
        assert_eq!(imported.title, plaintext_title);
        assert_eq!(imported.content_md, plaintext_body);
    }

    #[tokio::test]
    async fn processor_imports_legacy_plaintext_snapshot_with_dek() {
        let dir = tempfile::tempdir().unwrap();
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        let dek = generate_dek();
        let api = MockSyncApi::default();
        let mut note = Note::draft(Utc::now());
        note.owner_account_id = Some("acct_a".to_string());
        note.title = "中文标题".to_string();
        note.content_md = "# 中文正文".to_string();
        api.push(
            "token:acct_a",
            PushRequest {
                device_id: "device-b".to_string(),
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

        import_snapshot_and_assets(&repo, &api, "token:acct_a", dir.path(), Some(&dek))
            .await
            .unwrap();

        let imported = repo.get_note(&note.id).unwrap();
        assert_eq!(imported.title, "中文标题");
        assert_eq!(imported.content_md, "# 中文正文");
    }

    #[tokio::test]
    async fn processor_imports_snapshot_assets_and_decrypts_downloaded_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        let api = MockSyncApi::default();
        let dek = generate_dek();
        let mut note = Note::draft(Utc::now());
        note.owner_account_id = Some("acct_a".to_string());
        let asset_id = AssetId::new();
        let plaintext = vec![1, 2, 3, 4, 5, 6];
        let encrypted = encrypt_bytes(&dek, &plaintext).unwrap();
        api.upload_asset(
            "token:acct_a",
            AssetUploadRequest {
                metadata: snapline_domain::AssetUploadPayload {
                    asset_id: asset_id.clone(),
                    note_id: note.id.clone(),
                    content_type: "image/png".to_string(),
                    byte_size: encrypted.len() as i64,
                    sha256: "encrypted-sha".to_string(),
                    markdown_path: AppPaths::from_data_dir(dir.path())
                        .markdown_asset_path(&note.id, &asset_id, "png"),
                },
                bytes: encrypted,
            },
        )
        .await
        .unwrap();
        api.push(
            "token:acct_a",
            PushRequest {
                device_id: "device-b".to_string(),
                changes: vec![PushChange {
                    queue_id: "note".to_string(),
                    note_id: note.id.clone(),
                    base_version: 0,
                    payload: SyncPayload::Note(NoteChangePayload::from_note(&note)),
                }],
            },
        )
        .await
        .unwrap();

        import_snapshot_and_assets(&repo, &api, "token:acct_a", dir.path(), Some(&dek))
            .await
            .unwrap();

        let path = AppPaths::from_data_dir(dir.path()).note_asset_path(&note.id, &asset_id, "png");
        assert_eq!(std::fs::read(path).unwrap(), plaintext);
        assert_eq!(repo.get_or_create_sync_state().unwrap().server_cursor, 1);
    }

    #[tokio::test]
    async fn processor_snapshot_conflict_keeps_asset_upload_queue() {
        let dir = tempfile::tempdir().unwrap();
        let repo = NoteRepository::open_in_memory().unwrap();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        let api = MockSyncApi::default();
        let mut local_note = Note::draft(Utc::now());
        local_note.owner_account_id = Some("acct_a".to_string());
        local_note.title = "Local asset-only".to_string();
        repo.apply_remote_note(&local_note).unwrap();
        repo.enqueue_change(
            Some("acct_a"),
            &local_note.id,
            SyncOpType::AssetUpload,
            0,
            &SyncPayload::Asset(snapline_domain::AssetUploadPayload {
                asset_id: AssetId::new(),
                note_id: local_note.id.clone(),
                content_type: "image/png".to_string(),
                byte_size: 4,
                sha256: "sha".to_string(),
                markdown_path: format!("assets/notes/{}/local.png", local_note.id),
            }),
            Utc::now(),
        )
        .unwrap();
        let mut remote_note = local_note.clone();
        remote_note.title = "Remote snapshot".to_string();
        api.push(
            "token:acct_a",
            PushRequest {
                device_id: "device-b".to_string(),
                changes: vec![PushChange {
                    queue_id: "remote".to_string(),
                    note_id: remote_note.id.clone(),
                    base_version: 0,
                    payload: SyncPayload::Note(NoteChangePayload::from_note(&remote_note)),
                }],
            },
        )
        .await
        .unwrap();

        import_snapshot_and_assets(&repo, &api, "token:acct_a", dir.path(), None)
            .await
            .unwrap();

        assert_eq!(
            repo.get_note(&local_note.id).unwrap().title,
            "Remote snapshot"
        );
        assert!(repo
            .list_recent_for_owner(10, Some("acct_a"))
            .unwrap()
            .iter()
            .all(|note| !note.is_conflict_copy));
        let pending = repo.list_pending_changes(Some("acct_a"), 10).unwrap();
        assert_eq!(pending.len(), 1);
        assert!(matches!(pending[0].payload, SyncPayload::Asset(_)));
    }

    #[tokio::test]
    async fn processor_syncs_soft_deleted_notes() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let api = MockSyncApi::default();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        let mut note = Note::draft(Utc::now());
        note.owner_account_id = Some("acct_a".to_string());
        note.title = "Deleted remotely".to_string();
        note.deleted_at = Some(Utc.with_ymd_and_hms(2026, 5, 12, 4, 0, 0).unwrap());
        api.push(
            "token:acct_a",
            PushRequest {
                device_id: "device-b".to_string(),
                changes: vec![PushChange {
                    queue_id: "delete".to_string(),
                    note_id: note.id.clone(),
                    base_version: 0,
                    payload: SyncPayload::Note(NoteChangePayload::from_note(&note)),
                }],
            },
        )
        .await
        .unwrap();

        let report = pull_remote_changes(&repo, &api, "token:acct_a", "device-a", None)
            .await
            .unwrap();

        assert_eq!(report.accepted, 1);
        assert!(repo.get_note(&note.id).unwrap().deleted_at.is_some());
        assert!(repo
            .list_recent_for_owner(10, Some("acct_a"))
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn full_sync_uploads_pushes_pulls_and_imports_snapshot_assets() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("snapline.db");
        let repo = NoteRepository::open(&db_path).unwrap();
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.account_id = Some("acct_a".to_string());
        repo.save_sync_state(&state).unwrap();
        let mut local_note = Note::draft(Utc::now());
        local_note.owner_account_id = Some("acct_a".to_string());
        local_note.title = "Local".to_string();
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
        let asset_id = AssetId::new();
        let markdown_path = format!("assets/notes/{}/{}.png", local_note.id, asset_id);
        let asset_path = dir.path().join(&markdown_path);
        std::fs::create_dir_all(asset_path.parent().unwrap()).unwrap();
        std::fs::write(&asset_path, [1, 2, 3, 4]).unwrap();
        repo.enqueue_change(
            Some("acct_a"),
            &local_note.id,
            SyncOpType::AssetUpload,
            0,
            &SyncPayload::Asset(snapline_domain::AssetUploadPayload {
                asset_id: asset_id.clone(),
                note_id: local_note.id.clone(),
                content_type: "image/png".to_string(),
                byte_size: 4,
                sha256: "stale".to_string(),
                markdown_path,
            }),
            Utc::now(),
        )
        .unwrap();
        let api = MockSyncApi::default();
        let mut remote_note = Note::draft(Utc::now());
        remote_note.title = "Remote".to_string();
        api.push(
            "token:acct_a",
            PushRequest {
                device_id: "device-b".to_string(),
                changes: vec![PushChange {
                    queue_id: "remote".to_string(),
                    note_id: remote_note.id.clone(),
                    base_version: 0,
                    payload: SyncPayload::Note(NoteChangePayload::from_note(&remote_note)),
                }],
            },
        )
        .await
        .unwrap();

        let report = run_full_sync_from_path(
            &db_path,
            &api,
            FullSyncContext {
                token: "token:acct_a",
                device_id: "device-a",
                data_dir: dir.path(),
                dek: None,
            },
        )
        .await
        .unwrap();
        let repo = NoteRepository::open(&db_path).unwrap();

        assert_eq!(report.uploaded_assets, 1);
        assert_eq!(report.pushed, 1);
        assert_eq!(report.pulled, 0);
        assert_eq!(report.conflicts, 0);
        assert_eq!(
            repo.list_pending_changes(Some("acct_a"), 10).unwrap().len(),
            0
        );
        assert_eq!(repo.get_note(&local_note.id).unwrap().server_version, 1);
        assert_eq!(repo.get_note(&remote_note.id).unwrap().title, "Remote");
    }

    #[tokio::test]
    async fn sync_operations_require_logged_in_account() {
        let dir = tempfile::tempdir().unwrap();
        let repo = NoteRepository::open_in_memory().unwrap();
        let api = MockSyncApi::default();

        assert!(
            push_pending_changes(&repo, &api, "token:acct_a", "device-a", None)
                .await
                .is_err()
        );
        assert!(
            pull_remote_changes(&repo, &api, "token:acct_a", "device-a", None)
                .await
                .is_err()
        );
        assert!(
            upload_pending_assets(&repo, &api, "token:acct_a", dir.path(), None)
                .await
                .is_err()
        );
        assert!(
            import_snapshot_and_assets(&repo, &api, "token:acct_a", dir.path(), None)
                .await
                .is_err()
        );
    }

    fn acct_b_note_id(api: &MockSyncApi, title: &str) -> snapline_domain::NoteId {
        api.notes()
            .into_iter()
            .find(|note| note.title == title)
            .unwrap()
            .id
    }

    #[derive(Default)]
    struct FailingSyncApi {
        push_error: Option<String>,
    }

    impl FailingSyncApi {
        fn push(message: &str) -> Self {
            Self {
                push_error: Some(message.to_string()),
            }
        }
    }

    #[async_trait]
    impl SyncApi for FailingSyncApi {
        async fn register(&self, _request: LoginRequest) -> Result<LoginResponse> {
            anyhow::bail!("not implemented")
        }

        async fn login(&self, _request: LoginRequest) -> Result<LoginResponse> {
            anyhow::bail!("not implemented")
        }

        async fn push(&self, _token: &str, _request: PushRequest) -> Result<PushResponse> {
            if let Some(message) = &self.push_error {
                anyhow::bail!(message.clone());
            }
            Ok(PushResponse { results: vec![] })
        }

        async fn pull(&self, _token: &str, _cursor: i64) -> Result<PullResponse> {
            anyhow::bail!("not implemented")
        }

        async fn snapshot(&self, _token: &str) -> Result<SnapshotResponse> {
            Ok(SnapshotResponse {
                cursor: 0,
                notes: vec![],
                assets: vec![],
            })
        }

        async fn upload_asset(&self, _token: &str, _request: AssetUploadRequest) -> Result<()> {
            anyhow::bail!("not implemented")
        }

        async fn download_asset(&self, _token: &str, _asset_id: &str) -> Result<AssetDownload> {
            anyhow::bail!("not implemented")
        }
    }
}

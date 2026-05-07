/// 同步处理器：读取本地变更队列，推送到服务端并处理响应（接受/冲突）。
///
/// 这些函数是纯业务逻辑，不持有任何状态，可在后台任务中按需调用。
use crate::protocol::{AssetUploadRequest, PushChange, PushChangeResult, PushRequest};
use crate::SyncApi;
use anyhow::{Context, Result};
use chrono::Utc;
use snapline_domain::{Note, SyncPayload};
use snapline_storage::NoteRepository;
use std::{fs, path::Path};

/// 单次同步操作的统计报告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessReport {
    /// 被服务端接受的变更数量。
    pub accepted: usize,
    /// 发生冲突（服务端版本更新）的变更数量。
    pub conflicts: usize,
    /// 推送失败的变更数量。
    pub failed: usize,
}

/// 将当前账户的待处理笔记变更批量推送到服务端（每次最多 25 条）。
///
/// 对于每条推送结果：
/// - `Accepted`：更新本地 server_version 和游标，删除队列条目
/// - `Conflict`：保存本地版本为冲突副本，覆写为服务端版本，删除队列条目
pub async fn push_pending_changes<A: SyncApi + Sync>(
    repo: &NoteRepository,
    api: &A,
    token: &str,
    device_id: &str,
) -> Result<ProcessReport> {
    let account_id = repo
        .get_or_create_sync_state()?
        .account_id
        .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
    let pending = repo.list_pending_changes(Some(&account_id), 25)?;
    if pending.is_empty() {
        return Ok(ProcessReport {
            accepted: 0,
            conflicts: 0,
            failed: 0,
        });
    }
    let response = api
        .push(
            token,
            PushRequest {
                device_id: device_id.to_string(),
                changes: pending
                    .iter()
                    .map(|item| PushChange {
                        queue_id: item.id.clone(),
                        note_id: item.note_id.clone(),
                        base_version: item.base_version,
                        payload: item.payload.clone(),
                    })
                    .collect(),
            },
        )
        .await?;
    let mut report = ProcessReport {
        accepted: 0,
        conflicts: 0,
        failed: 0,
    };
    for result in response.results {
        match result {
            PushChangeResult::Accepted {
                queue_id,
                note_id,
                server_version,
                cursor,
            } => {
                repo.update_note_server_version(&note_id, server_version)?;
                repo.update_sync_cursor_success(cursor, Utc::now())?;
                repo.delete_change(&queue_id)?;
                report.accepted += 1;
            }
            PushChangeResult::Conflict {
                queue_id,
                note_id,
                server_note,
            } => {
                // 将本地编辑版本另存为冲突副本，再用服务端版本覆写本地
                if let Some(rejected_note) = local_note_from_pending(&pending, &queue_id, &note_id)
                {
                    repo.create_conflict_copy(&rejected_note, Utc::now())?;
                    repo.apply_remote_note(&server_note)?;
                }
                repo.delete_change(&queue_id)?;
                report.conflicts += 1;
            }
        }
    }
    Ok(report)
}

/// 将当前账户的待上传资源文件逐一读取并上传到服务端。
///
/// 文件路径从 `AssetUploadPayload.markdown_path` 中解析，相对于 `data_dir`。
pub async fn upload_pending_assets<A: SyncApi + Sync>(
    repo: &NoteRepository,
    api: &A,
    token: &str,
    data_dir: &Path,
) -> Result<ProcessReport> {
    let account_id = repo
        .get_or_create_sync_state()?
        .account_id
        .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
    let pending = repo.list_pending_changes(Some(&account_id), 100)?;
    let mut report = ProcessReport {
        accepted: 0,
        conflicts: 0,
        failed: 0,
    };
    for item in pending {
        let SyncPayload::Asset(metadata) = item.payload else {
            continue; // 跳过笔记变更条目，只处理资源上传
        };
        let asset_path = data_dir.join(&metadata.markdown_path);
        let bytes = fs::read(&asset_path)
            .with_context(|| format!("failed to read asset {}", asset_path.display()))?;
        api.upload_asset(token, AssetUploadRequest { metadata, bytes })
            .await?;
        repo.delete_change(&item.id)?;
        report.accepted += 1;
    }
    Ok(report)
}

/// 从服务端拉取增量变更并写入本地数据库。
///
/// 来自当前设备的变更会被跳过（避免回显覆盖本地未提交内容）。
/// 若本地对同一笔记有未推送的变更，则创建冲突副本后覆写。
pub async fn pull_remote_changes<A: SyncApi + Sync>(
    repo: &NoteRepository,
    api: &A,
    token: &str,
    device_id: &str,
) -> Result<ProcessReport> {
    let state = repo.get_or_create_sync_state()?;
    let account_id = state
        .account_id
        .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
    let response = api.pull(token, state.server_cursor).await?;
    let mut conflicts = 0;
    let mut accepted = 0;
    for change in &response.changes {
        if change.device_id == device_id {
            continue; // 跳过本设备推送产生的回显
        }
        if repo.has_pending_note_change(Some(&account_id), &change.note.id)? {
            // 本地有未推送变更，先保存副本再接受远端版本
            let local_note = repo.get_note_for_owner(&change.note.id, Some(&account_id))?;
            repo.create_conflict_copy(&local_note, Utc::now())?;
            repo.delete_changes_for_note(Some(&account_id), &change.note.id)?;
            conflicts += 1;
        }
        repo.apply_remote_note(&change.note)?;
        accepted += 1;
    }
    repo.update_sync_cursor_success(response.cursor, Utc::now())?;
    Ok(ProcessReport {
        accepted,
        conflicts,
        failed: 0,
    })
}

/// 从待处理队列中找到对应 queue_id 的本地笔记快照（用于冲突副本创建）。
fn local_note_from_pending(
    pending: &[snapline_storage::ChangeQueueItem],
    queue_id: &str,
    note_id: &snapline_domain::NoteId,
) -> Option<Note> {
    let item = pending.iter().find(|item| item.id == queue_id)?;
    let SyncPayload::Note(payload) = &item.payload else {
        return None;
    };
    let now = Utc::now();
    Some(Note {
        id: note_id.clone(),
        title: payload.title.clone(),
        content_md: payload.content_md.clone(),
        pinned: payload.pinned,
        created_at: now,
        updated_at: now,
        deleted_at: payload.deleted_at,
        server_version: item.base_version,
        last_modified_by_device: None,
        is_conflict_copy: false,
        source_note_id: None,
        owner_account_id: item.account_id.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockSyncApi;
    use chrono::Utc;
    use snapline_domain::{Note, NoteChangePayload, SyncOpType, SyncPayload};

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
        let report = push_pending_changes(&repo, &api, "token:acct_a", "device-a")
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

        let report = pull_remote_changes(&repo, &api, "token:acct_a", "device-a")
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

        let report = push_pending_changes(&repo, &api, "token:acct_a", "device-b")
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

        let report = pull_remote_changes(&repo, &api, "token:acct_a", "device-a")
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
        let push_report = push_pending_changes(&repo, &api, "token:acct_a", "device-a")
            .await
            .unwrap();
        assert_eq!(push_report.accepted, 1);
        let mut state = repo.get_or_create_sync_state().unwrap();
        state.server_cursor = 0;
        repo.save_sync_state(&state).unwrap();

        let pull_report = pull_remote_changes(&repo, &api, "token:acct_a", "device-a")
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

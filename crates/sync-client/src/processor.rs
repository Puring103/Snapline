use crate::protocol::{AssetUploadRequest, PushChange, PushChangeResult, PushRequest};
use crate::SyncApi;
use anyhow::{Context, Result};
use chrono::Utc;
use snapline_domain::{Note, SyncPayload};
use snapline_storage::NoteRepository;
use std::{fs, path::Path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessReport {
    pub accepted: usize,
    pub conflicts: usize,
    pub failed: usize,
}

pub async fn push_pending_changes<A: SyncApi + Sync>(
    repo: &NoteRepository,
    api: &A,
    token: &str,
    device_id: &str,
) -> Result<ProcessReport> {
    let pending = repo.list_pending_changes(25)?;
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
                if let Some(rejected_note) = local_note_from_pending(&pending, &queue_id, &note_id) {
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

pub async fn upload_pending_assets<A: SyncApi + Sync>(
    repo: &NoteRepository,
    api: &A,
    token: &str,
    data_dir: &Path,
) -> Result<ProcessReport> {
    let pending = repo.list_pending_changes(100)?;
    let mut report = ProcessReport {
        accepted: 0,
        conflicts: 0,
        failed: 0,
    };
    for item in pending {
        let SyncPayload::Asset(metadata) = item.payload else {
            continue;
        };
        let asset_path = data_dir.join(&metadata.markdown_path);
        let bytes = fs::read(&asset_path)
            .with_context(|| format!("failed to read asset {}", asset_path.display()))?;
        api.upload_asset(&token, AssetUploadRequest { metadata, bytes })
            .await?;
        repo.delete_change(&item.id)?;
        report.accepted += 1;
    }
    Ok(report)
}

pub async fn pull_remote_changes<A: SyncApi + Sync>(
    repo: &NoteRepository,
    api: &A,
    token: &str,
) -> Result<ProcessReport> {
    let state = repo.get_or_create_sync_state()?;
    let response = api.pull(token, state.server_cursor).await?;
    let mut conflicts = 0;
    for change in &response.changes {
        if repo.has_pending_note_change(&change.note.id)? {
            let local_note = repo.get_note(&change.note.id)?;
            repo.create_conflict_copy(&local_note, Utc::now())?;
            repo.delete_changes_for_note(&change.note.id)?;
            conflicts += 1;
        }
        repo.apply_remote_note(&change.note)?;
    }
    repo.update_sync_cursor_success(response.cursor, Utc::now())?;
    Ok(ProcessReport {
        accepted: response.changes.len(),
        conflicts,
        failed: 0,
    })
}

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
        let note = Note::draft(Utc::now());
        let payload = SyncPayload::Note(NoteChangePayload::from_note(&note));
        repo.apply_remote_note(&note).unwrap();
        repo.enqueue_change(&note.id, SyncOpType::UpsertNote, 0, &payload, Utc::now())
            .unwrap();

        let api = MockSyncApi::default();
        let report = push_pending_changes(&repo, &api, "token", "device-a")
            .await
            .unwrap();

        assert_eq!(report.accepted, 1);
        assert!(repo.list_pending_changes(10).unwrap().is_empty());
        assert_eq!(repo.get_note(&note.id).unwrap().server_version, 1);
        assert_eq!(repo.get_or_create_sync_state().unwrap().server_cursor, 1);
    }

    #[tokio::test]
    async fn processor_uploads_asset_queue_items_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let repo = NoteRepository::open_in_memory().unwrap();
        let note = Note::draft(Utc::now());
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
        repo.enqueue_change(&note.id, SyncOpType::AssetUpload, 0, &payload, Utc::now())
            .unwrap();

        let api = MockSyncApi::default();
        let report = upload_pending_assets(&repo, &api, "token", dir.path())
            .await
            .unwrap();

        assert_eq!(report.accepted, 1);
        assert!(repo.list_pending_changes(10).unwrap().is_empty());
    }

    #[tokio::test]
    async fn processor_pulls_remote_notes_into_repository() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let api = MockSyncApi::default();
        let mut note = Note::draft(Utc::now());
        note.title = "Remote".to_string();
        let payload = SyncPayload::Note(NoteChangePayload::from_note(&note));
        api.push(
            "token",
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

        let report = pull_remote_changes(&repo, &api, "token").await.unwrap();

        assert_eq!(report.accepted, 1);
        assert_eq!(repo.get_note(&note.id).unwrap().title, "Remote");
        assert_eq!(repo.get_or_create_sync_state().unwrap().server_cursor, 1);
    }

    #[tokio::test]
    async fn processor_creates_conflict_copy_for_rejected_local_edit() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let api = MockSyncApi::default();
        let mut server_note = Note::draft(Utc::now());
        server_note.title = "Server".to_string();
        api.push(
            "token",
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
            &local_note.id,
            SyncOpType::UpsertNote,
            0,
            &SyncPayload::Note(NoteChangePayload::from_note(&local_note)),
            Utc::now(),
        )
        .unwrap();

        let report = push_pending_changes(&repo, &api, "token", "device-b").await.unwrap();

        assert_eq!(report.conflicts, 1);
        let summaries = repo.list_recent(10).unwrap();
        let conflict_copy = summaries.iter().find(|note| note.is_conflict_copy).unwrap();
        assert_eq!(conflict_copy.source_note_id.as_ref(), Some(&local_note.id));
        let loaded_copy = repo.get_note(&conflict_copy.id).unwrap();
        assert_eq!(loaded_copy.content_md, local_note.content_md);
        assert_eq!(repo.get_note(&local_note.id).unwrap().title, "Server");
        assert!(repo.list_pending_changes(10).unwrap().is_empty());
    }

    #[tokio::test]
    async fn processor_preserves_unsynced_local_edit_when_pull_advances_same_note() {
        let repo = NoteRepository::open_in_memory().unwrap();
        let api = MockSyncApi::default();
        let mut local_note = Note::draft(Utc::now());
        local_note.title = "Local draft".to_string();
        repo.apply_remote_note(&local_note).unwrap();
        repo.enqueue_change(
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
            "token",
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

        let report = pull_remote_changes(&repo, &api, "token").await.unwrap();

        assert_eq!(report.conflicts, 1);
        assert_eq!(repo.get_note(&local_note.id).unwrap().title, "Remote accepted");
        let conflict_copy = repo
            .list_recent(10)
            .unwrap()
            .into_iter()
            .find(|note| note.is_conflict_copy)
            .unwrap();
        assert_eq!(conflict_copy.source_note_id.as_ref(), Some(&local_note.id));
        let loaded_copy = repo.get_note(&conflict_copy.id).unwrap();
        assert!(loaded_copy.title.contains("Conflict copy"));
        assert_eq!(loaded_copy.content_md, local_note.content_md);
        assert!(repo.list_pending_changes(10).unwrap().is_empty());
    }
}

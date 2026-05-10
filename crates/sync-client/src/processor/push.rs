use crate::processor::crypto::{decrypt_note, encrypt_note_payload, local_note_from_pending};
use crate::processor::ProcessReport;
use crate::protocol::{PushChange, PushChangeResult, PushRequest};
use crate::SyncApi;
use anyhow::Result;
use chrono::Utc;
use snapline_domain::SyncPayload;
use snapline_storage::NoteRepository;
use std::path::Path;

pub(super) async fn push_pending_changes_from_path<A: SyncApi + Sync>(
    db_path: &Path,
    api: &A,
    token: &str,
    device_id: &str,
    dek: Option<&[u8; 32]>,
) -> Result<ProcessReport> {
    let (pending, wire_changes) = {
        let repo = NoteRepository::open(db_path)?;
        let account_id = repo
            .get_or_create_sync_state()?
            .account_id
            .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
        let pending = repo.list_pending_changes(Some(&account_id), 25)?;
        let wire_changes = pending_wire_changes(&pending, dek)?;
        (pending, wire_changes)
    };

    if pending.is_empty() {
        return Ok(ProcessReport::default());
    }

    let response = api
        .push(
            token,
            PushRequest {
                device_id: device_id.to_string(),
                changes: wire_changes,
            },
        )
        .await?;

    let repo = NoteRepository::open(db_path)?;
    let mut report = ProcessReport::default();
    let mut max_cursor = 0;
    for result in response.results {
        match result {
            PushChangeResult::Accepted {
                queue_id,
                note_id,
                server_version,
                cursor,
            } => {
                repo.update_note_server_version(&note_id, server_version)?;
                repo.delete_change(&queue_id)?;
                max_cursor = max_cursor.max(cursor);
                report.accepted += 1;
            }
            PushChangeResult::Conflict {
                queue_id,
                note_id,
                server_note,
            } => {
                if let Some(rejected_note) = local_note_from_pending(&pending, &queue_id, &note_id)
                {
                    let decrypted_server = match dek {
                        Some(key) => decrypt_note(key, &server_note)?,
                        None => server_note,
                    };
                    repo.create_conflict_copy(&rejected_note, Utc::now())?;
                    repo.apply_remote_note(&decrypted_server)?;
                }
                repo.delete_change(&queue_id)?;
                report.conflicts += 1;
            }
        }
    }
    if max_cursor > 0 {
        repo.update_sync_cursor_success(max_cursor, Utc::now())?;
    }
    Ok(report)
}

/// 将当前账户的待处理笔记变更批量推送到服务端（每次最多 25 条）。
///
/// `dek` 不为 None 时，推送前加密 title 和 content_md；为 None 则明文推送（兼容旧账户）。
/// 对于每条推送结果：
/// - `Accepted`：更新本地 server_version 和游标，删除队列条目
/// - `Conflict`：保存本地版本为冲突副本，覆写为服务端版本，删除队列条目
pub async fn push_pending_changes<A: SyncApi + Sync>(
    repo: &NoteRepository,
    api: &A,
    token: &str,
    device_id: &str,
    dek: Option<&[u8; 32]>,
) -> Result<ProcessReport> {
    let account_id = repo
        .get_or_create_sync_state()?
        .account_id
        .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
    let pending = repo.list_pending_changes(Some(&account_id), 25)?;
    if pending.is_empty() {
        return Ok(ProcessReport::default());
    }
    let wire_changes = pending_wire_changes(&pending, dek)?;
    let response = api
        .push(
            token,
            PushRequest {
                device_id: device_id.to_string(),
                changes: wire_changes,
            },
        )
        .await?;
    let mut report = ProcessReport::default();
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
                if let Some(rejected_note) = local_note_from_pending(&pending, &queue_id, &note_id)
                {
                    let decrypted_server = match dek {
                        Some(key) => decrypt_note(key, &server_note)?,
                        None => server_note,
                    };
                    repo.create_conflict_copy(&rejected_note, Utc::now())?;
                    repo.apply_remote_note(&decrypted_server)?;
                }
                repo.delete_change(&queue_id)?;
                report.conflicts += 1;
            }
        }
    }
    Ok(report)
}

fn pending_wire_changes(
    pending: &[snapline_storage::ChangeQueueItem],
    dek: Option<&[u8; 32]>,
) -> Result<Vec<PushChange>> {
    let mut wire_changes = Vec::with_capacity(pending.len());
    for item in pending {
        let payload = match (&item.payload, dek) {
            (SyncPayload::Note(note_payload), Some(key)) => {
                SyncPayload::Note(encrypt_note_payload(key, note_payload)?)
            }
            _ => item.payload.clone(),
        };
        wire_changes.push(PushChange {
            queue_id: item.id.clone(),
            note_id: item.note_id.clone(),
            base_version: item.base_version,
            payload,
        });
    }
    Ok(wire_changes)
}

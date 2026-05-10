use crate::processor::crypto::decrypt_note;
use crate::processor::ProcessReport;
use crate::SyncApi;
use anyhow::Result;
use chrono::Utc;
use snapline_storage::NoteRepository;
use std::path::Path;

pub(super) async fn pull_remote_changes_from_path<A: SyncApi + Sync>(
    db_path: &Path,
    api: &A,
    token: &str,
    device_id: &str,
    dek: Option<&[u8; 32]>,
) -> Result<ProcessReport> {
    let (cursor, account_id) = {
        let repo = NoteRepository::open(db_path)?;
        let state = repo.get_or_create_sync_state()?;
        (
            state.server_cursor,
            state
                .account_id
                .ok_or_else(|| anyhow::anyhow!("not logged in"))?,
        )
    };
    let response = api.pull(token, cursor).await?;

    let repo = NoteRepository::open(db_path)?;
    apply_pull_response(
        &repo,
        &response.changes,
        response.cursor,
        &account_id,
        device_id,
        dek,
    )
}

/// 从服务端拉取增量变更并写入本地数据库。
///
/// `dek` 不为 None 时，写入本地前解密 title 和 content_md。
/// 来自当前设备的变更会被跳过（避免回显覆盖本地未提交内容）。
/// 若本地对同一笔记有未推送的变更，则创建冲突副本后覆写。
pub async fn pull_remote_changes<A: SyncApi + Sync>(
    repo: &NoteRepository,
    api: &A,
    token: &str,
    device_id: &str,
    dek: Option<&[u8; 32]>,
) -> Result<ProcessReport> {
    let state = repo.get_or_create_sync_state()?;
    let account_id = state
        .account_id
        .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
    let response = api.pull(token, state.server_cursor).await?;
    apply_pull_response(
        repo,
        &response.changes,
        response.cursor,
        &account_id,
        device_id,
        dek,
    )
}

fn apply_pull_response(
    repo: &NoteRepository,
    changes: &[crate::protocol::RemoteChange],
    cursor: i64,
    account_id: &str,
    device_id: &str,
    dek: Option<&[u8; 32]>,
) -> Result<ProcessReport> {
    let mut conflicts = 0;
    let mut accepted = 0;
    for change in changes {
        if change.device_id == device_id {
            continue;
        }
        let note = match dek {
            Some(key) => decrypt_note(key, &change.note)?,
            None => change.note.clone(),
        };
        if repo.has_pending_note_change(Some(account_id), &note.id)? {
            let local_note = repo.get_note_for_owner(&note.id, Some(account_id))?;
            repo.create_conflict_copy(&local_note, Utc::now())?;
            repo.delete_changes_for_note(Some(account_id), &note.id)?;
            conflicts += 1;
        }
        repo.apply_remote_note(&note)?;
        accepted += 1;
    }
    repo.update_sync_cursor_success(cursor, Utc::now())?;
    Ok(ProcessReport {
        accepted,
        conflicts,
        failed: 0,
    })
}

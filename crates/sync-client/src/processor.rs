use crate::protocol::{PushChange, PushChangeResult, PushRequest};
use crate::SyncApi;
use anyhow::Result;
use snapline_storage::NoteRepository;

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
            PushChangeResult::Accepted { queue_id, .. } => {
                repo.delete_change(&queue_id)?;
                report.accepted += 1;
            }
            PushChangeResult::Conflict { queue_id, .. } => {
                repo.mark_change_failed(&queue_id, "version conflict")?;
                report.conflicts += 1;
            }
        }
    }
    Ok(report)
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
        repo.enqueue_change(&note.id, SyncOpType::UpsertNote, 0, &payload, Utc::now())
            .unwrap();

        let api = MockSyncApi::default();
        let report = push_pending_changes(&repo, &api, "token", "device-a")
            .await
            .unwrap();

        assert_eq!(report.accepted, 1);
        assert!(repo.list_pending_changes(10).unwrap().is_empty());
    }
}

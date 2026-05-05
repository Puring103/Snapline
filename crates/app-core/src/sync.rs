use anyhow::Result;
use chrono::Utc;
use snapline_domain::{Note, NoteChangePayload, NoteId, SyncOpType, SyncPayload};

use crate::{AppCore, SyncAccountState};

impl AppCore {
    pub fn sync_account_state(&self) -> Result<SyncAccountState> {
        let state = self.repo.get_or_create_sync_state()?;
        Ok(SyncAccountState {
            account_id: state.account_id,
            device_id: state.device_id,
            server_base_url: state.server_base_url,
            is_logged_in: state.access_token.is_some(),
        })
    }

    pub fn save_sync_login(
        &self,
        server_base_url: &str,
        account_id: &str,
        access_token: &str,
    ) -> Result<SyncAccountState> {
        let mut state = self.repo.get_or_create_sync_state()?;
        state.server_base_url = Some(server_base_url.to_string());
        state.account_id = Some(account_id.to_string());
        state.access_token = Some(access_token.to_string());
        self.repo.save_sync_state(&state)?;
        self.sync_account_state()
    }

    pub(crate) fn current_account_id(&self) -> Result<Option<String>> {
        Ok(self.repo.get_or_create_sync_state()?.account_id)
    }

    pub fn anonymous_note_count(&self) -> Result<usize> {
        self.repo.count_anonymous_notes()
    }

    pub fn import_anonymous_notes_to_current_account(&self) -> Result<Vec<snapline_domain::NoteSummary>> {
        let account_id = self
            .current_account_id()?
            .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
        let imported_ids = self.repo.import_anonymous_notes(&account_id)?;
        for note_id in imported_ids {
            self.repo.delete_changes_for_note(None, &note_id)?;
            let note = self.repo.get_note(&note_id)?;
            self.enqueue_note_change(&note, SyncOpType::UpsertNote, 0)?;
        }
        self.repo.list_recent_for_owner(50, Some(&account_id))
    }

    pub fn pending_sync_changes(&self) -> Result<Vec<snapline_storage::ChangeQueueItem>> {
        let account_id = self
            .current_account_id()?
            .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
        self.repo.list_pending_changes(Some(&account_id), 100)
    }

    pub fn sync_state(&self) -> Result<snapline_storage::SyncState> {
        self.repo.get_or_create_sync_state()
    }

    pub fn sync_credentials(&self) -> Result<Option<(String, String, String)>> {
        let state = self.repo.get_or_create_sync_state()?;
        match (state.server_base_url, state.access_token) {
            (Some(base_url), Some(token)) => Ok(Some((base_url, token, state.device_id))),
            _ => Ok(None),
        }
    }

    pub fn delete_sync_change(&self, queue_id: &str) -> Result<()> {
        self.repo.delete_change(queue_id)
    }

    pub fn delete_sync_changes_for_note(&self, note_id: &NoteId) -> Result<()> {
        let account_id = self.current_account_id()?;
        self.repo
            .delete_changes_for_note(account_id.as_deref(), note_id)
    }

    pub fn mark_sync_change_failed(&self, queue_id: &str, error: &str) -> Result<()> {
        self.repo.mark_change_failed(queue_id, error)
    }

    pub fn update_note_server_version(&self, id: &NoteId, server_version: i64) -> Result<()> {
        self.repo.update_note_server_version(id, server_version)
    }

    pub fn apply_remote_note(&self, note: &Note) -> Result<()> {
        self.repo.apply_remote_note(note)
    }

    pub fn has_pending_note_change(&self, note_id: &NoteId) -> Result<bool> {
        let account_id = self.current_account_id()?;
        self.repo
            .has_pending_note_change(account_id.as_deref(), note_id)
    }

    pub fn create_conflict_copy(&self, note: &Note) -> Result<Note> {
        self.repo.create_conflict_copy(note, Utc::now())
    }

    pub fn import_snapshot(&self, notes: &[Note], cursor: i64) -> Result<()> {
        let account_id = self
            .current_account_id()?
            .ok_or_else(|| anyhow::anyhow!("not logged in"))?;
        for note in notes {
            if self
                .repo
                .has_pending_note_change(Some(&account_id), &note.id)?
            {
                let local_note = self.repo.get_note_for_owner(&note.id, Some(&account_id))?;
                self.repo.create_conflict_copy(&local_note, Utc::now())?;
                self.repo
                    .delete_changes_for_note(Some(&account_id), &note.id)?;
            }
            self.repo.apply_remote_note(note)?;
        }
        self.repo.update_sync_cursor_success(cursor, Utc::now())
    }

    pub fn update_sync_cursor_success(
        &self,
        cursor: i64,
        now: chrono::DateTime<Utc>,
    ) -> Result<()> {
        self.repo.update_sync_cursor_success(cursor, now)
    }

    pub(crate) fn enqueue_note_change(
        &self,
        note: &Note,
        op_type: SyncOpType,
        base_version: i64,
    ) -> Result<()> {
        let Some(account_id) = note.owner_account_id.as_deref() else {
            return Ok(());
        };
        let payload = SyncPayload::Note(NoteChangePayload::from_note(note));
        self.repo.enqueue_change(
            Some(account_id),
            &note.id,
            op_type,
            base_version,
            &payload,
            Utc::now(),
        )?;
        Ok(())
    }
}

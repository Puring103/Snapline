use anyhow::Result;
use chrono::Utc;
use snapline_domain::{crypto, Note, NoteChangePayload, NoteId, SyncOpType, SyncPayload};

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

    /// 若服务端返回了 `kek_salt` 和 `encrypted_dek`，则用密码派生 KEK、解包 DEK 并保存至内存。
    pub fn save_sync_login(
        &mut self,
        server_base_url: &str,
        account_id: &str,
        access_token: &str,
        password: Option<&str>,
        kek_salt: Option<&str>,
        encrypted_dek: Option<&str>,
    ) -> Result<SyncAccountState> {
        let mut state = self.repo.get_or_create_sync_state()?;
        state.server_base_url = Some(server_base_url.to_string());
        state.account_id = Some(account_id.to_string());
        state.access_token = Some(access_token.to_string());
        state.kek_salt = kek_salt.map(str::to_string);
        state.encrypted_dek = encrypted_dek.map(str::to_string);
        self.repo.save_sync_state(&state)?;
        if let (Some(pw), Some(salt_b64), Some(wrapped)) = (password, kek_salt, encrypted_dek) {
            let salt = crypto::decode_salt(salt_b64)?;
            let kek = crypto::derive_kek(pw, &salt)?;
            self.dek = Some(crypto::unwrap_dek(&kek, wrapped)?);
        }
        self.sync_account_state()
    }

    /// 注册新账户时生成 E2EE 材料，返回 `(kek_salt_b64, encrypted_dek_b64)`。
    pub fn generate_e2ee_material(&mut self, password: &str) -> Result<(String, String)> {
        let salt = crypto::generate_kek_salt();
        let kek = crypto::derive_kek(password, &salt)?;
        let dek = crypto::generate_dek();
        let wrapped = crypto::wrap_dek(&kek, &dek)?;
        self.dek = Some(dek);
        Ok((crypto::encode_salt(&salt), wrapped))
    }

    /// 返回当前内存中的 DEK（供 processor 使用）。
    pub fn dek(&self) -> Option<&[u8; 32]> {
        self.dek.as_ref()
    }

    pub(crate) fn current_account_id(&self) -> Result<Option<String>> {
        Ok(self.repo.get_or_create_sync_state()?.account_id)
    }

    pub fn anonymous_note_count(&self) -> Result<usize> {
        self.repo.count_anonymous_notes()
    }

    pub fn import_anonymous_notes_to_current_account(
        &self,
    ) -> Result<Vec<snapline_domain::NoteSummary>> {
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

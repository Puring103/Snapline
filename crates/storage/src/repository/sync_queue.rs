use crate::sync;
use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::params;
use snapline_domain::{NoteId, SyncOpType, SyncPayload};

use super::NoteRepository;

impl NoteRepository {
    pub fn enqueue_change(
        &self,
        account_id: Option<&str>,
        note_id: &NoteId,
        op_type: SyncOpType,
        base_version: i64,
        payload: &SyncPayload,
        queued_at: DateTime<Utc>,
    ) -> Result<String> {
        sync::enqueue_change(
            &self.conn,
            account_id,
            note_id,
            op_type,
            base_version,
            payload,
            queued_at,
        )
    }

    pub fn list_pending_changes(
        &self,
        account_id: Option<&str>,
        limit: usize,
    ) -> Result<Vec<sync::ChangeQueueItem>> {
        sync::list_pending_changes(&self.conn, account_id, limit)
    }

    pub fn has_pending_note_change(
        &self,
        account_id: Option<&str>,
        note_id: &NoteId,
    ) -> Result<bool> {
        let count: i64 = match account_id {
            Some(account_id) => self.conn.query_row(
                "SELECT COUNT(*) FROM change_queue WHERE account_id = ?1 AND note_id = ?2",
                params![account_id, note_id.to_string()],
                |row| row.get(0),
            )?,
            None => self.conn.query_row(
                "SELECT COUNT(*) FROM change_queue WHERE account_id IS NULL AND note_id = ?1",
                params![note_id.to_string()],
                |row| row.get(0),
            )?,
        };
        Ok(count > 0)
    }

    pub fn delete_change(&self, id: &str) -> Result<()> {
        sync::delete_change(&self.conn, id)
    }

    pub fn delete_changes_for_note(
        &self,
        account_id: Option<&str>,
        note_id: &NoteId,
    ) -> Result<()> {
        match account_id {
            Some(account_id) => {
                self.conn.execute(
                    "DELETE FROM change_queue WHERE account_id = ?1 AND note_id = ?2",
                    params![account_id, note_id.to_string()],
                )?;
            }
            None => {
                self.conn.execute(
                    "DELETE FROM change_queue WHERE account_id IS NULL AND note_id = ?1",
                    params![note_id.to_string()],
                )?;
            }
        }
        Ok(())
    }

    pub fn mark_change_failed(&self, id: &str, error: &str) -> Result<()> {
        sync::mark_change_failed(&self.conn, id, error)
    }

    pub fn get_or_create_sync_state(&self) -> Result<sync::SyncState> {
        sync::get_or_create_sync_state(&self.conn)
    }

    pub fn save_sync_state(&self, state: &sync::SyncState) -> Result<()> {
        sync::save_sync_state(&self.conn, state)
    }

    pub fn update_sync_cursor_success(&self, cursor: i64, now: DateTime<Utc>) -> Result<()> {
        let mut state = self.get_or_create_sync_state()?;
        state.server_cursor = cursor;
        state.last_sync_at = Some(now);
        state.last_success_at = Some(now);
        self.save_sync_state(&state)
    }
}

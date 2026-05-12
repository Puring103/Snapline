use anyhow::Result;
use chrono::Utc;
use serde::Serialize;
use snapline_domain::{summarize_note, Note, NoteId, NoteSummary, SyncOpType};

use crate::AppCore;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SaveDraftSessionResult {
    pub note: Option<Note>,
    pub skipped: bool,
}

impl AppCore {
    pub fn create_note(&self) -> Result<Note> {
        let owner = self.current_account_id()?;
        self.repo.create_note(Utc::now(), owner.as_deref())
    }

    pub fn get_note(&self, id: &NoteId) -> Result<Note> {
        let owner = self.current_account_id()?;
        self.repo.get_note_for_owner(id, owner.as_deref())
    }

    pub fn get_note_summary(&self, id: &NoteId) -> Result<NoteSummary> {
        let note = self.get_note(id)?;
        Ok(summarize_note(&note))
    }

    pub fn search_notes(&self, query: &str) -> Result<Vec<NoteSummary>> {
        let owner = self.current_account_id()?;
        self.repo
            .search_notes_for_owner(query, 50, owner.as_deref())
    }

    pub fn list_notes(&self) -> Result<Vec<NoteSummary>> {
        let owner = self.current_account_id()?;
        self.repo.list_recent_for_owner(50, owner.as_deref())
    }

    pub fn save_note(
        &self,
        id: &NoteId,
        title: &str,
        content_md: &str,
        pinned: bool,
    ) -> Result<Note> {
        let owner = self.current_account_id()?;
        if self.repo.note_exists(id)? {
            self.repo.get_note_for_owner(id, owner.as_deref())?;
        }
        let note =
            self.repo
                .save_note(id, title, content_md, pinned, Utc::now(), owner.as_deref())?;
        self.enqueue_note_change(&note, SyncOpType::UpsertNote, note.server_version)?;
        Ok(note)
    }

    pub fn save_draft_session(
        &self,
        id: Option<NoteId>,
        title: &str,
        body_md: &str,
        pinned: bool,
    ) -> Result<SaveDraftSessionResult> {
        if !self.has_meaningful_draft_content(title, body_md) {
            return Ok(SaveDraftSessionResult {
                note: None,
                skipped: true,
            });
        }

        let prepared = self.prepare_draft_for_save(title, body_md);
        let note_id = match id {
            Some(id) => id,
            None => self.create_note()?.id,
        };
        let note = self.save_note(&note_id, &prepared.title, &prepared.body_md, pinned)?;

        Ok(SaveDraftSessionResult {
            note: Some(note),
            skipped: false,
        })
    }

    pub fn set_note_title(&self, id: &NoteId, title: &str) -> Result<Note> {
        self.get_note(id)?;
        let note = self.repo.update_note_title(id, title, Utc::now())?;
        self.enqueue_note_change(&note, SyncOpType::UpsertNote, note.server_version)?;
        Ok(note)
    }

    pub fn set_note_pinned(&self, id: &NoteId, pinned: bool) -> Result<Note> {
        self.get_note(id)?;
        let note = self.repo.set_pinned(id, pinned, Utc::now())?;
        self.enqueue_note_change(&note, SyncOpType::UpsertNote, note.server_version)?;
        Ok(note)
    }

    pub fn delete_note(&self, id: &NoteId) -> Result<Vec<NoteSummary>> {
        let owner = self.current_account_id()?;
        let existing = self.repo.get_note_for_owner(id, owner.as_deref())?;
        self.repo.soft_delete(id, Utc::now())?;
        let deleted = self.repo.get_note_for_owner(id, owner.as_deref())?;
        self.enqueue_note_change(&deleted, SyncOpType::DeleteNote, existing.server_version)?;
        self.repo.list_recent_for_owner(50, owner.as_deref())
    }
}

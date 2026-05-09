use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use snapline_domain::{Note, NoteSummary};

use crate::AppCore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapState {
    pub notes: Vec<NoteSummary>,
    pub current: Note,
    pub data_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncAccountState {
    pub account_id: Option<String>,
    pub device_id: String,
    pub server_base_url: Option<String>,
    pub is_logged_in: bool,
}

impl AppCore {
    pub fn bootstrap(&self) -> Result<BootstrapState> {
        let owner = self.current_account_id()?;
        let notes = self.repo.list_recent_for_owner(50, owner.as_deref())?;
        let current = if let Some(note) = notes.first() {
            self.repo.get_note_for_owner(&note.id, owner.as_deref())?
        } else {
            Note::draft(Utc::now())
        };
        Ok(BootstrapState {
            notes,
            current,
            data_dir: self.paths.data_dir.to_string_lossy().to_string(),
        })
    }
}

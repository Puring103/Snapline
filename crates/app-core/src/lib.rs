use anyhow::{bail, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use snapline_domain::{AssetId, AssetRef, Note, NoteId, NoteSummary};
use snapline_platform::AppPaths;
use snapline_storage::NoteRepository;
use std::fs;

pub struct AppCore {
    repo: NoteRepository,
    paths: AppPaths,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapState {
    pub notes: Vec<NoteSummary>,
    pub current: Note,
}

impl AppCore {
    pub fn open(paths: AppPaths) -> Result<Self> {
        fs::create_dir_all(&paths.data_dir)?;
        let repo = NoteRepository::open(&paths.db_path)?;
        Ok(Self { repo, paths })
    }

    pub fn with_repo(paths: AppPaths, repo: NoteRepository) -> Self {
        Self { repo, paths }
    }

    pub fn bootstrap(&self) -> Result<BootstrapState> {
        let mut notes = self.repo.list_recent(50)?;
        let current = if let Some(first) = notes.first() {
            self.repo.get_note(&first.id)?
        } else {
            self.repo.create_note(Utc::now())?
        };
        notes = self.repo.list_recent(50)?;
        Ok(BootstrapState { notes, current })
    }

    pub fn create_note(&self) -> Result<Note> {
        self.repo.create_note(Utc::now())
    }

    pub fn get_note(&self, id: &NoteId) -> Result<Note> {
        self.repo.get_note(id)
    }

    pub fn save_note(&self, id: &NoteId, content_md: &str) -> Result<Note> {
        self.repo.update_note_content(id, content_md, Utc::now())
    }

    pub fn delete_note(&self, id: &NoteId) -> Result<Vec<NoteSummary>> {
        self.repo.soft_delete(id, Utc::now())?;
        self.repo.list_recent(50)
    }

    pub fn save_png_asset(&self, note_id: &NoteId, png_bytes: &[u8]) -> Result<AssetRef> {
        if png_bytes.is_empty() {
            bail!("image bytes are empty");
        }
        let asset_id = AssetId::new();
        let dir = self.paths.note_asset_dir(note_id);
        fs::create_dir_all(&dir)?;
        let path = self.paths.note_asset_path(note_id, &asset_id, "png");
        fs::write(path, png_bytes)?;
        Ok(AssetRef {
            markdown_path: self.paths.markdown_asset_path(note_id, &asset_id, "png"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::AppCore;
    use snapline_platform::AppPaths;
    use snapline_storage::NoteRepository;

    #[test]
    fn bootstrap_creates_first_note() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);

        let state = core.bootstrap().unwrap();

        assert_eq!(state.notes.len(), 1);
        assert_eq!(state.current.title, "Untitled");
    }

    #[test]
    fn saves_png_asset_under_note_directory() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);
        let note = core.create_note().unwrap();

        let asset = core.save_png_asset(&note.id, &[137, 80, 78, 71]).unwrap();

        assert!(asset.markdown_path.starts_with(&format!("assets/notes/{}/", note.id)));
        assert!(dir.path().join(&asset.markdown_path).exists());
    }
}

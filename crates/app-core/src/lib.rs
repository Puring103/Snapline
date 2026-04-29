use anyhow::{bail, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use snapline_domain::{AssetId, AssetRef, Note, NoteId, NoteSummary};
use snapline_platform::AppPaths;
use snapline_storage::NoteRepository;
use std::fs;

const OPEN_SHORTCUT_KEY: &str = "open_shortcut";
const DEFAULT_OPEN_SHORTCUT: &str = "Ctrl+Shift+Space";

pub struct AppCore {
    repo: NoteRepository,
    paths: AppPaths,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapState {
    pub notes: Vec<NoteSummary>,
    pub current: Note,
    pub data_dir: String,
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
        let notes = self.repo.list_recent(50)?;
        let current = Note::draft(Utc::now());
        Ok(BootstrapState {
            notes,
            current,
            data_dir: self.paths.data_dir.to_string_lossy().to_string(),
        })
    }

    pub fn create_note(&self) -> Result<Note> {
        Ok(Note::draft(Utc::now()))
    }

    pub fn get_note(&self, id: &NoteId) -> Result<Note> {
        self.repo.get_note(id)
    }

    pub fn save_note(&self, id: &NoteId, title: &str, content_md: &str, pinned: bool) -> Result<Note> {
        self.repo.save_note(id, title, content_md, pinned, Utc::now())
    }

    pub fn set_note_title(&self, id: &NoteId, title: &str) -> Result<Note> {
        self.repo.update_note_title(id, title, Utc::now())
    }

    pub fn set_note_pinned(&self, id: &NoteId, pinned: bool) -> Result<Note> {
        self.repo.set_pinned(id, pinned, Utc::now())
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
        let markdown_path = self.paths.markdown_asset_path(note_id, &asset_id, "png");
        Ok(AssetRef {
            markdown_path: markdown_path.clone(),
            filesystem_path: self
                .paths
                .note_asset_path(note_id, &asset_id, "png")
                .to_string_lossy()
                .to_string(),
            asset_url: self.paths.markdown_asset_url(&markdown_path),
        })
    }

    pub fn resolve_asset_url(&self, markdown_path: &str) -> String {
        self.paths.markdown_asset_url(markdown_path)
    }

    pub fn get_open_shortcut(&self) -> Result<String> {
        Ok(self
            .repo
            .get_setting(OPEN_SHORTCUT_KEY)?
            .unwrap_or_else(|| DEFAULT_OPEN_SHORTCUT.to_string()))
    }

    pub fn set_open_shortcut(&self, shortcut: &str) -> Result<()> {
        self.repo.set_setting(OPEN_SHORTCUT_KEY, Some(shortcut))
    }
}

#[cfg(test)]
mod tests {
    use super::AppCore;
    use snapline_platform::AppPaths;
    use snapline_storage::NoteRepository;

    #[test]
    fn bootstrap_starts_with_a_blank_draft_note() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);

        let state = core.bootstrap().unwrap();

        assert!(state.notes.is_empty());
        assert_eq!(state.current.title, "Untitled");
        assert!(!state.current.pinned);
    }

    #[test]
    fn bootstrap_does_not_persist_a_blank_draft_note() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let core = AppCore::open(AppPaths::from_data_dir(dir.path())).unwrap();

        let state = core.bootstrap().unwrap();
        let repo = NoteRepository::open(&paths.db_path).unwrap();

        assert_eq!(state.current.title, "Untitled");
        assert!(repo.list_recent(10).unwrap().is_empty());
    }

    #[test]
    fn stores_and_loads_open_shortcut() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let core = AppCore::open(AppPaths::from_data_dir(dir.path())).unwrap();

        core.set_open_shortcut("Ctrl+Alt+S").unwrap();

        let reopened = AppCore::open(paths).unwrap();
        assert_eq!(reopened.get_open_shortcut().unwrap(), "Ctrl+Alt+S");
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
        assert!(asset.asset_url.starts_with("asset://localhost/"));
    }

    #[test]
    fn resolves_asset_urls_without_frontend_path_api() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);

        let resolved = core.resolve_asset_url("assets/notes/example/image.png");
        assert!(resolved.starts_with("asset://localhost/"));
        assert!(resolved.ends_with("image.png"));
    }
}

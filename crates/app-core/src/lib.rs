use anyhow::{bail, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use snapline_domain::{
    AssetId, AssetMetadata, AssetRef, AssetUploadPayload, Note, NoteChangePayload, NoteId,
    NoteSummary, SyncOpType, SyncPayload,
};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncAccountState {
    pub account_id: Option<String>,
    pub device_id: String,
    pub server_base_url: Option<String>,
    pub is_logged_in: bool,
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
        let owner = self.current_account_id()?;
        let notes = self.repo.list_recent_for_owner(50, owner.as_deref())?;
        let current = Note::draft(Utc::now());
        Ok(BootstrapState {
            notes,
            current,
            data_dir: self.paths.data_dir.to_string_lossy().to_string(),
        })
    }

    pub fn create_note(&self) -> Result<Note> {
        let owner = self.current_account_id()?;
        self.repo.create_note(Utc::now(), owner.as_deref())
    }

    pub fn get_note(&self, id: &NoteId) -> Result<Note> {
        let owner = self.current_account_id()?;
        self.repo.get_note_for_owner(id, owner.as_deref())
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

    pub fn save_png_asset(&self, note_id: &NoteId, png_bytes: &[u8]) -> Result<AssetRef> {
        if png_bytes.is_empty() {
            bail!("image bytes are empty");
        }
        let note = self.get_note(note_id)?;
        let asset_id = AssetId::new();
        let dir = self.paths.note_asset_dir(note_id);
        fs::create_dir_all(&dir)?;
        let path = self.paths.note_asset_path(note_id, &asset_id, "png");
        fs::write(path, png_bytes)?;
        let markdown_path = self.paths.markdown_asset_path(note_id, &asset_id, "png");
        let mut hasher = Sha256::new();
        hasher.update(png_bytes);
        let sha256 = format!("{:x}", hasher.finalize());
        let payload = SyncPayload::Asset(AssetUploadPayload {
            asset_id: asset_id.clone(),
            note_id: note_id.clone(),
            content_type: "image/png".to_string(),
            byte_size: png_bytes.len() as i64,
            sha256,
            markdown_path: markdown_path.clone(),
        });
        if let Some(account_id) = note.owner_account_id.as_deref() {
            self.repo.enqueue_change(
                Some(account_id),
                note_id,
                SyncOpType::AssetUpload,
                0,
                &payload,
                Utc::now(),
            )?;
        }
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

    pub fn resolve_asset_path(&self, markdown_path: &str) -> std::path::PathBuf {
        self.paths.resolve_markdown_asset_path(markdown_path)
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

    fn current_account_id(&self) -> Result<Option<String>> {
        Ok(self.repo.get_or_create_sync_state()?.account_id)
    }

    pub fn anonymous_note_count(&self) -> Result<usize> {
        self.repo.count_anonymous_notes()
    }

    pub fn import_anonymous_notes_to_current_account(&self) -> Result<Vec<NoteSummary>> {
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

    pub fn data_dir(&self) -> &std::path::Path {
        &self.paths.data_dir
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

    pub fn missing_asset_metadata(&self, assets: &[AssetMetadata]) -> Vec<AssetMetadata> {
        assets
            .iter()
            .filter(|asset| {
                let path = self.paths.markdown_asset_path(
                    &asset.note_id,
                    &asset.id,
                    asset_extension(asset),
                );
                !self.paths.resolve_markdown_asset_path(&path).exists()
            })
            .cloned()
            .collect()
    }

    pub fn save_remote_asset(&self, asset: &AssetMetadata, bytes: &[u8]) -> Result<()> {
        let extension = asset_extension(asset);
        let path = self
            .paths
            .note_asset_path(&asset.note_id, &asset.id, extension);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, bytes)?;
        Ok(())
    }

    pub fn update_sync_cursor_success(
        &self,
        cursor: i64,
        now: chrono::DateTime<Utc>,
    ) -> Result<()> {
        self.repo.update_sync_cursor_success(cursor, now)
    }

    fn enqueue_note_change(
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

fn asset_extension(asset: &AssetMetadata) -> &str {
    match asset.content_type.as_str() {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/webp" => "webp",
        _ => "bin",
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

        assert!(asset
            .markdown_path
            .starts_with(&format!("assets/notes/{}/", note.id)));
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

    #[test]
    fn anonymous_save_does_not_enqueue_sync_change() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);
        let note = core.create_note().unwrap();

        core.save_note(&note.id, "Title", "# Title", false).unwrap();

        assert!(core.pending_sync_changes().is_err());
    }

    #[test]
    fn account_save_enqueues_upsert_change() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);
        core.save_sync_login("http://localhost:8080", "acct_a", "token")
            .unwrap();
        let note = core.create_note().unwrap();

        core.save_note(&note.id, "Title", "# Title", false).unwrap();

        let changes = core.pending_sync_changes().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op_type, snapline_domain::SyncOpType::UpsertNote);
        assert_eq!(changes[0].account_id.as_deref(), Some("acct_a"));
    }

    #[test]
    fn set_note_title_enqueues_upsert_change() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);
        core.save_sync_login("http://localhost:8080", "acct_a", "token")
            .unwrap();
        let note = core.create_note().unwrap();
        core.save_note(&note.id, "Title", "# Title", false).unwrap();
        for change in core.pending_sync_changes().unwrap() {
            core.delete_sync_change(&change.id).unwrap();
        }

        core.set_note_title(&note.id, "Renamed").unwrap();

        let changes = core.pending_sync_changes().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op_type, snapline_domain::SyncOpType::UpsertNote);
        assert_eq!(changes[0].base_version, 0);
    }

    #[test]
    fn save_png_asset_enqueues_asset_upload() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);
        core.save_sync_login("http://localhost:8080", "acct_a", "token")
            .unwrap();
        let note = core.create_note().unwrap();

        core.save_png_asset(&note.id, &[137, 80, 78, 71]).unwrap();

        let changes = core.pending_sync_changes().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].op_type, snapline_domain::SyncOpType::AssetUpload);
    }

    #[test]
    fn import_snapshot_applies_notes_and_updates_cursor() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);
        core.save_sync_login("http://localhost:8080", "acct_a", "token")
            .unwrap();
        let mut note = snapline_domain::Note::draft(chrono::Utc::now());
        note.owner_account_id = Some("acct_a".to_string());
        note.title = "Remote".to_string();
        note.server_version = 4;

        core.import_snapshot(&[note.clone()], 9).unwrap();

        assert_eq!(core.get_note(&note.id).unwrap().title, "Remote");
        assert_eq!(core.sync_state().unwrap().server_cursor, 9);
    }

    #[test]
    fn import_snapshot_creates_conflict_copy_for_pending_local_changes() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);
        core.save_sync_login("http://localhost:8080", "acct_a", "token")
            .unwrap();
        let note = core.create_note().unwrap();
        core.save_note(&note.id, "Local", "# Local", false).unwrap();
        let mut remote_note = note.clone();
        remote_note.title = "Remote".to_string();
        remote_note.content_md = "# Remote".to_string();
        remote_note.server_version = 3;

        core.import_snapshot(&[remote_note], 9).unwrap();

        assert_eq!(core.get_note(&note.id).unwrap().title, "Remote");
        assert!(core
            .bootstrap()
            .unwrap()
            .notes
            .iter()
            .any(|note| note.is_conflict_copy));
        assert!(core.pending_sync_changes().unwrap().is_empty());
        assert_eq!(core.sync_state().unwrap().server_cursor, 9);
    }

    #[test]
    fn save_remote_asset_uses_metadata_location() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);
        let note_id = snapline_domain::NoteId::new();
        let asset = snapline_domain::AssetMetadata {
            id: snapline_domain::AssetId::new(),
            note_id: note_id.clone(),
            content_type: "image/png".to_string(),
            byte_size: 4,
            sha256: "sha".to_string(),
            storage_key: "server/key".to_string(),
            created_at: chrono::Utc::now(),
            deleted_at: None,
        };

        core.save_remote_asset(&asset, &[1, 2, 3, 4]).unwrap();

        assert_eq!(
            std::fs::read(
                dir.path()
                    .join(core.paths.markdown_asset_path(&note_id, &asset.id, "png"))
            )
            .unwrap(),
            vec![1, 2, 3, 4]
        );
    }

    #[test]
    fn bootstrap_shows_local_notes_when_logged_out_and_account_notes_when_logged_in() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);

        let local = core.create_note().unwrap();
        core.save_note(&local.id, "Local", "Local", false).unwrap();
        assert_eq!(core.bootstrap().unwrap().notes.len(), 1);

        core.save_sync_login("http://localhost:8080", "acct_a", "token")
            .unwrap();
        assert!(core.bootstrap().unwrap().notes.is_empty());

        core.import_anonymous_notes_to_current_account().unwrap();
        assert_eq!(core.bootstrap().unwrap().notes.len(), 1);
    }

    #[test]
    fn importing_anonymous_notes_enqueues_upserts_for_current_account() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);

        let local = core.create_note().unwrap();
        core.save_note(&local.id, "Local", "Local", false).unwrap();
        core.save_sync_login("http://localhost:8080", "acct_a", "token")
            .unwrap();
        core.import_anonymous_notes_to_current_account().unwrap();

        let changes = core.pending_sync_changes().unwrap();
        assert!(changes
            .iter()
            .all(|item| item.account_id.as_deref() == Some("acct_a")));
        assert!(changes.iter().any(|item| item.note_id == local.id));
    }

    #[test]
    fn importing_anonymous_notes_removes_old_anonymous_queue_rows() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);

        let local = core.create_note().unwrap();
        core.save_note(&local.id, "Local", "Local", false).unwrap();
        let payload = snapline_domain::SyncPayload::Note(
            snapline_domain::NoteChangePayload::from_note(&local),
        );
        core.repo
            .enqueue_change(
                None,
                &local.id,
                snapline_domain::SyncOpType::UpsertNote,
                0,
                &payload,
                chrono::Utc::now(),
            )
            .unwrap();

        core.save_sync_login("http://localhost:8080", "acct_a", "token")
            .unwrap();
        core.import_anonymous_notes_to_current_account().unwrap();

        assert!(core.repo.list_pending_changes(None, 10).unwrap().is_empty());
        let changes = core.pending_sync_changes().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].account_id.as_deref(), Some("acct_a"));
    }

    #[test]
    fn logged_in_account_cannot_modify_anonymous_note_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(dir.path());
        let repo = NoteRepository::open_in_memory().unwrap();
        let core = AppCore::with_repo(paths, repo);

        let local = core.create_note().unwrap();
        core.save_note(&local.id, "Local", "Local", false).unwrap();
        core.save_sync_login("http://localhost:8080", "acct_a", "token")
            .unwrap();

        assert!(core
            .save_note(&local.id, "Account edit", "Account edit", false)
            .is_err());
        assert!(core.set_note_pinned(&local.id, true).is_err());
        assert!(core.delete_note(&local.id).is_err());
        assert!(core.save_png_asset(&local.id, &[137, 80, 78, 71]).is_err());
    }
}
